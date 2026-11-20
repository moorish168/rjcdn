//! Check the CDN 主域 the way the rest of the internet sees it.
//!
//! `dns_probe` asks each DNS node directly from the control plane. That can
//! succeed while real resolvers still fail: the cloud security group may let
//! only the control host in, the registrar may still list other name servers,
//! or the glue may point somewhere else. Asking public DNS-over-HTTPS
//! resolvers shows what visitors' resolvers actually get.

use std::time::Duration;

use serde::Serialize;
use serde_json::Value;

const TIMEOUT: Duration = Duration::from_secs(6);

const TYPE_A: u64 = 1;
const TYPE_NS: u64 = 2;
const TYPE_SOA: u64 = 6;

struct Resolver {
    key: &'static str,
    name: &'static str,
    url: &'static str,
    /// Cloudflare answers in JSON only when asked for it.
    accept_json: bool,
}

const RESOLVERS: [Resolver; 3] = [
    Resolver {
        key: "google",
        name: "Google",
        url: "https://dns.google/resolve",
        accept_json: false,
    },
    Resolver {
        key: "cloudflare",
        name: "Cloudflare",
        url: "https://cloudflare-dns.com/dns-query",
        accept_json: true,
    },
    Resolver {
        key: "alidns",
        name: "阿里 DNS",
        url: "https://dns.alidns.com/resolve",
        accept_json: false,
    },
];

/// One DoH lookup as the resolver reported it.
#[derive(Debug, Clone, PartialEq)]
pub enum Lookup {
    /// The resolver answered: DNS RCODE, `(type, data)` answers, its notes.
    Answered {
        rcode: u64,
        answers: Vec<(u64, String)>,
        comment: String,
    },
    /// The DoH service itself could not be reached or sent something else.
    Unavailable(String),
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CheckState {
    /// Resolves, the NS set matches the console, current serial.
    Ok,
    /// Resolves and matches, but the resolver still caches an older serial.
    Stale,
    /// Our servers answer, but the probe name got no address.
    NoAnswer,
    /// The resolver sees other name servers than the console lists.
    WrongNs,
    /// SERVFAIL: the resolver could not get an answer from our servers.
    Unreachable,
    /// NXDOMAIN for the zone itself.
    NotFound,
    /// Any other unexpected answer.
    Error,
    /// The DoH service was unreachable from the control plane; no verdict.
    Skipped,
}

#[derive(Debug, Clone, Serialize)]
pub struct ResolverCheck {
    pub key: String,
    pub name: String,
    pub state: CheckState,
    /// NS names the resolver returned (lower-case, no trailing dot).
    pub ns: Vec<String>,
    pub soa_serial: Option<i64>,
    /// An address the probe name resolved to (proves scheduling works).
    pub sample_ip: Option<String>,
    /// What this means and what to do, for the console.
    pub detail: String,
    /// The resolver's own note (e.g. "Name servers did not respond").
    pub comment: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PublicCheck {
    pub zone: String,
    pub expected_ns: Vec<String>,
    pub expected_serial: i64,
    pub probe_name: String,
    /// `ok` | `partial` | `fail` | `unknown`
    pub verdict: String,
    pub summary: String,
    pub resolvers: Vec<ResolverCheck>,
    pub checked_at: chrono::DateTime<chrono::Utc>,
}

fn normalize_name(raw: &str) -> String {
    raw.trim().trim_end_matches('.').to_ascii_lowercase()
}

/// Parse a Google/Cloudflare/AliDNS JSON answer.
pub fn parse_doh(body: &str) -> Lookup {
    let Ok(v) = serde_json::from_str::<Value>(body) else {
        return Lookup::Unavailable("返回的不是 JSON".into());
    };
    let Some(rcode) = v.get("Status").and_then(Value::as_u64) else {
        return Lookup::Unavailable("返回内容缺少 Status".into());
    };
    let answers = v
        .get("Answer")
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .filter_map(|a| {
                    let rtype = a.get("type").and_then(Value::as_u64)?;
                    let data = a.get("data").and_then(Value::as_str)?;
                    Some((rtype, data.trim().to_string()))
                })
                .collect()
        })
        .unwrap_or_default();
    let mut notes: Vec<String> = Vec::new();
    match v.get("Comment") {
        Some(Value::String(s)) => notes.push(s.trim().to_string()),
        Some(Value::Array(list)) => {
            notes.extend(list.iter().filter_map(|x| x.as_str().map(|s| s.trim().to_string())))
        }
        _ => {}
    }
    if let Some(list) = v.get("extended_dns_errors").and_then(Value::as_array) {
        for e in list {
            let code = e.get("info_code").and_then(Value::as_u64).unwrap_or(0);
            let text = e.get("extra_text").and_then(Value::as_str).unwrap_or("").trim();
            notes.push(format!("EDE {code}: {text}").trim_end_matches(": ").to_string());
        }
    }
    notes.retain(|n| !n.is_empty());
    Lookup::Answered {
        rcode,
        answers,
        comment: notes.join(" "),
    }
}

fn soa_serial(answers: &[(u64, String)]) -> Option<i64> {
    answers
        .iter()
        .find(|(t, _)| *t == TYPE_SOA)
        .and_then(|(_, data)| data.split_whitespace().nth(2))
        .and_then(|s| s.parse::<i64>().ok())
}

/// Judge one resolver from its NS, SOA and probe-name lookups.
pub fn classify(
    expected_ns: &[String],
    expected_serial: i64,
    ns: &Lookup,
    soa: &Lookup,
    probe: &Lookup,
) -> ResolverCheck {
    let mut out = ResolverCheck {
        key: String::new(),
        name: String::new(),
        state: CheckState::Error,
        ns: Vec::new(),
        soa_serial: None,
        sample_ip: None,
        detail: String::new(),
        comment: String::new(),
    };
    let (rcode, answers, comment) = match ns {
        Lookup::Unavailable(err) => {
            out.state = CheckState::Skipped;
            out.detail = format!("控制面访问不了它的检测接口（{err}），这一项跳过");
            return out;
        }
        Lookup::Answered {
            rcode,
            answers,
            comment,
        } => (*rcode, answers, comment),
    };
    out.comment = comment.clone();
    match rcode {
        0 => {}
        2 => {
            out.state = CheckState::Unreachable;
            out.detail = "联系不上平台的 DNS 服务器：确认 lagran-dns 在运行，云安全组放行了 UDP 和 TCP 53 端口。\
                          刚启动或刚放行时，公共 DNS 可能还缓存着失败结果，过几分钟再测"
                .into();
            return out;
        }
        3 => {
            out.state = CheckState::NotFound;
            out.detail = "查不到这个域名：确认域名已注册、没有过期，并且在注册商处设置了 DNS 服务器".into();
            return out;
        }
        5 => {
            out.state = CheckState::Error;
            out.detail = "公共 DNS 拒绝了这次查询（REFUSED）".into();
            return out;
        }
        other => {
            out.state = CheckState::Error;
            out.detail = format!("公共 DNS 返回错误码 {other}");
            return out;
        }
    }

    let mut got: Vec<String> = answers
        .iter()
        .filter(|(t, _)| *t == TYPE_NS)
        .map(|(_, d)| normalize_name(d))
        .collect();
    got.sort();
    got.dedup();
    out.ns = got.clone();
    let mut want: Vec<String> = expected_ns.iter().map(|n| normalize_name(n)).collect();
    want.sort();
    want.dedup();
    if got.is_empty() {
        out.state = CheckState::Error;
        out.detail = "没有返回 NS 记录".into();
        return out;
    }
    if got != want {
        out.state = CheckState::WrongNs;
        out.detail = format!(
            "看到的 DNS 服务器是 {}，和这里填写的 {} 不一致。两边必须相同：\
             注册商处的「DNS 服务器」要改成这里的服务器名，或者把这里改成注册商处的。\
             刚改过注册商的话，注册局和公共 DNS 缓存可能要几分钟到两天才更新",
            got.join("、"),
            want.join("、"),
        );
        return out;
    }

    if let Lookup::Answered {
        rcode: 0, answers, ..
    } = soa
    {
        out.soa_serial = soa_serial(answers);
    }

    match probe {
        Lookup::Answered {
            rcode: 0, answers, ..
        } => {
            out.sample_ip = answers
                .iter()
                .find(|(t, _)| *t == TYPE_A)
                .map(|(_, d)| d.clone());
            if out.sample_ip.is_none() {
                out.state = CheckState::NoAnswer;
                out.detail = "能联系上平台 DNS，但测试子域没有返回 IP：确认有在线的边缘节点，或在上面设置兜底 IP".into();
                return out;
            }
        }
        Lookup::Answered { rcode: 2, .. } => {
            out.state = CheckState::NoAnswer;
            out.detail = "能联系上平台 DNS，但测试子域解析失败（SERVFAIL）：平台调度没有返回节点，\
                          检查控制面和边缘节点是否正常，或在上面设置兜底 IP"
                .into();
            return out;
        }
        Lookup::Answered { rcode, .. } => {
            out.state = CheckState::Error;
            out.detail = format!("测试子域返回错误码 {rcode}");
            return out;
        }
        // The zone itself checked out; a failed probe request says nothing new.
        Lookup::Unavailable(_) => {}
    }

    match out.soa_serial {
        Some(seen) if seen < expected_serial => {
            out.state = CheckState::Stale;
            out.detail = format!(
                "能解析，但还缓存着旧配置（序号 {seen}，最新 {expected_serial}），缓存过期后自动更新"
            );
        }
        _ => {
            out.state = CheckState::Ok;
            out.detail = match &out.sample_ip {
                Some(ip) => format!("解析正常，测试子域返回 {ip}"),
                None => "解析正常".into(),
            };
        }
    }
    out
}

/// Overall verdict and one-line summary from the per-resolver results.
pub fn summarize(zone: &str, results: &[ResolverCheck]) -> (String, String) {
    let judged: Vec<&ResolverCheck> = results
        .iter()
        .filter(|r| r.state != CheckState::Skipped)
        .collect();
    if judged.is_empty() {
        return (
            "unknown".into(),
            "控制面访问不了任何公共 DNS 的检测接口，无法从外部检测：确认控制面服务器能访问外网 HTTPS".into(),
        );
    }
    let good: Vec<&str> = judged
        .iter()
        .filter(|r| matches!(r.state, CheckState::Ok | CheckState::Stale))
        .map(|r| r.name.as_str())
        .collect();
    let bad: Vec<&ResolverCheck> = judged
        .iter()
        .copied()
        .filter(|r| !matches!(r.state, CheckState::Ok | CheckState::Stale))
        .collect();
    if bad.is_empty() {
        let mut summary = format!("{} 都能正常解析 {zone}，配置已在公网生效", good.join("、"));
        if judged.iter().any(|r| r.state == CheckState::Stale) {
            summary.push_str("（个别还缓存着旧配置，稍后自动更新）");
        }
        return ("ok".into(), summary);
    }
    if !good.is_empty() {
        let names: Vec<&str> = bad.iter().map(|r| r.name.as_str()).collect();
        return (
            "partial".into(),
            format!(
                "{} 能解析，{} 还不行。刚改过配置或刚放行端口时属正常，几分钟后再测；否则看下方说明",
                good.join("、"),
                names.join("、"),
            ),
        );
    }
    // Lead with the most actionable cause.
    let rank = |s: CheckState| match s {
        CheckState::WrongNs => 0,
        CheckState::NotFound => 1,
        CheckState::Unreachable => 2,
        CheckState::NoAnswer => 3,
        _ => 4,
    };
    let first = bad
        .iter()
        .min_by_key(|r| rank(r.state))
        .map(|r| r.detail.clone())
        .unwrap_or_default();
    ("fail".into(), first)
}

async fn lookup(client: &reqwest::Client, resolver: &Resolver, name: &str, rtype: &str) -> Lookup {
    let Ok(url) = reqwest::Url::parse_with_params(resolver.url, &[("name", name), ("type", rtype)])
    else {
        return Lookup::Unavailable("地址无效".into());
    };
    let mut req = client.get(url);
    if resolver.accept_json {
        req = req.header(reqwest::header::ACCEPT, "application/dns-json");
    }
    let resp = match req.send().await {
        Ok(resp) => resp,
        Err(e) if e.is_timeout() => return Lookup::Unavailable("超时".into()),
        Err(e) if e.is_connect() => return Lookup::Unavailable("连接失败".into()),
        Err(_) => return Lookup::Unavailable("请求失败".into()),
    };
    if !resp.status().is_success() {
        return Lookup::Unavailable(format!("HTTP {}", resp.status().as_u16()));
    }
    match resp.text().await {
        Ok(body) => parse_doh(&body),
        Err(_) => Lookup::Unavailable("读取响应失败".into()),
    }
}

async fn check_one(
    client: &reqwest::Client,
    resolver: &Resolver,
    zone: &str,
    probe_name: &str,
    expected_ns: &[String],
    expected_serial: i64,
) -> ResolverCheck {
    let (ns, soa, probe) = tokio::join!(
        lookup(client, resolver, zone, "NS"),
        lookup(client, resolver, zone, "SOA"),
        lookup(client, resolver, probe_name, "A"),
    );
    let mut out = classify(expected_ns, expected_serial, &ns, &soa, &probe);
    out.key = resolver.key.into();
    out.name = resolver.name.into();
    out
}

/// NS names for `domain` via public DoH (Google / Cloudflare / AliDNS).
///
/// Used when the control host's `/etc/resolv.conf` cannot see the delegation
/// (common on cloud images whose stub resolver returns empty or times out).
pub async fn lookup_ns_names(domain: &str) -> Vec<String> {
    let zone = normalize_name(domain);
    if zone.is_empty() {
        return Vec::new();
    }
    let client = reqwest::Client::builder()
        .timeout(TIMEOUT)
        .user_agent("lagran-control")
        .build()
        .unwrap_or_else(|_| reqwest::Client::new());
    let (a, b, c) = tokio::join!(
        lookup(&client, &RESOLVERS[0], &zone, "NS"),
        lookup(&client, &RESOLVERS[1], &zone, "NS"),
        lookup(&client, &RESOLVERS[2], &zone, "NS"),
    );
    for hit in [a, b, c] {
        if let Lookup::Answered { rcode: 0, answers, .. } = hit {
            let mut names: Vec<String> = answers
                .into_iter()
                .filter(|(t, _)| *t == TYPE_NS)
                .map(|(_, d)| normalize_name(&d))
                .filter(|n| !n.is_empty())
                .collect();
            if !names.is_empty() {
                names.sort();
                names.dedup();
                return names;
            }
        }
    }
    Vec::new()
}

/// Resolve the CDN 主域 through every public resolver in parallel.
pub async fn run(zone: &str, expected_ns: &[String], expected_serial: i64) -> PublicCheck {
    let zone = normalize_name(zone);
    // A fresh label every time, so no resolver can answer from its cache.
    let probe_name = format!(
        "lagran-check-{}.{zone}",
        &uuid::Uuid::new_v4().simple().to_string()[..8]
    );
    let client = reqwest::Client::builder()
        .timeout(TIMEOUT)
        .user_agent("lagran-control")
        .build()
        .unwrap_or_else(|_| reqwest::Client::new());
    let (a, b, c) = tokio::join!(
        check_one(&client, &RESOLVERS[0], &zone, &probe_name, expected_ns, expected_serial),
        check_one(&client, &RESOLVERS[1], &zone, &probe_name, expected_ns, expected_serial),
        check_one(&client, &RESOLVERS[2], &zone, &probe_name, expected_ns, expected_serial),
    );
    let resolvers = vec![a, b, c];
    let (verdict, summary) = summarize(&zone, &resolvers);
    PublicCheck {
        zone,
        expected_ns: expected_ns.iter().map(|n| normalize_name(n)).collect(),
        expected_serial,
        probe_name,
        verdict,
        summary,
        resolvers,
        checked_at: chrono::Utc::now(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ns() -> Vec<String> {
        vec!["ns1.nxdns.cc".into(), "ns2.nxdns.cc".into()]
    }

    fn answered(rcode: u64, answers: &[(u64, &str)], comment: &str) -> Lookup {
        Lookup::Answered {
            rcode,
            answers: answers.iter().map(|(t, d)| (*t, d.to_string())).collect(),
            comment: comment.into(),
        }
    }

    fn ok_ns() -> Lookup {
        answered(0, &[(2, "ns2.nxdns.cc."), (2, "NS1.nxdns.cc.")], "")
    }

    fn soa(serial: i64) -> Lookup {
        answered(
            0,
            &[(6, &format!("ns1.nxdns.cc. hostmaster.nxdns.cc. {serial} 3600 600 86400 60"))],
            "",
        )
    }

    fn probe_ip() -> Lookup {
        answered(0, &[(1, "47.237.129.74")], "")
    }

    #[test]
    fn parses_google_servfail_with_ede() {
        let body = r#"{"Status":2,"TC":false,"Question":[{"name":"nxdns.cc.","type":2}],
            "Comment":"Name servers did not respond [47.237.129.74].",
            "extended_dns_errors":[{"info_code":22,"extra_text":"At delegation nxdns.cc for nxdns.cc/ns"}]}"#;
        let Lookup::Answered { rcode, answers, comment } = parse_doh(body) else {
            panic!("expected an answer");
        };
        assert_eq!(rcode, 2);
        assert!(answers.is_empty());
        assert!(comment.contains("did not respond"));
        assert!(comment.contains("EDE 22"));
    }

    #[test]
    fn parses_cloudflare_comment_array_and_answers() {
        let body = r#"{"Status":0,"Answer":[{"name":"nxdns.cc","type":2,"TTL":60,"data":"ns1.nxdns.cc."}],
            "Comment":["EDE(22): No Reachable Authority"]}"#;
        let Lookup::Answered { rcode, answers, comment } = parse_doh(body) else {
            panic!("expected an answer");
        };
        assert_eq!(rcode, 0);
        assert_eq!(answers, vec![(2, "ns1.nxdns.cc.".to_string())]);
        assert_eq!(comment, "EDE(22): No Reachable Authority");
    }

    #[test]
    fn rejects_non_json_bodies() {
        assert!(matches!(parse_doh("<html>blocked</html>"), Lookup::Unavailable(_)));
        assert!(matches!(parse_doh("{}"), Lookup::Unavailable(_)));
    }

    #[test]
    fn healthy_zone_is_ok_regardless_of_case_and_order() {
        let r = classify(&ns(), 6, &ok_ns(), &soa(6), &probe_ip());
        assert_eq!(r.state, CheckState::Ok);
        assert_eq!(r.ns, ns());
        assert_eq!(r.soa_serial, Some(6));
        assert_eq!(r.sample_ip.as_deref(), Some("47.237.129.74"));
    }

    #[test]
    fn older_serial_is_stale_not_failed() {
        let r = classify(&ns(), 7, &ok_ns(), &soa(6), &probe_ip());
        assert_eq!(r.state, CheckState::Stale);
    }

    #[test]
    fn servfail_means_our_servers_are_unreachable() {
        let r = classify(
            &ns(),
            6,
            &answered(2, &[], "Name servers did not respond [47.237.129.74]."),
            &answered(2, &[], ""),
            &answered(2, &[], ""),
        );
        assert_eq!(r.state, CheckState::Unreachable);
        assert!(r.detail.contains("安全组"));
        assert!(r.comment.contains("did not respond"));
    }

    #[test]
    fn registrar_still_on_old_servers_is_wrong_ns() {
        let old = answered(0, &[(2, "dns1.hichina.com."), (2, "dns2.hichina.com.")], "");
        let r = classify(&ns(), 6, &old, &soa(1), &probe_ip());
        assert_eq!(r.state, CheckState::WrongNs);
        assert_eq!(r.ns, vec!["dns1.hichina.com", "dns2.hichina.com"]);
        assert!(r.detail.contains("ns1.nxdns.cc"));
    }

    #[test]
    fn nxdomain_is_not_found() {
        let r = classify(&ns(), 6, &answered(3, &[], ""), &answered(3, &[], ""), &answered(3, &[], ""));
        assert_eq!(r.state, CheckState::NotFound);
    }

    #[test]
    fn probe_without_address_means_no_edge_answer() {
        let r = classify(&ns(), 6, &ok_ns(), &soa(6), &answered(0, &[], ""));
        assert_eq!(r.state, CheckState::NoAnswer);
        let r = classify(&ns(), 6, &ok_ns(), &soa(6), &answered(2, &[], ""));
        assert_eq!(r.state, CheckState::NoAnswer);
    }

    #[test]
    fn unreachable_doh_service_is_skipped() {
        let down = Lookup::Unavailable("超时".into());
        let r = classify(&ns(), 6, &down, &down, &down);
        assert_eq!(r.state, CheckState::Skipped);
        // A failed probe request alone does not spoil a zone that checked out.
        let r = classify(&ns(), 6, &ok_ns(), &soa(6), &down);
        assert_eq!(r.state, CheckState::Ok);
        assert_eq!(r.sample_ip, None);
    }

    fn named(name: &str, state: CheckState, detail: &str) -> ResolverCheck {
        ResolverCheck {
            key: name.to_ascii_lowercase(),
            name: name.into(),
            state,
            ns: vec![],
            soa_serial: None,
            sample_ip: None,
            detail: detail.into(),
            comment: String::new(),
        }
    }

    #[test]
    fn summary_covers_all_outcomes() {
        let all_ok = [
            named("Google", CheckState::Ok, ""),
            named("Cloudflare", CheckState::Stale, ""),
            named("阿里 DNS", CheckState::Skipped, ""),
        ];
        let (verdict, summary) = summarize("nxdns.cc", &all_ok);
        assert_eq!(verdict, "ok");
        assert!(summary.contains("Google、Cloudflare"));
        assert!(summary.contains("旧配置"));

        let partial = [
            named("Google", CheckState::Unreachable, "u"),
            named("Cloudflare", CheckState::Ok, ""),
        ];
        assert_eq!(summarize("nxdns.cc", &partial).0, "partial");

        let failed = [
            named("Google", CheckState::Unreachable, "unreachable"),
            named("Cloudflare", CheckState::WrongNs, "wrong ns"),
        ];
        let (verdict, summary) = summarize("nxdns.cc", &failed);
        assert_eq!(verdict, "fail");
        assert_eq!(summary, "wrong ns", "the registrar problem is the one to fix first");

        let skipped = [named("Google", CheckState::Skipped, "")];
        assert_eq!(summarize("nxdns.cc", &skipped).0, "unknown");
    }
}
