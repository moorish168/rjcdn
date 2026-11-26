//! Origin HTTP version selection: HTTP/2 first, HTTP/1.1 as the fallback.
//!
//! Shared by both origin paths, the Pingora proxy (`upstream_peer`) and the
//! hyper [`OriginClient`](super::OriginClient), so an origin that fails over
//! HTTP/2 on one path is not retried over HTTP/2 on the other.
//!
//! * `https` origins offer ALPN `h2, http/1.1` and the origin chooses, so a
//!   server without HTTP/2 simply answers over HTTP/1.1. An origin that
//!   negotiated h2 and then failed at the protocol level (`HTTP_1_1_REQUIRED`,
//!   frames our HTTP/2 stack rejects, a failed HTTP/2 handshake) is held on
//!   HTTP/1.1 for [`DOWNGRADE_TTL`]; the request that hit the failure is
//!   retried over HTTP/1.1 by the caller.
//! * `http` origins cannot negotiate. HTTP/2 over cleartext (h2c with prior
//!   knowledge, RFC 9113 §3.3) is sent only after a background probe saw the
//!   origin answer the connection preface with a SETTINGS frame: an
//!   HTTP/1.1-only server fails such a request, and some never answer the
//!   preface at all. Until a probe confirms h2c the request goes out over
//!   HTTP/1.1 immediately, so probing never delays traffic.
//! * Loopback targets are internal hops (prefetch through this edge's own
//!   listener) and stay on HTTP/1.1.

use dashmap::mapref::entry::Entry;
use dashmap::DashMap;
use std::io::ErrorKind;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

/// How long an origin stays on HTTP/1.1 after an HTTP/2 protocol failure.
pub const DOWNGRADE_TTL: Duration = Duration::from_secs(30 * 60);
/// Re-probe interval after an origin answered the h2c preface with something
/// other than HTTP/2 (typically an HTTP/1.1 400/505, or closing the socket).
const H2C_REFUSED_TTL: Duration = Duration::from_secs(60 * 60);
/// Re-probe interval after a probe without a verdict (connect failure,
/// timeout): the origin may simply have been unreachable.
const H2C_INCONCLUSIVE_TTL: Duration = Duration::from_secs(10 * 60);
/// Upper bound for one probe: name resolution, TCP connect and the server
/// preface together.
const H2C_PROBE_TIMEOUT: Duration = Duration::from_secs(3);
/// A probe still marked as running after this long is assumed lost.
const H2C_PROBE_STALE: Duration = Duration::from_secs(10);

/// HTTP/2 client connection preface (RFC 9113 §3.4) followed by an empty
/// SETTINGS frame.
const CLIENT_PREFACE: &[u8] =
    b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n\x00\x00\x00\x04\x00\x00\x00\x00\x00";
/// GOAWAY(last stream 0, NO_ERROR): ends the probe connection cleanly.
const GOAWAY_NO_ERROR: &[u8] = &[0, 0, 8, 7, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];

/// Wire protocol for one origin request.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum OriginHttp {
    /// HTTP/1.1 only.
    #[default]
    Http1,
    /// TLS offering ALPN `h2, http/1.1`; the origin picks.
    Alpn,
    /// Cleartext HTTP/2 with prior knowledge.
    H2c,
}

impl OriginHttp {
    /// Whether this attempt may run over HTTP/2.
    pub fn is_http2(self) -> bool {
        !matches!(self, Self::Http1)
    }
}

/// Where the h2c probe can reach a cleartext origin.
#[derive(Clone, Debug)]
pub enum ProbeTarget {
    /// Address already resolved by the caller.
    Addr(SocketAddr),
    /// Host name (or IP literal) and port, resolved by the probe.
    Host(String, u16),
}

impl ProbeTarget {
    /// Known without a lookup to be this host. Names that resolve to a
    /// loopback address are caught by the probe instead.
    fn is_internal(&self) -> bool {
        match self {
            Self::Addr(addr) => is_internal(addr.ip()),
            Self::Host(host, _) => host
                .trim_start_matches('[')
                .trim_end_matches(']')
                .parse::<IpAddr>()
                .is_ok_and(is_internal),
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum State {
    /// h2c probe in flight; HTTP/1.1 meanwhile.
    Probing { since: Instant },
    /// The origin answered the h2c preface.
    H2c,
    /// HTTP/1.1 only until `until` (h2c refused, or HTTP/2 failed).
    Http1 { until: Instant },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Verdict {
    H2c,
    Http1,
    Unknown,
}

/// Why an HTTP/2 request to an origin failed, when the failure calls for a
/// resend or for HTTP/1.1.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum H2Failure {
    /// `REFUSED_STREAM`: the origin did not process the request (RFC 9113
    /// §8.7). Resend it as is.
    Refused,
    /// `GOAWAY(NO_ERROR)` reached the stream: the connection is draining.
    /// The stream was most likely not processed; idempotent requests are
    /// resent on a new connection.
    Draining,
    /// `HTTP_1_1_REQUIRED`: the origin refused the request over HTTP/2.
    Http11Required,
    /// Our HTTP/2 stack rejected what the origin sent; it may not speak
    /// HTTP/2 at all. The request may have been processed.
    Malformed,
}

impl H2Failure {
    /// Whether the origin should get HTTP/1.1 from now on.
    pub fn downgrades(self) -> bool {
        matches!(self, Self::Http11Required | Self::Malformed)
    }

    /// Whether a request may be sent again. Only refusals guarantee the
    /// origin did not act on it; anything else is limited to idempotent
    /// methods.
    pub fn may_resend(self, idempotent: bool) -> bool {
        matches!(self, Self::Refused | Self::Http11Required) || idempotent
    }
}

/// Classify an `h2` error from an origin connection.
pub fn classify_h2(error: &h2::Error) -> Option<H2Failure> {
    let reason = error.reason()?;
    if error.is_remote() {
        if reason == h2::Reason::HTTP_1_1_REQUIRED {
            return Some(H2Failure::Http11Required);
        }
        if error.is_reset() && reason == h2::Reason::REFUSED_STREAM {
            return Some(H2Failure::Refused);
        }
        if error.is_go_away() && reason == h2::Reason::NO_ERROR {
            return Some(H2Failure::Draining);
        }
    }
    // A library GOAWAY means our side found the peer breaking the protocol,
    // e.g. an HTTP/1.1 status line where a frame header belongs.
    if error.is_library()
        && error.is_go_away()
        && (reason == h2::Reason::PROTOCOL_ERROR || reason == h2::Reason::FRAME_SIZE_ERROR)
    {
        return Some(H2Failure::Malformed);
    }
    None
}

/// The first `h2` error in a source chain.
pub fn find_h2_error<'a>(error: &'a (dyn std::error::Error + 'static)) -> Option<&'a h2::Error> {
    let mut current = Some(error);
    while let Some(error) = current {
        if let Some(h2) = error.downcast_ref::<h2::Error>() {
            return Some(h2);
        }
        current = error.source();
    }
    None
}

/// Key for one origin connection target. HTTPS keys include the TLS server
/// name: ALPN is answered per virtual server.
pub fn origin_key(tls: bool, host: &str, port: u16, sni: Option<&str>) -> String {
    let host = host.trim().trim_end_matches('.').to_ascii_lowercase();
    if tls {
        let sni = sni
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| s.trim_end_matches('.').to_ascii_lowercase())
            .unwrap_or_else(|| host.clone());
        format!("https://{host}:{port}#{sni}")
    } else {
        format!("http://{host}:{port}")
    }
}

/// Per-origin HTTP version state for this edge process.
#[derive(Debug, Default)]
pub struct OriginProtocols {
    states: DashMap<String, State>,
}

impl OriginProtocols {
    pub fn new() -> Self {
        Self::default()
    }

    /// Protocol for an `https` origin: ALPN with h2 unless the origin is
    /// currently held on HTTP/1.1.
    pub fn tls(&self, key: &str) -> OriginHttp {
        match self.state(key) {
            Some(State::Http1 { until }) if until > Instant::now() => OriginHttp::Http1,
            _ => OriginHttp::Alpn,
        }
    }

    /// Protocol for an `http` origin: h2c once a probe confirmed it,
    /// HTTP/1.1 otherwise. Starts a background probe when nothing current is
    /// known, so this call never waits on the network.
    pub fn cleartext(self: &Arc<Self>, key: &str, target: ProbeTarget) -> OriginHttp {
        if target.is_internal() {
            return OriginHttp::Http1;
        }
        let now = Instant::now();
        match self.state(key) {
            Some(State::H2c) => return OriginHttp::H2c,
            Some(State::Http1 { until }) if until > now => return OriginHttp::Http1,
            Some(State::Probing { since }) if now.duration_since(since) < H2C_PROBE_STALE => {
                return OriginHttp::Http1
            }
            _ => {}
        }
        self.start_probe(key, target, now);
        OriginHttp::Http1
    }

    /// Record an HTTP/2 protocol failure: HTTP/1.1 for [`DOWNGRADE_TTL`].
    /// Logs once per downgrade, not once per failed request.
    pub fn downgrade(&self, key: &str, reason: &str) {
        let now = Instant::now();
        let previous = self.states.insert(
            key.to_string(),
            State::Http1 {
                until: now + DOWNGRADE_TTL,
            },
        );
        let already = matches!(previous, Some(State::Http1 { until }) if until > now);
        if !already {
            tracing::warn!(
                origin = key,
                %reason,
                hold_secs = DOWNGRADE_TTL.as_secs(),
                "origin HTTP/2 failed; using HTTP/1.1"
            );
        }
    }

    fn state(&self, key: &str) -> Option<State> {
        self.states.get(key).map(|state| *state)
    }

    /// Whether `key` is currently held on HTTP/1.1.
    #[cfg(test)]
    pub(crate) fn held_on_http1(&self, key: &str) -> bool {
        matches!(self.state(key), Some(State::Http1 { until }) if until > Instant::now())
    }

    fn start_probe(self: &Arc<Self>, key: &str, target: ProbeTarget, now: Instant) {
        // Claim the slot under the shard lock so concurrent requests start a
        // single probe.
        let claimed = match self.states.entry(key.to_string()) {
            Entry::Occupied(mut entry) => {
                let due = match *entry.get() {
                    State::H2c => false,
                    State::Http1 { until } => until <= now,
                    State::Probing { since } => now.duration_since(since) >= H2C_PROBE_STALE,
                };
                if due {
                    entry.insert(State::Probing { since: now });
                }
                due
            }
            Entry::Vacant(entry) => {
                entry.insert(State::Probing { since: now });
                true
            }
        };
        if !claimed {
            return;
        }
        let Ok(runtime) = tokio::runtime::Handle::try_current() else {
            self.states.remove(key);
            return;
        };
        let this = Arc::clone(self);
        let key = key.to_string();
        runtime.spawn(async move {
            let verdict = probe(target, H2C_PROBE_TIMEOUT).await;
            this.finish_probe(&key, verdict);
        });
    }

    fn finish_probe(&self, key: &str, verdict: Verdict) {
        let now = Instant::now();
        let next = match verdict {
            Verdict::H2c => State::H2c,
            Verdict::Http1 => State::Http1 {
                until: now + H2C_REFUSED_TTL,
            },
            Verdict::Unknown => State::Http1 {
                until: now + H2C_INCONCLUSIVE_TTL,
            },
        };
        let Some(mut state) = self.states.get_mut(key) else {
            return;
        };
        // A downgrade recorded while the probe ran is newer evidence.
        if !matches!(*state, State::Probing { .. }) {
            return;
        }
        *state = next;
        drop(state);
        match verdict {
            Verdict::H2c => tracing::info!(origin = key, "origin accepts h2c; using HTTP/2"),
            Verdict::Http1 => tracing::debug!(origin = key, "origin refused h2c; using HTTP/1.1"),
            Verdict::Unknown => {
                tracing::debug!(origin = key, "h2c probe inconclusive; using HTTP/1.1")
            }
        }
    }
}

/// Loopback and unspecified targets are this host: internal hops, not
/// origins worth probing.
fn is_internal(ip: IpAddr) -> bool {
    let ip = ip.to_canonical();
    ip.is_loopback() || ip.is_unspecified()
}

async fn probe(target: ProbeTarget, timeout: Duration) -> Verdict {
    let deadline = tokio::time::Instant::now() + timeout;
    let addr = match target {
        ProbeTarget::Addr(addr) => addr,
        ProbeTarget::Host(host, port) => {
            let host = host.trim_start_matches('[').trim_end_matches(']');
            let lookup = tokio::time::timeout_at(deadline, tokio::net::lookup_host((host, port)));
            match lookup.await {
                Ok(Ok(mut addrs)) => match addrs.next() {
                    Some(addr) => addr,
                    None => return Verdict::Unknown,
                },
                _ => return Verdict::Unknown,
            }
        }
    };
    if is_internal(addr.ip()) {
        return Verdict::Http1;
    }
    probe_addr(addr, deadline).await
}

async fn probe_addr(addr: SocketAddr, deadline: tokio::time::Instant) -> Verdict {
    tokio::time::timeout_at(deadline, exchange_preface(addr))
        .await
        .unwrap_or(Verdict::Unknown)
}

/// Send the client preface and read the first frame header of the answer.
async fn exchange_preface(addr: SocketAddr) -> Verdict {
    let Ok(mut stream) = TcpStream::connect(addr).await else {
        return Verdict::Unknown;
    };
    let _ = stream.set_nodelay(true);
    if stream.write_all(CLIENT_PREFACE).await.is_err() {
        return Verdict::Unknown;
    }
    let mut head = [0u8; 9];
    match stream.read_exact(&mut head).await {
        Ok(_) => {}
        // Closed or reset instead of answering: an HTTP/1.x server rejecting
        // the preface.
        Err(e)
            if matches!(
                e.kind(),
                ErrorKind::UnexpectedEof
                    | ErrorKind::ConnectionReset
                    | ErrorKind::ConnectionAborted
                    | ErrorKind::BrokenPipe
            ) =>
        {
            return Verdict::Http1
        }
        Err(_) => return Verdict::Unknown,
    }
    if !is_server_settings(&head) {
        return Verdict::Http1;
    }
    let _ = stream.write_all(GOAWAY_NO_ERROR).await;
    let _ = stream.shutdown().await;
    Verdict::H2c
}

/// The server preface starts with a SETTINGS frame (type 0x4) on stream 0,
/// without ACK, whose payload is a whole number of 6-byte settings and fits
/// the default 16 KiB frame size.
fn is_server_settings(head: &[u8; 9]) -> bool {
    let length = u32::from_be_bytes([0, head[0], head[1], head[2]]);
    let stream_id = u32::from_be_bytes([head[5], head[6], head[7], head[8]]) & 0x7fff_ffff;
    head[3] == 0x4
        && head[4] & 0x1 == 0
        && stream_id == 0
        && length.is_multiple_of(6)
        && length <= 16_384
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpListener;

    #[test]
    fn keys_normalize_host_and_separate_tls_server_names() {
        assert_eq!(
            origin_key(false, "Origin.Example.", 8080, Some("ignored")),
            "http://origin.example:8080"
        );
        assert_eq!(
            origin_key(true, "1.2.3.4", 443, None),
            "https://1.2.3.4:443#1.2.3.4"
        );
        assert_eq!(
            origin_key(true, "1.2.3.4", 443, Some(" WWW.Example.com ")),
            "https://1.2.3.4:443#www.example.com"
        );
        assert_ne!(
            origin_key(true, "1.2.3.4", 443, Some("a.example")),
            origin_key(true, "1.2.3.4", 443, Some("b.example"))
        );
    }

    #[test]
    fn server_preface_must_be_settings_on_stream_zero() {
        // SETTINGS with one entry (6 bytes), no flags, stream 0.
        assert!(is_server_settings(&[0, 0, 6, 4, 0, 0, 0, 0, 0]));
        assert!(is_server_settings(&[0, 0, 0, 4, 0, 0, 0, 0, 0]));
        // ACK, odd length, non-zero stream, other frame type.
        assert!(!is_server_settings(&[0, 0, 0, 4, 1, 0, 0, 0, 0]));
        assert!(!is_server_settings(&[0, 0, 5, 4, 0, 0, 0, 0, 0]));
        assert!(!is_server_settings(&[0, 0, 6, 4, 0, 0, 0, 0, 1]));
        assert!(!is_server_settings(&[0, 0, 8, 7, 0, 0, 0, 0, 0]));
        // An HTTP/1.1 status line read as a frame header.
        let mut h1 = [0u8; 9];
        h1.copy_from_slice(&b"HTTP/1.1 400 Bad Request"[..9]);
        assert!(!is_server_settings(&h1));
    }

    #[test]
    fn https_origins_use_alpn_until_downgraded() {
        let protocols = OriginProtocols::new();
        let key = origin_key(true, "origin.example", 443, None);
        assert_eq!(protocols.tls(&key), OriginHttp::Alpn);
        protocols.downgrade(&key, "test");
        assert_eq!(protocols.tls(&key), OriginHttp::Http1);
        protocols.states.insert(
            key.clone(),
            State::Http1 {
                until: Instant::now() - Duration::from_secs(1),
            },
        );
        assert_eq!(protocols.tls(&key), OriginHttp::Alpn);
    }

    #[test]
    fn loopback_targets_stay_on_http1_without_probing() {
        let protocols = Arc::new(OriginProtocols::new());
        let key = origin_key(false, "127.0.0.1", 8081, None);
        let target = ProbeTarget::Addr("127.0.0.1:8081".parse().unwrap());
        assert_eq!(protocols.cleartext(&key, target), OriginHttp::Http1);
        assert!(protocols.state(&key).is_none());
        let mapped = ProbeTarget::Addr("[::ffff:127.0.0.1]:80".parse().unwrap());
        assert_eq!(protocols.cleartext(&key, mapped), OriginHttp::Http1);
        // Prefetch reaches this edge's own listener by IP literal.
        let literal = ProbeTarget::Host("127.0.0.1".into(), 8081);
        assert_eq!(protocols.cleartext(&key, literal), OriginHttp::Http1);
        let v6 = ProbeTarget::Host("[::1]".into(), 8081);
        assert_eq!(protocols.cleartext(&key, v6), OriginHttp::Http1);
        assert!(protocols.state(&key).is_none());
    }

    #[tokio::test]
    async fn cleartext_uses_http1_until_the_probe_confirms_h2c() {
        let protocols = Arc::new(OriginProtocols::new());
        let key = origin_key(false, "origin.example", 80, None);
        // Documentation address: the background probe cannot succeed, and the
        // verdict below is written directly.
        let target = ProbeTarget::Addr("192.0.2.1:80".parse().unwrap());
        assert_eq!(protocols.cleartext(&key, target.clone()), OriginHttp::Http1);
        assert!(matches!(protocols.state(&key), Some(State::Probing { .. })));
        // A second request while probing neither waits nor starts another.
        assert_eq!(protocols.cleartext(&key, target.clone()), OriginHttp::Http1);

        protocols.finish_probe(&key, Verdict::H2c);
        assert_eq!(protocols.cleartext(&key, target.clone()), OriginHttp::H2c);

        // An h2c failure later holds the origin on HTTP/1.1.
        protocols.downgrade(&key, "test");
        assert_eq!(protocols.cleartext(&key, target), OriginHttp::Http1);
    }

    #[tokio::test]
    async fn downgrade_during_a_probe_is_not_overwritten() {
        let protocols = OriginProtocols::new();
        let key = origin_key(false, "origin.example", 80, None);
        protocols.states.insert(
            key.clone(),
            State::Probing {
                since: Instant::now(),
            },
        );
        protocols.downgrade(&key, "test");
        protocols.finish_probe(&key, Verdict::H2c);
        assert!(matches!(protocols.state(&key), Some(State::Http1 { .. })));
    }

    fn soon(ms: u64) -> tokio::time::Instant {
        tokio::time::Instant::now() + Duration::from_millis(ms)
    }

    #[tokio::test]
    async fn probe_recognizes_an_h2c_server() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            if let Ok(mut connection) = h2::server::handshake(socket).await {
                while let Some(Ok(_)) = connection.accept().await {}
            }
        });
        assert_eq!(probe_addr(addr, soon(2_000)).await, Verdict::H2c);
    }

    #[tokio::test]
    async fn probe_rejects_an_http1_server() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buf = [0u8; 64];
            let _ = socket.read(&mut buf).await;
            let _ = socket
                .write_all(
                    b"HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                )
                .await;
        });
        assert_eq!(probe_addr(addr, soon(2_000)).await, Verdict::Http1);
    }

    #[tokio::test]
    async fn probe_treats_an_immediate_close_as_http1() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            drop(socket);
        });
        assert_eq!(probe_addr(addr, soon(2_000)).await, Verdict::Http1);
    }

    #[tokio::test]
    async fn silent_server_is_inconclusive() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let hold = tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            tokio::time::sleep(Duration::from_secs(5)).await;
            drop(socket);
        });
        assert_eq!(probe_addr(addr, soon(300)).await, Verdict::Unknown);
        hold.abort();
    }

    /// Error an `h2` client sees for one GET against `server`.
    async fn h2_request_error<F, Fut>(server: F) -> h2::Error
    where
        F: FnOnce(TcpStream) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = ()> + Send,
    {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            server(socket).await;
        });
        let tcp = TcpStream::connect(addr).await.unwrap();
        let (client, connection) = h2::client::handshake(tcp).await.unwrap();
        tokio::spawn(async move {
            let _ = connection.await;
        });
        let mut client = client.ready().await.unwrap();
        let request = http::Request::get("http://origin.test/").body(()).unwrap();
        let (response, _) = client.send_request(request, true).unwrap();
        tokio::time::timeout(Duration::from_secs(5), response)
            .await
            .expect("h2 response")
            .expect_err("request must fail")
    }

    async fn reset_first_stream(socket: TcpStream, reason: h2::Reason) {
        let mut connection = h2::server::handshake(socket).await.unwrap();
        if let Some(Ok((_, mut respond))) = connection.accept().await {
            respond.send_reset(reason);
        }
        while let Some(Ok(_)) = connection.accept().await {}
    }

    #[tokio::test]
    async fn http1_reply_to_h2_is_malformed_and_downgrades() {
        let error = h2_request_error(|mut socket| async move {
            let mut buf = [0u8; 256];
            let _ = socket.read(&mut buf).await;
            let _ = socket
                .write_all(b"HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\n\r\n")
                .await;
            tokio::time::sleep(Duration::from_millis(200)).await;
        })
        .await;
        let failure = classify_h2(&error);
        assert_eq!(failure, Some(H2Failure::Malformed), "{error:?}");
        assert!(failure.unwrap().downgrades());
        assert!(!failure.unwrap().may_resend(false));
        assert!(failure.unwrap().may_resend(true));
    }

    #[tokio::test]
    async fn refused_stream_is_resent_for_any_method() {
        let error =
            h2_request_error(|socket| reset_first_stream(socket, h2::Reason::REFUSED_STREAM)).await;
        let failure = classify_h2(&error);
        assert_eq!(failure, Some(H2Failure::Refused), "{error:?}");
        assert!(!failure.unwrap().downgrades());
        assert!(failure.unwrap().may_resend(false));
    }

    #[tokio::test]
    async fn http_1_1_required_downgrades_and_resends() {
        let error =
            h2_request_error(|socket| reset_first_stream(socket, h2::Reason::HTTP_1_1_REQUIRED))
                .await;
        let failure = classify_h2(&error);
        assert_eq!(failure, Some(H2Failure::Http11Required), "{error:?}");
        assert!(failure.unwrap().downgrades());
        assert!(failure.unwrap().may_resend(false));
    }

    #[tokio::test]
    async fn other_stream_errors_are_left_to_the_caller() {
        let error =
            h2_request_error(|socket| reset_first_stream(socket, h2::Reason::INTERNAL_ERROR)).await;
        assert_eq!(classify_h2(&error), None, "{error:?}");

        // hyper reports h2 errors as the source of its own error type.
        #[derive(Debug)]
        struct Wrapper(h2::Error);
        impl std::fmt::Display for Wrapper {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("wrapped")
            }
        }
        impl std::error::Error for Wrapper {
            fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
                Some(&self.0)
            }
        }
        assert!(find_h2_error(&Wrapper(error)).is_some());
        assert!(find_h2_error(&std::fmt::Error).is_none());
    }

    #[tokio::test]
    async fn names_resolving_to_loopback_are_not_probed() {
        let verdict = probe(ProbeTarget::Host("localhost".into(), 9), H2C_PROBE_TIMEOUT).await;
        assert_eq!(verdict, Verdict::Http1);
    }
}
