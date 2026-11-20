//! Parallel HTTP/3 (QUIC) listener.
//!
//! Pingora 0.9 has no native H3. This module terminates QUIC with the shared
//! [`CertStore`] and proxies each request to the local Pingora HTTP listener,
//! so the cache / WAF / origin pipeline is reused.
//!
//! The loopback hop must not change who the request appears to come from:
//! - Every client-supplied `x-lagran-*` and forwarding header is dropped, so a
//!   visitor cannot claim to be an internal prefetch (which skips hotlink and
//!   signed-URL checks) or spoof `X-Forwarded-For`.
//! - The real client address and the fact that the request arrived over TLS
//!   are passed in [`BRIDGE_CLIENT_HEADER`], authenticated by a random
//!   per-process secret ([`BRIDGE_SECRET_HEADER`]). Pingora honours them only
//!   from a loopback peer with the right secret (see [`bridge_client`]).
//! - Bodies are streamed both ways; nothing is buffered whole.

use crate::config::registry::DomainRegistry;
use crate::server::cert_store::CertStore;
use anyhow::{Context, Result};
use bytes::{Buf, Bytes};
use http_body_util::BodyExt;
use hyper::body::{Body, Frame, SizeHint};
use hyper::Request;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;
use quinn::crypto::rustls::QuicServerConfig;
use rustls::ServerConfig;
use std::net::{IpAddr, SocketAddr};
use std::pin::Pin;
use std::sync::{Arc, OnceLock};
use std::task::{Context as TaskContext, Poll};
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

/// Secret proving a request came from this process's H3 bridge.
pub const BRIDGE_SECRET_HEADER: &str = "x-lagran-h3-secret";
/// Real client address of a bridged request.
pub const BRIDGE_CLIENT_HEADER: &str = "x-lagran-h3-client";

/// Time allowed for Pingora to start answering a bridged request.
const UPSTREAM_HEADER_TIMEOUT: Duration = Duration::from_secs(120);

fn bridge_secret() -> &'static str {
    static SECRET: OnceLock<String> = OnceLock::new();
    SECRET.get_or_init(|| {
        let mut bytes = [0u8; 32];
        if ring::rand::SecureRandom::fill(&ring::rand::SystemRandom::new(), &mut bytes).is_err() {
            // Without randomness the bridge cannot be authenticated; an empty
            // secret makes `bridge_client` reject everything.
            return String::new();
        }
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    })
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// Client address of a request bridged from the H3 listener, or `None` when the
/// request did not come from the bridge (wrong peer, missing / wrong secret).
pub fn bridge_client(peer: Option<&SocketAddr>, headers: &http::HeaderMap) -> Option<IpAddr> {
    let peer = peer?;
    if !peer.ip().is_loopback() {
        return None;
    }
    let secret = bridge_secret();
    if secret.is_empty() {
        return None;
    }
    let presented = headers.get(BRIDGE_SECRET_HEADER)?.as_bytes();
    if !constant_time_eq(presented, secret.as_bytes()) {
        return None;
    }
    headers
        .get(BRIDGE_CLIENT_HEADER)?
        .to_str()
        .ok()?
        .trim()
        .parse()
        .ok()
}

static ADVERTISED_PORT: OnceLock<u16> = OnceLock::new();

/// `Alt-Svc` value for responses, only when the QUIC listener is running.
pub fn alt_svc_value() -> Option<String> {
    ADVERTISED_PORT
        .get()
        .map(|port| format!("h3=\":{port}\"; ma=86400"))
}

/// Request body fed from the QUIC stream through a channel.
struct ChannelBody {
    rx: Option<mpsc::Receiver<Result<Bytes, std::io::Error>>>,
    /// Exact length when the client sent `Content-Length`.
    exact: Option<u64>,
}

impl Body for ChannelBody {
    type Data = Bytes;
    type Error = std::io::Error;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        cx: &mut TaskContext<'_>,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        let Some(rx) = self.rx.as_mut() else {
            return Poll::Ready(None);
        };
        match rx.poll_recv(cx) {
            Poll::Ready(Some(Ok(data))) => Poll::Ready(Some(Ok(Frame::data(data)))),
            Poll::Ready(Some(Err(e))) => Poll::Ready(Some(Err(e))),
            Poll::Ready(None) => {
                self.rx = None;
                Poll::Ready(None)
            }
            Poll::Pending => Poll::Pending,
        }
    }

    fn is_end_stream(&self) -> bool {
        self.rx.is_none() || self.exact == Some(0)
    }

    fn size_hint(&self) -> SizeHint {
        match self.exact {
            Some(n) => SizeHint::with_exact(n),
            None => SizeHint::default(),
        }
    }
}

type UpstreamClient = Client<HttpConnector, ChannelBody>;

/// Spawn the H3 accept loop. Returns immediately; work runs on the Tokio runtime.
pub fn spawn(
    listen: SocketAddr,
    certs: Arc<CertStore>,
    domains: DomainRegistry,
    upstream: SocketAddr,
) -> Result<()> {
    let server_crypto = build_rustls_server(certs.clone())?;
    let quic_config = QuicServerConfig::try_from(server_crypto)
        .map_err(|e| anyhow::anyhow!("quic server config: {e}"))?;
    let mut transport = quinn::TransportConfig::default();
    transport.max_concurrent_bidi_streams(256u32.into());
    let mut server_config = quinn::ServerConfig::with_crypto(Arc::new(quic_config));
    server_config.transport = Arc::new(transport);

    let endpoint = quinn::Endpoint::server(server_config, listen)
        .with_context(|| format!("bind H3 UDP {listen}"))?;
    let _ = ADVERTISED_PORT.set(listen.port());
    info!(listen = %listen, upstream = %upstream, "HTTP/3 QUIC listener started");

    let http: UpstreamClient = Client::builder(TokioExecutor::new()).build(HttpConnector::new());

    tokio::spawn(async move {
        loop {
            let incoming = endpoint.accept().await;
            let Some(connecting) = incoming else {
                break;
            };
            let domains = domains.clone();
            let http = http.clone();
            tokio::spawn(async move {
                if let Err(e) = handle_connection(connecting, domains, http, upstream).await {
                    debug!(error = %e, "h3 connection closed");
                }
            });
        }
        warn!("HTTP/3 endpoint closed");
    });
    Ok(())
}

fn build_rustls_server(certs: Arc<CertStore>) -> Result<ServerConfig> {
    // Explicit provider: the binary links both ring and aws-lc-rs, and the
    // process default would panic when none was installed yet.
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let mut cfg = ServerConfig::builder_with_provider(provider)
        .with_protocol_versions(&[&rustls::version::TLS13])
        .map_err(|e| anyhow::anyhow!("h3 TLS versions: {e}"))?
        .with_no_client_auth()
        .with_cert_resolver(certs.resolver());
    cfg.alpn_protocols = vec![b"h3".to_vec()];
    // No 0-RTT: early data can be replayed, and requests (POST included) are
    // forwarded to origins as-is.
    cfg.max_early_data_size = 0;
    Ok(cfg)
}

async fn handle_connection(
    connecting: quinn::Incoming,
    domains: DomainRegistry,
    http: UpstreamClient,
    upstream: SocketAddr,
) -> Result<()> {
    let conn = connecting.await.context("h3 accept")?;
    let client_ip = conn.remote_address().ip();
    let h3_conn = h3_quinn::Connection::new(conn);
    let mut h3 = h3::server::Connection::new(h3_conn)
        .await
        .context("h3 handshake")?;

    loop {
        match h3.accept().await {
            Ok(Some(resolver)) => {
                let domains = domains.clone();
                let http = http.clone();
                tokio::spawn(async move {
                    if let Err(e) = handle_request(resolver, domains, http, upstream, client_ip).await
                    {
                        debug!(error = %e, "h3 request failed");
                    }
                });
            }
            Ok(None) => break,
            Err(e) => {
                debug!(error = %e, "h3 accept stream ended");
                break;
            }
        }
    }
    Ok(())
}

/// Headers a client must never be able to set on the bridged request.
fn is_stripped_request_header(name: &str) -> bool {
    name.starts_with("x-lagran-")
        || matches!(
            name,
            "host"
                | "x-forwarded-for"
                | "x-forwarded-proto"
                | "x-forwarded-host"
                | "x-real-ip"
                | "forwarded"
                | "connection"
                | "keep-alive"
                | "proxy-connection"
                | "transfer-encoding"
                | "upgrade"
                | "te"
        )
}

async fn handle_request(
    resolver: h3::server::RequestResolver<h3_quinn::Connection, Bytes>,
    domains: DomainRegistry,
    http: UpstreamClient,
    upstream: SocketAddr,
    client_ip: IpAddr,
) -> Result<()> {
    let (req, stream) = resolver.resolve_request().await.context("resolve h3 req")?;
    let (mut send, mut recv) = stream.split();
    let host = req
        .uri()
        .host()
        .map(str::to_string)
        .or_else(|| {
            req.headers()
                .get(http::header::HOST)
                .and_then(|v| v.to_str().ok())
                .map(|s| s.split(':').next().unwrap_or(s).to_string())
        })
        .unwrap_or_default();

    let (_, domain) = domains.lookup(&host).await;
    if domain.as_ref().is_some_and(|d| !d.http3_enabled) {
        let resp = http::Response::builder()
            .status(421)
            .header(http::header::CONTENT_TYPE, "text/plain; charset=utf-8")
            .body(())
            .context("build 421")?;
        send.send_response(resp).await.context("send 421")?;
        send.send_data(Bytes::from_static(b"HTTP/3 is disabled for this site"))
            .await
            .ok();
        send.finish().await.ok();
        return Ok(());
    }

    let exact = req
        .headers()
        .get(http::header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.trim().parse::<u64>().ok())
        .or_else(|| {
            // GET/HEAD/OPTIONS without a length carry no body.
            matches!(
                *req.method(),
                http::Method::GET | http::Method::HEAD | http::Method::OPTIONS
            )
            .then_some(0)
        });

    // Pump the request body into the upstream request as it arrives.
    let (tx, rx) = mpsc::channel::<Result<Bytes, std::io::Error>>(8);
    if exact != Some(0) {
        tokio::spawn(async move {
            loop {
                match recv.recv_data().await {
                    Ok(Some(mut chunk)) => {
                        let len = chunk.remaining();
                        if tx.send(Ok(chunk.copy_to_bytes(len))).await.is_err() {
                            break;
                        }
                    }
                    Ok(None) => break,
                    Err(e) => {
                        let _ = tx.send(Err(std::io::Error::other(e.to_string()))).await;
                        break;
                    }
                }
            }
        });
    } else {
        drop(tx);
    }
    let body = ChannelBody { rx: Some(rx), exact };

    let path_and_query = req
        .uri()
        .path_and_query()
        .map(|pq| pq.as_str())
        .unwrap_or("/");
    let upstream_uri = format!("http://{upstream}{path_and_query}");
    let mut builder = Request::builder()
        .method(req.method().clone())
        .uri(&upstream_uri);
    for (name, value) in req.headers().iter() {
        if is_stripped_request_header(name.as_str()) {
            continue;
        }
        builder = builder.header(name, value);
    }
    if !host.is_empty() {
        builder = builder.header(http::header::HOST, &host);
    }
    builder = builder
        .header(BRIDGE_SECRET_HEADER, bridge_secret())
        .header(BRIDGE_CLIENT_HEADER, client_ip.to_string());
    let upstream_req = builder.body(body).context("build upstream req")?;

    let upstream_resp = tokio::time::timeout(UPSTREAM_HEADER_TIMEOUT, http.request(upstream_req))
        .await
        .context("pingora response timeout")?
        .context("proxy to pingora")?;
    let (parts, mut body) = upstream_resp.into_parts();

    let mut resp = http::Response::builder().status(parts.status);
    for (name, value) in parts.headers.iter() {
        // Hop-by-hop / framing headers are not valid on H3.
        if matches!(
            name.as_str(),
            "connection"
                | "keep-alive"
                | "proxy-connection"
                | "transfer-encoding"
                | "upgrade"
                | "http2-settings"
        ) {
            continue;
        }
        resp = resp.header(name, value);
    }
    let resp = resp.body(()).context("build h3 response")?;
    send.send_response(resp).await.context("send h3 headers")?;
    while let Some(frame) = body.frame().await {
        let frame = frame.context("read pingora body")?;
        if let Ok(data) = frame.into_data() {
            if !data.is_empty() {
                send.send_data(data).await.context("send h3 body")?;
            }
        }
    }
    send.finish().await.context("finish h3")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers(secret: &str, client: &str) -> http::HeaderMap {
        let mut h = http::HeaderMap::new();
        h.insert(BRIDGE_SECRET_HEADER, secret.parse().unwrap());
        h.insert(BRIDGE_CLIENT_HEADER, client.parse().unwrap());
        h
    }

    #[test]
    fn bridge_identity_needs_loopback_peer_and_secret() {
        let loopback: SocketAddr = "127.0.0.1:40000".parse().unwrap();
        let remote: SocketAddr = "203.0.113.9:40000".parse().unwrap();
        let good = headers(bridge_secret(), "198.51.100.7");
        assert_eq!(
            bridge_client(Some(&loopback), &good),
            Some("198.51.100.7".parse().unwrap())
        );
        // Same headers from a non-loopback peer: ignored.
        assert_eq!(bridge_client(Some(&remote), &good), None);
        // Wrong secret from loopback: ignored.
        assert_eq!(bridge_client(Some(&loopback), &headers("guess", "1.2.3.4")), None);
        assert_eq!(bridge_client(None, &good), None);
    }

    #[test]
    fn client_controlled_internal_headers_are_stripped() {
        for name in [
            "x-lagran-prefetch",
            "x-lagran-h3-secret",
            "x-lagran-h3-client",
            "x-forwarded-for",
            "x-real-ip",
            "forwarded",
            "host",
        ] {
            assert!(is_stripped_request_header(name), "{name}");
        }
        assert!(!is_stripped_request_header("accept"));
        assert!(!is_stripped_request_header("cookie"));
    }
}
