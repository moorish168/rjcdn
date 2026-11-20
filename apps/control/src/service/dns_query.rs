//! Minimal async DNS client for control-plane checks.
//!
//! Used to see whether a site's NS delegation points at the platform and whether
//! an ACME DNS-01 TXT value is already served by the authoritative servers. It
//! replaces shelling out to `dig` / `nslookup` (not installed on minimal hosts)
//! and the blocking `getaddrinfo` path that stalled Tokio workers.

use std::net::{IpAddr, SocketAddr};
use std::time::Duration;
use tokio::net::UdpSocket;

pub const TYPE_A: u16 = 1;
pub const TYPE_NS: u16 = 2;
pub const TYPE_CNAME: u16 = 5;
pub const TYPE_TXT: u16 = 16;
pub const TYPE_AAAA: u16 = 28;

const QUERY_TIMEOUT: Duration = Duration::from_millis(2500);

/// One decoded answer record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Answer {
    Address(IpAddr),
    /// NS target, lower-case, no trailing dot.
    Name(String),
    /// TXT character-strings joined.
    Text(String),
    Other,
}

fn encode_query(id: u16, name: &str, qtype: u16, recursion_desired: bool) -> Vec<u8> {
    let mut out = Vec::with_capacity(64);
    out.extend_from_slice(&id.to_be_bytes());
    let flags: u16 = if recursion_desired { 0x0100 } else { 0 };
    out.extend_from_slice(&flags.to_be_bytes());
    out.extend_from_slice(&1u16.to_be_bytes()); // QDCOUNT
    out.extend_from_slice(&[0, 0, 0, 0, 0, 0]); // AN/NS/AR
    for label in name.trim_end_matches('.').split('.') {
        if label.is_empty() {
            continue;
        }
        let bytes = label.as_bytes();
        let len = bytes.len().min(63);
        out.push(len as u8);
        out.extend_from_slice(&bytes[..len]);
    }
    out.push(0);
    out.extend_from_slice(&qtype.to_be_bytes());
    out.extend_from_slice(&1u16.to_be_bytes()); // IN
    out
}

/// Read a (possibly compressed) name starting at `pos`; returns the name and the
/// offset just past it in the original stream.
fn read_name(msg: &[u8], mut pos: usize) -> Option<(String, usize)> {
    let mut labels: Vec<String> = Vec::new();
    let mut end: Option<usize> = None;
    let mut jumps = 0;
    loop {
        let len = *msg.get(pos)? as usize;
        if len & 0xC0 == 0xC0 {
            let b2 = *msg.get(pos + 1)? as usize;
            if end.is_none() {
                end = Some(pos + 2);
            }
            pos = ((len & 0x3F) << 8) | b2;
            jumps += 1;
            if jumps > 32 {
                return None;
            }
            continue;
        }
        if len == 0 {
            let next = end.unwrap_or(pos + 1);
            return Some((labels.join(".").to_ascii_lowercase(), next));
        }
        let label = msg.get(pos + 1..pos + 1 + len)?;
        labels.push(String::from_utf8_lossy(label).into_owned());
        pos += 1 + len;
    }
}

fn parse_response(msg: &[u8], id: u16) -> Result<Vec<(u16, Answer)>, String> {
    if msg.len() < 12 {
        return Err("short DNS response".into());
    }
    if u16::from_be_bytes([msg[0], msg[1]]) != id {
        return Err("DNS response id mismatch".into());
    }
    let flags = u16::from_be_bytes([msg[2], msg[3]]);
    if flags & 0x8000 == 0 {
        return Err("not a DNS response".into());
    }
    if flags & 0x0200 != 0 {
        return Err("truncated DNS response".into());
    }
    let rcode = flags & 0x000F;
    if rcode != 0 && rcode != 3 {
        return Err(format!("DNS rcode {rcode}"));
    }
    let qd = u16::from_be_bytes([msg[4], msg[5]]) as usize;
    let an = u16::from_be_bytes([msg[6], msg[7]]) as usize;
    let mut pos = 12;
    for _ in 0..qd {
        let (_, next) = read_name(msg, pos).ok_or("bad question name")?;
        pos = next + 4;
    }
    let mut out = Vec::new();
    for _ in 0..an {
        let (_, next) = read_name(msg, pos).ok_or("bad answer name")?;
        pos = next;
        let header = msg.get(pos..pos + 10).ok_or("short answer")?;
        let rtype = u16::from_be_bytes([header[0], header[1]]);
        let rdlen = u16::from_be_bytes([header[8], header[9]]) as usize;
        pos += 10;
        let rdata = msg.get(pos..pos + rdlen).ok_or("short rdata")?;
        let answer = match rtype {
            TYPE_A if rdlen == 4 => {
                Answer::Address(IpAddr::from([rdata[0], rdata[1], rdata[2], rdata[3]]))
            }
            TYPE_AAAA if rdlen == 16 => {
                let mut b = [0u8; 16];
                b.copy_from_slice(rdata);
                Answer::Address(IpAddr::from(b))
            }
            TYPE_NS | TYPE_CNAME => match read_name(msg, pos) {
                Some((name, _)) => Answer::Name(name),
                None => Answer::Other,
            },
            TYPE_TXT => {
                let mut text = String::new();
                let mut i = 0;
                while i < rdata.len() {
                    let len = rdata[i] as usize;
                    let chunk = rdata.get(i + 1..i + 1 + len).unwrap_or_default();
                    text.push_str(&String::from_utf8_lossy(chunk));
                    i += 1 + len;
                }
                Answer::Text(text)
            }
            _ => Answer::Other,
        };
        out.push((rtype, answer));
        pos += rdlen;
    }
    Ok(out)
}

/// Send one query over UDP and decode the answer section.
pub async fn query(
    server: SocketAddr,
    name: &str,
    qtype: u16,
    recursion_desired: bool,
) -> Result<Vec<Answer>, String> {
    let bind = if server.is_ipv4() { "0.0.0.0:0" } else { "[::]:0" };
    let sock = UdpSocket::bind(bind).await.map_err(|e| e.to_string())?;
    let id: u16 = rand_id();
    let packet = encode_query(id, name, qtype, recursion_desired);
    sock.send_to(&packet, server).await.map_err(|e| e.to_string())?;
    let mut buf = vec![0u8; 4096];
    let deadline = tokio::time::Instant::now() + QUERY_TIMEOUT;
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        let (n, from) = tokio::time::timeout(remaining, sock.recv_from(&mut buf))
            .await
            .map_err(|_| format!("DNS query to {server} timed out"))?
            .map_err(|e| e.to_string())?;
        // Ignore datagrams from anyone but the server we asked (spoofing).
        if from.ip() != server.ip() {
            continue;
        }
        return parse_response(&buf[..n], id).map(|answers| {
            answers
                .into_iter()
                .filter(|(t, _)| *t == qtype)
                .map(|(_, a)| a)
                .collect()
        });
    }
}

fn rand_id() -> u16 {
    let mut b = [0u8; 2];
    if ring::rand::SecureRandom::fill(&ring::rand::SystemRandom::new(), &mut b).is_err() {
        return std::process::id() as u16;
    }
    u16::from_be_bytes(b)
}

/// Recursive resolvers of this host (`/etc/resolv.conf`).
pub fn system_resolvers() -> Vec<SocketAddr> {
    let content = std::fs::read_to_string("/etc/resolv.conf").unwrap_or_default();
    let mut out: Vec<SocketAddr> = content
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            let rest = line.strip_prefix("nameserver")?;
            let ip: IpAddr = rest.trim().split('%').next()?.parse().ok()?;
            Some(SocketAddr::new(ip, 53))
        })
        .collect();
    if out.is_empty() {
        out.push(SocketAddr::from(([127, 0, 0, 53], 53)));
    }
    out
}

/// NS names currently delegated for `domain`, via the system resolvers.
pub async fn lookup_ns(domain: &str) -> Vec<String> {
    for server in system_resolvers() {
        if let Ok(answers) = query(server, domain, TYPE_NS, true).await {
            let names: Vec<String> = answers
                .into_iter()
                .filter_map(|a| match a {
                    Answer::Name(n) => Some(n.trim_end_matches('.').to_string()),
                    _ => None,
                })
                .collect();
            return names;
        }
    }
    Vec::new()
}

/// First CNAME target for `name` via the system resolvers (one hop).
///
/// Used for ACME DNS-01 when a customer CNAME (including a synthesized answer
/// from `*.base`) points `_acme-challenge.<base>` into the platform schedule zone.
pub async fn lookup_cname(name: &str) -> Option<String> {
    let name = name.trim().trim_end_matches('.').to_ascii_lowercase();
    if name.is_empty() {
        return None;
    }
    for server in system_resolvers() {
        if let Ok(answers) = query(server, &name, TYPE_CNAME, true).await {
            if let Some(target) = answers.into_iter().find_map(|a| match a {
                Answer::Name(n) => {
                    let n = n.trim_end_matches('.').to_ascii_lowercase();
                    (!n.is_empty()).then_some(n)
                }
                _ => None,
            }) {
                return Some(target);
            }
        }
    }
    None
}

/// TXT values for `name` as served by the authoritative server at `server`.
pub async fn authoritative_txt(server: SocketAddr, name: &str) -> Result<Vec<String>, String> {
    let answers = query(server, name, TYPE_TXT, false).await?;
    Ok(answers
        .into_iter()
        .filter_map(|a| match a {
            Answer::Text(t) => Some(t),
            _ => None,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn response_with(answers: &[(&str, u16, Vec<u8>)]) -> Vec<u8> {
        let mut msg = Vec::new();
        msg.extend_from_slice(&0x1234u16.to_be_bytes());
        msg.extend_from_slice(&0x8180u16.to_be_bytes());
        msg.extend_from_slice(&1u16.to_be_bytes());
        msg.extend_from_slice(&(answers.len() as u16).to_be_bytes());
        msg.extend_from_slice(&[0, 0, 0, 0]);
        // question: example.com IN TXT at offset 12
        msg.extend_from_slice(&[7]);
        msg.extend_from_slice(b"example");
        msg.extend_from_slice(&[3]);
        msg.extend_from_slice(b"com");
        msg.push(0);
        msg.extend_from_slice(&TYPE_TXT.to_be_bytes());
        msg.extend_from_slice(&1u16.to_be_bytes());
        for (_, rtype, rdata) in answers {
            msg.extend_from_slice(&[0xC0, 12]); // pointer to question name
            msg.extend_from_slice(&rtype.to_be_bytes());
            msg.extend_from_slice(&1u16.to_be_bytes());
            msg.extend_from_slice(&30u32.to_be_bytes());
            msg.extend_from_slice(&(rdata.len() as u16).to_be_bytes());
            msg.extend_from_slice(rdata);
        }
        msg
    }

    #[test]
    fn parses_txt_split_strings() {
        let mut rdata = vec![3];
        rdata.extend_from_slice(b"abc");
        rdata.push(2);
        rdata.extend_from_slice(b"de");
        let msg = response_with(&[("example.com", TYPE_TXT, rdata)]);
        let parsed = parse_response(&msg, 0x1234).unwrap();
        assert_eq!(parsed, vec![(TYPE_TXT, Answer::Text("abcde".into()))]);
    }

    #[test]
    fn parses_compressed_ns_target() {
        // ns1 + pointer to "example.com" in the question.
        let rdata = vec![3, b'n', b's', b'1', 0xC0, 12];
        let msg = response_with(&[("example.com", TYPE_NS, rdata)]);
        let parsed = parse_response(&msg, 0x1234).unwrap();
        assert_eq!(parsed, vec![(TYPE_NS, Answer::Name("ns1.example.com".into()))]);
    }

    #[test]
    fn rejects_wrong_id_and_pointer_loops() {
        let msg = response_with(&[]);
        assert!(parse_response(&msg, 0x9999).is_err());
        let looped = [0xC0u8, 0];
        assert!(read_name(&looped, 0).is_none());
    }
}
