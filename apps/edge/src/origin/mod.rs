//! Origin client — HTTP/HTTPS via hyper-rustls, with request method/body
//! forwarding and connection pooling.
//!
//! HTTP version follows [`protocol`]: HTTP/2 first (ALPN for `https`, h2c
//! for `http` origins a probe confirmed), HTTP/1.1 as the fallback.

pub mod protocol;

use crate::cache::policy::{compute_etag, VALIDATOR_SOURCE_HEADER};
use anyhow::{anyhow, Context, Result};
use axum::http::uri::Authority;
use axum::http::{header, HeaderMap, HeaderName, HeaderValue, Method, StatusCode, Uri};
use bytes::Bytes;
use dashmap::DashMap;
use http_body_util::{BodyExt, Full, Limited};
use hyper::body::Incoming;
use hyper::Request;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;
use lagran_cache::CacheEntry;
use protocol::{origin_key, H2Failure, OriginHttp, OriginProtocols, ProbeTarget};
use rustls::pki_types::ServerName;
use std::net::IpAddr;
use std::sync::Arc;
use std::task::{Context as TaskContext, Poll};
use std::time::Duration;
use tokio::sync::RwLock;
use tower::Service;

type HttpsConnector = hyper_rustls::HttpsConnector<HttpConnector>;
type TargetClient = Client<TargetConnector, Full<Bytes>>;

/// Receive windows for HTTP/2 origin connections, as on the Pingora path:
/// 8 MiB per stream (about one HTTP/1.1 socket buffer), and room for several
/// stalled streams before a slow client can hold the shared connection
/// window and block the other requests on it.
const H2_STREAM_WINDOW: u32 = 8 * 1024 * 1024;
const H2_CONNECTION_WINDOW: u32 = 64 * 1024 * 1024;

/// Connector that dials one origin whatever the request URI names.
///
/// The request URI carries the HTTP authority — over HTTP/2 it is sent as
/// `:authority`, which must name the site (RFC 9113 §8.3.1) — while the TCP
/// target and the TLS server name stay those of the configured origin.
#[derive(Clone)]
struct TargetConnector {
    inner: HttpsConnector,
    target: Uri,
}

impl Service<Uri> for TargetConnector {
    type Response = <HttpsConnector as Service<Uri>>::Response;
    type Error = <HttpsConnector as Service<Uri>>::Error;
    type Future = <HttpsConnector as Service<Uri>>::Future;

    fn poll_ready(&mut self, cx: &mut TaskContext<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, _authority: Uri) -> Self::Future {
        self.inner.call(self.target.clone())
    }
}

/// Where one origin base URL is dialled.
struct Dial {
    tls: bool,
    /// `scheme://host:port`.
    target: Uri,
    /// TLS server name when configured (SNI); `None` uses the URL host.
    server_name: Option<String>,
    host: String,
    port: u16,
    /// Key into [`OriginProtocols`].
    key: String,
}

impl Dial {
    fn new(base_url: &str, sni: Option<&str>) -> Result<Self> {
        let uri: Uri = base_url
            .trim()
            .parse()
            .with_context(|| format!("invalid origin url: {base_url}"))?;
        let tls = match uri.scheme_str() {
            Some("https") => true,
            Some("http") => false,
            _ => {
                return Err(anyhow!(
                    "origin url must start with http:// or https://: {base_url}"
                ))
            }
        };
        let host = uri
            .host()
            .filter(|host| !host.is_empty())
            .ok_or_else(|| anyhow!("origin url has no host: {base_url}"))?
            .to_string();
        let port = uri.port_u16().unwrap_or(if tls { 443 } else { 80 });
        let target: Uri = format!("{}://{host}:{port}", if tls { "https" } else { "http" })
            .parse()
            .with_context(|| format!("invalid origin url: {base_url}"))?;
        let server_name = if tls { sni.map(str::to_string) } else { None };
        let key = origin_key(tls, &host, port, server_name.as_deref());
        Ok(Self {
            tls,
            target,
            server_name,
            host,
            port,
            key,
        })
    }
}

#[derive(Clone, PartialEq, Eq, Hash)]
struct ClientKey {
    target: String,
    server_name: Option<String>,
    http: OriginHttp,
}

fn build_target_client(
    dial: &Dial,
    http: OriginHttp,
    connect_timeout: Duration,
) -> Result<TargetClient> {
    let mut tcp = HttpConnector::new();
    tcp.enforce_http(false);
    tcp.set_nodelay(true);
    tcp.set_connect_timeout(Some(connect_timeout));

    let tls = hyper_rustls::HttpsConnectorBuilder::new()
        .with_webpki_roots()
        .https_or_http();
    let tls = match &dial.server_name {
        Some(name) => {
            let name = ServerName::try_from(
                name.trim_start_matches('[')
                    .trim_end_matches(']')
                    .to_string(),
            )
            .map_err(|e| anyhow!("invalid TLS server name {name}: {e}"))?;
            tls.with_server_name_resolver(hyper_rustls::FixedServerNameResolver::new(name))
        }
        None => tls,
    };
    let connector = match http {
        // ALPN `h2, http/1.1`: the origin picks.
        OriginHttp::Alpn => tls.enable_http1().enable_http2().wrap_connector(tcp),
        // No ALPN offered: HTTP/1.1.
        OriginHttp::Http1 => tls.enable_http1().wrap_connector(tcp),
        // Cleartext target; the client speaks HTTP/2 with prior knowledge.
        OriginHttp::H2c => tls.enable_http2().wrap_connector(tcp),
    };
    let mut builder = Client::builder(TokioExecutor::new());
    builder
        .pool_idle_timeout(Duration::from_secs(90))
        .pool_max_idle_per_host(64)
        .http2_initial_stream_window_size(H2_STREAM_WINDOW)
        .http2_initial_connection_window_size(H2_CONNECTION_WINDOW);
    if http == OriginHttp::H2c {
        builder.http2_only(true);
    }
    Ok(builder.build(TargetConnector {
        inner: connector,
        target: dial.target.clone(),
    }))
}

/// Upstream response as returned by the origin.
pub struct OriginResponse {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: Bytes,
    /// Operator-facing cache diagnostic, set by the handler when a response is
    /// served but not stored.
    pub cache_reason: Option<&'static str>,
}

/// Streaming origin response: status/headers are available before the body is
/// consumed. Used by legacy bypass/video paths so large media is never
/// accumulated in memory.
pub struct OriginStreamResponse {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: Incoming,
}

impl OriginResponse {
    pub fn into_entry(self) -> CacheEntry {
        let headers = self
            .headers
            .iter()
            .filter_map(|(k, v)| {
                v.to_str()
                    .ok()
                    .map(|s| (k.as_str().to_string(), s.to_string()))
            })
            .collect();
        let mut entry = CacheEntry::new(self.status.as_u16(), headers, self.body);
        let has_etag = entry.header("etag").is_some();
        let has_last_modified = entry.header("last-modified").is_some();
        entry
            .headers
            .retain(|(name, _)| !name.eq_ignore_ascii_case(VALIDATOR_SOURCE_HEADER));
        if has_etag {
            entry
                .headers
                .push((VALIDATOR_SOURCE_HEADER.to_string(), "etag".to_string()));
        } else if has_last_modified {
            entry.headers.push((
                VALIDATOR_SOURCE_HEADER.to_string(),
                "last-modified".to_string(),
            ));
        } else {
            entry
                .headers
                .push(("etag".to_string(), compute_etag(&entry)));
            entry
                .headers
                .push((VALIDATOR_SOURCE_HEADER.to_string(), "computed".to_string()));
        }
        entry
    }
}

#[derive(Clone)]
pub struct OriginClient {
    /// Pooled clients, one per dialled target, TLS server name and protocol.
    clients: Arc<DashMap<ClientKey, Arc<TargetClient>>>,
    /// Fallback default origin (legacy single-origin)
    base_url: Arc<RwLock<String>>,
    /// TCP connect timeout applied to every connector this client builds.
    connect_timeout: Duration,
    /// Per-origin HTTP/2 vs HTTP/1.1 state, shared with the Pingora path.
    protocols: Arc<OriginProtocols>,
}

/// Headers worth replaying to the origin for a cacheable GET/HEAD.
fn cacheable_forward_headers(headers: &HeaderMap) -> Vec<(&'static str, HeaderValue)> {
    const ALLOW: &[&str] = &[
        "accept",
        "accept-language",
        "user-agent",
        "referer",
        "if-none-match",
        "if-modified-since",
    ];
    let mut out = Vec::with_capacity(ALLOW.len());
    for name in ALLOW {
        if let Some(v) = headers.get(*name) {
            out.push((*name, v.clone()));
        }
    }
    out
}

fn is_hop_by_hop(name: &str) -> bool {
    matches!(
        name,
        "connection"
            | "alt-svc"
            | "keep-alive"
            | "proxy-authenticate"
            | "proxy-authorization"
            | "proxy-connection"
            | "te"
            | "trailer"
            | "trailers"
            | "transfer-encoding"
            | "upgrade"
            | "host"
            | "content-length"
            | "expect"
    )
}

/// Request URI whose authority is the Host header, for HTTP/2's
/// `:authority`. `None` when the header is not a plain `host[:port]`.
fn with_authority(uri: &Uri, host: &HeaderValue) -> Option<Uri> {
    let authority: Authority = host.to_str().ok()?.trim().parse().ok()?;
    if authority.as_str().contains('@') {
        return None;
    }
    let mut parts = uri.clone().into_parts();
    parts.authority = Some(authority);
    Uri::from_parts(parts).ok()
}

/// Build one origin request. `host` is sent as a header only on the
/// HTTP/1.1-only path; otherwise the URI authority carries it.
fn origin_request(
    method: &Method,
    uri: Uri,
    headers: &HeaderMap,
    forward_sensitive: bool,
    host: Option<&HeaderValue>,
    body: Bytes,
) -> std::result::Result<Request<Full<Bytes>>, axum::http::Error> {
    let mut builder = Request::builder().method(method.clone()).uri(uri);
    if forward_sensitive {
        for (name, value) in headers.iter() {
            let n = name.as_str();
            if is_hop_by_hop(n) || n.eq_ignore_ascii_case("accept-encoding") {
                continue;
            }
            builder = builder.header(name.clone(), value.clone());
        }
        // Replay the client's encoding preferences for pass-through traffic.
        if let Some(v) = headers.get("accept-encoding") {
            builder = builder.header("accept-encoding", v.clone());
        }
    } else {
        for (name, value) in cacheable_forward_headers(headers) {
            builder = builder.header(name, value);
        }
        // Force identity so the stored body is directly reusable.
        builder = builder.header("accept-encoding", "identity");
        // Preserve the client address for origin-side logging/policies. The
        // pass-through branch above already copied it.
        if let Some(xff) = headers.get("x-forwarded-for") {
            builder = builder.header("x-forwarded-for", xff.clone());
        }
    }
    if let Some(host) = host {
        builder = builder.header(header::HOST, host.clone());
    }
    builder.body(Full::new(body))
}

/// An HTTP/2 failure that calls for a resend or for HTTP/1.1.
fn h2_failure(error: &hyper_util::client::legacy::Error, http: OriginHttp) -> Option<H2Failure> {
    if let Some(failure) = protocol::find_h2_error(error).and_then(protocol::classify_h2) {
        return Some(failure);
    }
    // h2c has no negotiation to fall back on: prior-knowledge HTTP/2 that
    // breaks before a response, other than failing to connect at all, means
    // the origin no longer speaks it.
    (http == OriginHttp::H2c && !error.is_connect()).then_some(H2Failure::Malformed)
}

/// `error: cause: cause…`, for logs.
fn error_chain(error: &(dyn std::error::Error + 'static)) -> String {
    let mut text = error.to_string();
    let mut source = error.source();
    while let Some(cause) = source {
        text.push_str(": ");
        text.push_str(&cause.to_string());
        source = cause.source();
    }
    text
}

impl OriginClient {
    pub fn new(base_url: String) -> Self {
        Self::with_connect_timeout(base_url, Duration::from_secs(5))
    }

    /// Build a client with an explicit TCP connect timeout.
    ///
    /// The connect timeout is independent of how long the origin may take to
    /// send the response: a dead or unreachable origin should fail over in
    /// seconds instead of consuming the per-domain request budget.
    pub fn with_connect_timeout(base_url: String, connect_timeout: Duration) -> Self {
        let _ = rustls::crypto::ring::default_provider().install_default();
        Self {
            clients: Arc::new(DashMap::new()),
            base_url: Arc::new(RwLock::new(base_url)),
            connect_timeout,
            protocols: Arc::new(OriginProtocols::new()),
        }
    }

    pub fn connect_timeout_ms(&self) -> u64 {
        self.connect_timeout.as_millis().min(u128::from(u64::MAX)) as u64
    }

    /// Per-origin HTTP version state. The Pingora path shares it so both
    /// origin paths agree on which origins get HTTP/2.
    pub fn protocols(&self) -> &Arc<OriginProtocols> {
        &self.protocols
    }

    fn http_version(&self, dial: &Dial) -> OriginHttp {
        if dial.tls {
            self.protocols.tls(&dial.key)
        } else {
            self.protocols
                .cleartext(&dial.key, ProbeTarget::Host(dial.host.clone(), dial.port))
        }
    }

    fn client(&self, dial: &Dial, http: OriginHttp) -> Result<Arc<TargetClient>> {
        let key = ClientKey {
            target: dial.target.to_string(),
            server_name: dial.server_name.clone(),
            http,
        };
        if let Some(client) = self.clients.get(&key) {
            return Ok(Arc::clone(&client));
        }
        let client = Arc::new(build_target_client(dial, http, self.connect_timeout)?);
        Ok(Arc::clone(&self.clients.entry(key).or_insert(client)))
    }

    pub async fn set_base_url(&self, url: String) {
        let mut w = self.base_url.write().await;
        if *w != url {
            tracing::info!(old = %*w, new = %url, "default origin base_url changed");
            *w = url;
        }
    }

    pub async fn base_url(&self) -> String {
        self.base_url.read().await.clone()
    }

    pub async fn fetch(
        &self,
        path_and_query: &str,
        headers: &HeaderMap,
    ) -> Result<(StatusCode, HeaderMap, Bytes)> {
        let base = self.base_url.read().await.clone();
        let resp = self
            .request(&base, &Method::GET, path_and_query, headers, None, false)
            .await?;
        Ok((resp.status, resp.headers, resp.body))
    }

    pub async fn fetch_from(
        &self,
        base_url: &str,
        path_and_query: &str,
        headers: &HeaderMap,
    ) -> Result<(StatusCode, HeaderMap, Bytes)> {
        self.fetch_from_with_sni(base_url, path_and_query, headers, None)
            .await
    }

    pub async fn fetch_from_with_sni(
        &self,
        base_url: &str,
        path_and_query: &str,
        headers: &HeaderMap,
        sni: Option<&str>,
    ) -> Result<(StatusCode, HeaderMap, Bytes)> {
        let resp = self
            .request_with_sni(
                base_url,
                &Method::GET,
                path_and_query,
                headers,
                None,
                false,
                sni,
            )
            .await?;
        Ok((resp.status, resp.headers, resp.body))
    }

    pub async fn fetch_as_entry(
        &self,
        path_and_query: &str,
        headers: &HeaderMap,
    ) -> Result<CacheEntry> {
        let base = self.base_url.read().await.clone();
        self.fetch_as_entry_from(&base, path_and_query, headers)
            .await
    }

    pub async fn fetch_as_entry_from(
        &self,
        base_url: &str,
        path_and_query: &str,
        headers: &HeaderMap,
    ) -> Result<CacheEntry> {
        self.fetch_as_entry_from_with_sni(base_url, path_and_query, headers, None)
            .await
    }

    pub async fn fetch_as_entry_from_with_sni(
        &self,
        base_url: &str,
        path_and_query: &str,
        headers: &HeaderMap,
        sni: Option<&str>,
    ) -> Result<CacheEntry> {
        Ok(self
            .request_with_sni(
                base_url,
                &Method::GET,
                path_and_query,
                headers,
                None,
                false,
                sni,
            )
            .await?
            .into_entry())
    }

    /// Proxy one request to the origin.
    ///
    /// `forward_sensitive` selects between the cacheable GET allow-list (false)
    /// and replaying all end-to-end headers, including cookies/authorization
    /// and conditional headers, for bypass/dynamic traffic (true).
    pub async fn request(
        &self,
        base_url: &str,
        method: &Method,
        path_and_query: &str,
        headers: &HeaderMap,
        body: Option<Bytes>,
        forward_sensitive: bool,
    ) -> Result<OriginResponse> {
        self.request_with_sni(
            base_url,
            method,
            path_and_query,
            headers,
            body,
            forward_sensitive,
            None,
        )
        .await
    }

    // Origin request dimensions are intentionally explicit here: callers
    // independently control forwarding, body, and TLS identity.
    #[allow(clippy::too_many_arguments)]
    async fn send_with_sni(
        &self,
        base_url: &str,
        method: &Method,
        path_and_query: &str,
        headers: &HeaderMap,
        body: Option<Bytes>,
        forward_sensitive: bool,
        sni: Option<&str>,
    ) -> Result<(StatusCode, HeaderMap, Incoming)> {
        let sni = sni.map(str::trim).filter(|s| !s.is_empty());
        // An IP origin with a certificate name is addressed by that name when
        // the caller sends no Host of its own.
        let effective_base = match sni {
            Some(name) if split_ip_base(base_url).is_some() => base_with_host(base_url, name),
            _ => base_url.to_string(),
        };
        let url = build_url(&effective_base, path_and_query)?;
        let dial = Dial::new(base_url, sni)?;
        let http = self.http_version(&dial);
        self.send(&dial, http, &url, method, headers, body, forward_sensitive)
            .await
    }

    /// Send over `http`, falling back to HTTP/1.1 once when HTTP/2 fails in
    /// a way [`H2Failure`] says is safe to resend.
    #[allow(clippy::too_many_arguments)]
    async fn send(
        &self,
        dial: &Dial,
        mut http: OriginHttp,
        url: &str,
        method: &Method,
        headers: &HeaderMap,
        body: Option<Bytes>,
        forward_sensitive: bool,
    ) -> Result<(StatusCode, HeaderMap, Incoming)> {
        let uri: Uri = url
            .parse()
            .with_context(|| format!("build origin request: {url}"))?;
        let host = headers.get(header::HOST);
        // Over HTTP/2 the Host travels as `:authority` (RFC 9113 §8.3.1), so
        // the URI must carry the site rather than the dialled address. A Host
        // that cannot be an authority keeps the request on HTTP/1.1.
        let site_uri = match host {
            Some(host) => with_authority(&uri, host),
            None => Some(uri.clone()),
        };
        if site_uri.is_none() {
            http = OriginHttp::Http1;
        }
        let body = body.unwrap_or_default();
        let mut resent = false;
        let resp = loop {
            let req = match (&site_uri, http) {
                (Some(site_uri), OriginHttp::Alpn | OriginHttp::H2c) => origin_request(
                    method,
                    site_uri.clone(),
                    headers,
                    forward_sensitive,
                    None,
                    body.clone(),
                ),
                // HTTP/1.1 keeps the configured address in the URI (one pool
                // per origin, shared by every site) and sends Host as is.
                _ => origin_request(
                    method,
                    uri.clone(),
                    headers,
                    forward_sensitive,
                    host,
                    body.clone(),
                ),
            }
            .with_context(|| format!("build origin request: {url}"))?;
            let error = match self.client(dial, http)?.request(req).await {
                Ok(resp) => break resp,
                Err(error) => error,
            };
            let failure = if http.is_http2() && !resent {
                h2_failure(&error, http)
            } else {
                None
            };
            if let Some(failure) = failure {
                if failure.downgrades() {
                    self.protocols.downgrade(&dial.key, &error_chain(&error));
                }
                if failure.may_resend(method.is_idempotent()) {
                    resent = true;
                    if failure.downgrades() {
                        http = OriginHttp::Http1;
                    }
                    continue;
                }
            }
            return Err(anyhow::Error::new(error).context(format!("origin request failed: {url}")));
        };

        let status = StatusCode::from_u16(resp.status().as_u16())
            .map_err(|e| anyhow!("invalid status: {e}"))?;

        let mut out_headers = HeaderMap::new();
        // A HEAD response has no body: its Content-Length describes the
        // representation rather than the framing, so it must survive the
        // hop-by-hop filter. The caller reports it to the client (and the
        // generic filter would otherwise turn every HEAD into length 0).
        if method == Method::HEAD {
            if let Some(value) = resp.headers().get(header::CONTENT_LENGTH) {
                if let (Ok(name), Ok(val)) = (
                    HeaderName::from_bytes(header::CONTENT_LENGTH.as_str().as_bytes()),
                    HeaderValue::from_bytes(value.as_bytes()),
                ) {
                    out_headers.insert(name, val);
                }
            }
        }
        for (k, v) in resp.headers().iter() {
            if is_hop_by_hop(k.as_str()) {
                continue;
            }
            if let (Ok(name), Ok(val)) = (
                HeaderName::from_bytes(k.as_ref()),
                HeaderValue::from_bytes(v.as_bytes()),
            ) {
                out_headers.insert(name, val);
            }
        }

        Ok((status, out_headers, resp.into_body()))
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn request_with_sni(
        &self,
        base_url: &str,
        method: &Method,
        path_and_query: &str,
        headers: &HeaderMap,
        body: Option<Bytes>,
        forward_sensitive: bool,
        sni: Option<&str>,
    ) -> Result<OriginResponse> {
        self.request_with_sni_limit(
            base_url,
            method,
            path_and_query,
            headers,
            body,
            forward_sensitive,
            sni,
            64 * 1024 * 1024,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn request_with_sni_limit(
        &self,
        base_url: &str,
        method: &Method,
        path_and_query: &str,
        headers: &HeaderMap,
        body: Option<Bytes>,
        forward_sensitive: bool,
        sni: Option<&str>,
        max_body: usize,
    ) -> Result<OriginResponse> {
        let (status, headers, body) = self
            .send_with_sni(
                base_url,
                method,
                path_and_query,
                headers,
                body,
                forward_sensitive,
                sni,
            )
            .await?;
        let body = Limited::new(body, max_body)
            .collect()
            .await
            .map_err(|e| anyhow!("read origin body: {e}"))?
            .to_bytes();
        Ok(OriginResponse {
            status,
            headers,
            body,
            cache_reason: None,
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn request_stream_with_sni(
        &self,
        base_url: &str,
        method: &Method,
        path_and_query: &str,
        headers: &HeaderMap,
        body: Option<Bytes>,
        forward_sensitive: bool,
        sni: Option<&str>,
    ) -> Result<OriginStreamResponse> {
        let (status, headers, body) = self
            .send_with_sni(
                base_url,
                method,
                path_and_query,
                headers,
                body,
                forward_sensitive,
                sni,
            )
            .await?;
        Ok(OriginStreamResponse {
            status,
            headers,
            body,
        })
    }
}

fn split_ip_base(base_url: &str) -> Option<(IpAddr, u16)> {
    let uri: Uri = base_url.parse().ok()?;
    let host = uri.host()?;
    let ip: IpAddr = host.parse().ok()?;
    let port = uri.port_u16().unwrap_or_else(|| {
        if uri.scheme_str() == Some("http") {
            80
        } else {
            443
        }
    });
    Some((ip, port))
}

fn base_with_host(base_url: &str, host: &str) -> String {
    let Ok(uri) = base_url.parse::<Uri>() else {
        return base_url.to_string();
    };
    let scheme = uri.scheme_str().unwrap_or("https");
    match uri.port_u16() {
        Some(port) => format!("{scheme}://{host}:{port}"),
        None => format!("{scheme}://{host}"),
    }
}

fn build_url(base_url: &str, path_and_query: &str) -> Result<String> {
    let base = base_url.trim_end_matches('/');
    if !(base.starts_with("http://") || base.starts_with("https://")) {
        return Err(anyhow!(
            "origin url must start with http:// or https://: {base}"
        ));
    }
    let path = if path_and_query.starts_with('/') {
        path_and_query.to_string()
    } else {
        format!("/{path_and_query}")
    };
    Ok(format!("{base}{path}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn joins_base_and_path() {
        assert_eq!(
            build_url("http://origin.test:8080/", "/a?b=1").unwrap(),
            "http://origin.test:8080/a?b=1"
        );
        assert_eq!(
            build_url("https://origin.test", "a").unwrap(),
            "https://origin.test/a"
        );
        assert!(build_url("origin.test", "/a").is_err());
    }

    #[test]
    fn ip_base_is_split_and_rewritten_to_sni_host() {
        assert_eq!(
            split_ip_base("https://45.63.84.225:443"),
            Some(("45.63.84.225".parse().unwrap(), 443))
        );
        assert_eq!(
            base_with_host("https://45.63.84.225", "test-origin.akgo.net"),
            "https://test-origin.akgo.net"
        );
        assert_eq!(
            base_with_host("https://45.63.84.225:8443", "test-origin.akgo.net"),
            "https://test-origin.akgo.net:8443"
        );
        assert_eq!(split_ip_base("https://origin.test"), None);
    }

    #[test]
    fn origin_entry_gets_computed_etag_when_origin_omits_it() {
        let entry = OriginResponse {
            status: StatusCode::OK,
            headers: HeaderMap::new(),
            body: Bytes::from_static(b"hello"),
            cache_reason: None,
        }
        .into_entry();
        assert!(entry.header("etag").is_some());
    }

    #[test]
    fn hop_by_hop_detection() {
        assert!(is_hop_by_hop("connection"));
        assert!(is_hop_by_hop("transfer-encoding"));
        assert!(!is_hop_by_hop("content-type"));
        assert!(!is_hop_by_hop("set-cookie"));
    }

    #[test]
    fn connect_timeout_is_configurable_and_reported() {
        let client = OriginClient::with_connect_timeout(
            "https://origin.test".into(),
            Duration::from_millis(2_500),
        );
        assert_eq!(client.connect_timeout_ms(), 2_500);
    }

    #[test]
    fn host_header_becomes_the_request_authority() {
        let uri: Uri = "https://45.63.84.225:8443/a?b=1".parse().unwrap();
        let site = with_authority(&uri, &HeaderValue::from_static("www.example.com")).unwrap();
        assert_eq!(site.to_string(), "https://www.example.com/a?b=1");
        let with_port =
            with_authority(&uri, &HeaderValue::from_static("example.com:8080")).unwrap();
        assert_eq!(with_port.authority().unwrap().as_str(), "example.com:8080");
        assert!(with_authority(&uri, &HeaderValue::from_static("[::1]:80")).is_some());
        assert!(with_authority(&uri, &HeaderValue::from_static("")).is_none());
        assert!(with_authority(&uri, &HeaderValue::from_static("a b")).is_none());
        assert!(with_authority(&uri, &HeaderValue::from_static("user@example.com")).is_none());
    }

    #[test]
    fn dial_targets_the_configured_origin() {
        let dial = Dial::new("https://45.63.84.225", Some("www.example.com")).unwrap();
        assert!(dial.tls);
        assert_eq!(dial.target.to_string(), "https://45.63.84.225:443/");
        assert_eq!(dial.server_name.as_deref(), Some("www.example.com"));
        assert_eq!(dial.key, "https://45.63.84.225:443#www.example.com");
        let plain = Dial::new("http://origin.test:8080/prefix", Some("ignored")).unwrap();
        assert!(!plain.tls);
        assert_eq!(plain.server_name, None);
        assert_eq!(plain.key, "http://origin.test:8080");
        assert!(Dial::new("ftp://origin.test", None).is_err());
    }

    fn site_headers() -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(header::HOST, HeaderValue::from_static("www.example.com"));
        headers.insert("x-forwarded-for", HeaderValue::from_static("198.51.100.7"));
        headers
    }

    #[tokio::test]
    async fn h2c_sends_the_site_as_authority_without_a_host_header() {
        use tokio::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (seen_tx, seen_rx) = tokio::sync::oneshot::channel();
        tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            let mut connection = h2::server::handshake(socket).await.unwrap();
            let Some(Ok((request, mut respond))) = connection.accept().await else {
                return;
            };
            let _ = seen_tx.send((
                request.uri().authority().map(|a| a.to_string()),
                request.headers().get_all(header::HOST).iter().count(),
                request.headers().get_all("x-forwarded-for").iter().count(),
            ));
            let response = http::Response::builder().status(200).body(()).unwrap();
            let mut body = respond.send_response(response, false).unwrap();
            body.send_data(Bytes::from_static(b"ok"), true).unwrap();
            while let Some(Ok(_)) = connection.accept().await {}
        });

        let client = OriginClient::new(String::new());
        let base = format!("http://{addr}");
        let dial = Dial::new(&base, None).unwrap();
        let url = build_url(&base, "/a?b=1").unwrap();
        let (status, _, body) = client
            .send(
                &dial,
                OriginHttp::H2c,
                &url,
                &Method::GET,
                &site_headers(),
                None,
                true,
            )
            .await
            .unwrap();
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body.collect().await.unwrap().to_bytes(), "ok");
        let (authority, hosts, forwarded) = seen_rx.await.unwrap();
        assert_eq!(authority.as_deref(), Some("www.example.com"));
        assert_eq!(hosts, 0);
        assert_eq!(forwarded, 1);
    }

    /// HTTP/1.1-only origin: rejects an h2c preface with 400, answers
    /// HTTP/1.1 requests with `ok`. Reports each request head it reads.
    async fn http1_only_origin() -> (String, tokio::sync::mpsc::UnboundedReceiver<String>) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        tokio::spawn(async move {
            while let Ok((mut socket, _)) = listener.accept().await {
                let tx = tx.clone();
                tokio::spawn(async move {
                    let mut head = Vec::new();
                    let mut buf = [0u8; 1024];
                    while !head.windows(4).any(|w| w == b"\r\n\r\n") {
                        match socket.read(&mut buf).await {
                            Ok(0) | Err(_) => return,
                            Ok(n) => head.extend_from_slice(&buf[..n]),
                        }
                    }
                    let head = String::from_utf8_lossy(&head).to_string();
                    let reply: &[u8] = if head.starts_with("PRI * HTTP/2.0") {
                        b"HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                    } else {
                        b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok"
                    };
                    let _ = tx.send(head);
                    let _ = socket.write_all(reply).await;
                    let _ = socket.shutdown().await;
                });
            }
        });
        (format!("http://{addr}"), rx)
    }

    #[tokio::test]
    async fn h2c_rejected_by_an_http1_origin_falls_back_for_get() {
        let (base, mut heads) = http1_only_origin().await;
        let client = OriginClient::new(String::new());
        let dial = Dial::new(&base, None).unwrap();
        let url = build_url(&base, "/a").unwrap();
        let (status, _, body) = client
            .send(
                &dial,
                OriginHttp::H2c,
                &url,
                &Method::GET,
                &site_headers(),
                None,
                true,
            )
            .await
            .unwrap();
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body.collect().await.unwrap().to_bytes(), "ok");
        assert!(client.protocols().held_on_http1(&dial.key));

        assert!(heads.recv().await.unwrap().starts_with("PRI * HTTP/2.0"));
        let resent = heads.recv().await.unwrap().to_ascii_lowercase();
        assert!(resent.starts_with("get /a http/1.1\r\n"), "{resent}");
        assert!(resent.contains("\r\nhost: www.example.com\r\n"), "{resent}");
        assert_eq!(resent.matches("x-forwarded-for").count(), 1, "{resent}");
    }

    #[tokio::test]
    async fn h2c_rejected_by_an_http1_origin_is_not_resent_for_post() {
        let (base, mut heads) = http1_only_origin().await;
        let client = OriginClient::new(String::new());
        let dial = Dial::new(&base, None).unwrap();
        let url = build_url(&base, "/submit").unwrap();
        let result = client
            .send(
                &dial,
                OriginHttp::H2c,
                &url,
                &Method::POST,
                &site_headers(),
                Some(Bytes::from_static(b"payload")),
                true,
            )
            .await;
        assert!(result.is_err());
        // Later requests go straight to HTTP/1.1.
        assert!(client.protocols().held_on_http1(&dial.key));
        assert!(heads.recv().await.unwrap().starts_with("PRI * HTTP/2.0"));
        assert!(heads.try_recv().is_err());
    }
}
