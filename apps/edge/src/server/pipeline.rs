//! Shared request pipeline surface.
//!
//! Both the Pingora native data plane and the legacy in-process data plane use
//! this module for request preparation, so policy/preflight/classification
//! cannot drift between them. The implementation is being moved here
//! incrementally; the public entry points are stable.

use crate::cache::policy::{
    apply_key_ignore_default, cache_key_for_normalized_request, request_bypass_reason_headers,
    resolve_rule_policy, ResolvedPolicy, RuleAction,
};
use crate::config::registry::DomainEntry;
use crate::server::handler::{preflight_cacheable, AppState};
use axum::body::Body;
use axum::http::{HeaderMap, Method, Request, Response};
use std::sync::Arc;
use std::time::Instant;

/// Facts extracted once at the start of a request and shared by every stage.
pub struct RequestFacts<'a> {
    pub method: &'a Method,
    pub host: &'a str,
    pub path_only: &'a str,
    pub path_and_query: &'a str,
    pub headers: &'a HeaderMap,
    pub client_ip: &'a str,
    pub is_https: bool,
    pub start: Instant,
}

/// How the request should be handled by the data plane.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestClass {
    /// Cacheable GET/HEAD: native cache path.
    Cacheable,
    /// Cache bypass (cookie/auth/no-store/rule).
    Bypass(&'static str),
    /// WebSocket upgrade: Pingora H1 tunnel (not cacheable).
    Websocket,
    /// Non-cacheable dynamic method/request.
    Dynamic,
}

impl RequestClass {
    pub fn reason(self) -> Option<&'static str> {
        match self {
            RequestClass::Cacheable | RequestClass::Websocket => None,
            RequestClass::Bypass(reason) => Some(reason),
            RequestClass::Dynamic => Some("method"),
        }
    }

    pub fn is_cacheable(self) -> bool {
        matches!(self, RequestClass::Cacheable)
    }
}

pub struct RequestPipeline<'a> {
    state: &'a Arc<AppState>,
}

impl<'a> RequestPipeline<'a> {
    pub fn new(state: &'a Arc<AppState>) -> Self {
        Self { state }
    }

    /// Resolve the effective cache policy for this request.
    pub fn policy(&self, domain: &DomainEntry, facts: &RequestFacts<'_>) -> ResolvedPolicy {
        self.policy_for(
            domain,
            facts.method,
            facts.path_only,
            query_of(facts.path_and_query),
        )
    }

    pub fn policy_for(
        &self,
        domain: &DomainEntry,
        method: &Method,
        path_only: &str,
        query: &str,
    ) -> ResolvedPolicy {
        let mut policy = resolve_rule_policy(&domain.rules, path_only, method.as_str(), query);
        apply_key_ignore_default(&mut policy, &domain.cache_key_ignore_headers);
        apply_domain_cache_switch(&mut policy, domain.cache_enabled);
        policy
    }

    /// Classify the request for data-plane routing. Range handling stays with
    /// the caller because small slices and streaming ranges use different
    /// storage paths, but the bypass/revalidation decision is centralized here.
    pub fn classify(
        &self,
        _domain: &DomainEntry,
        facts: &RequestFacts<'_>,
        policy: &ResolvedPolicy,
    ) -> RequestClass {
        self.classify_request(facts.method, facts.headers, policy)
    }

    pub fn classify_request(
        &self,
        method: &Method,
        headers: &HeaderMap,
        policy: &ResolvedPolicy,
    ) -> RequestClass {
        if crate::server::websocket::is_websocket_upgrade(headers) {
            return RequestClass::Websocket;
        }
        if !crate::cache::policy::request_method_cacheable(method) {
            return RequestClass::Dynamic;
        }
        let reason = request_bypass_reason_headers(headers, policy.ignore_cookie).or_else(|| {
            if policy.action != RuleAction::Cache {
                Some("rule-bypass")
            } else {
                None
            }
        });
        match reason {
            Some(reason) => RequestClass::Bypass(reason),
            None => RequestClass::Cacheable,
        }
    }

    /// Shared cache key, including `Vary` request headers and ignore-query
    /// policy. HEAD reuses the GET representation.
    pub fn cache_key(
        &self,
        domain: &DomainEntry,
        facts: &RequestFacts<'_>,
        policy: &ResolvedPolicy,
    ) -> String {
        self.cache_key_for(
            domain,
            facts.method,
            facts.host,
            facts.path_only,
            facts.path_and_query,
            facts.headers,
            policy,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn cache_key_for(
        &self,
        domain: &DomainEntry,
        method: &Method,
        host: &str,
        path_only: &str,
        path_and_query: &str,
        headers: &HeaderMap,
        policy: &ResolvedPolicy,
    ) -> String {
        let ignore_query = policy.ignore_query.unwrap_or(domain.ignore_query_string);
        self.cache_key_with_ignore_query(
            method,
            host,
            path_only,
            path_and_query,
            headers,
            ignore_query,
            policy.key_ignore,
            &domain.version,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn cache_key_with_ignore_query(
        &self,
        method: &Method,
        host: &str,
        path_only: &str,
        path_and_query: &str,
        headers: &HeaderMap,
        ignore_query: bool,
        key_ignore: u8,
        domain_version: &str,
    ) -> String {
        let lookup_path = if ignore_query {
            path_only
        } else {
            path_and_query
        };
        let method = if *method == Method::HEAD {
            Method::GET.as_str()
        } else {
            method.as_str()
        };
        let mut key = cache_key_for_normalized_request(
            method,
            host,
            lookup_path,
            headers,
            key_ignore,
            &self.state.cache_key_ignore_headers,
        );
        // Persisted disk/L2 entries can outlive an edge process, so the
        // in-memory registry comparison alone cannot invalidate them after a
        // restart. A stable per-domain config suffix makes old entries
        // unreachable as soon as any effective domain setting changes.
        key.push_str("|cfg=");
        key.push_str(&domain_version_hash(domain_version));
        key
    }

    /// HTTPS redirect, bot challenge, CC, rate limit, WAF and hotlink checks.
    /// This is the same implementation used by both data planes today; moving
    /// the body of the function here is the next mechanical step.
    pub async fn preflight(
        &self,
        domain: &DomainEntry,
        facts: &RequestFacts<'_>,
    ) -> Option<Response<Body>> {
        preflight_cacheable(
            self.state,
            domain,
            facts.method,
            facts.host,
            facts.path_only,
            facts.path_and_query,
            facts.headers,
            facts.client_ip,
            facts.is_https,
            facts.start,
        )
        .await
    }
}

pub(crate) fn apply_domain_cache_switch(policy: &mut ResolvedPolicy, cache_enabled: bool) {
    if !cache_enabled {
        policy.action = RuleAction::Bypass;
    }
}

fn domain_version_hash(version: &str) -> String {
    // FNV-1a keeps the key suffix stable across process and compiler restarts.
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in version.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_domain_cache_forces_bypass() {
        let mut policy = ResolvedPolicy::default();
        apply_domain_cache_switch(&mut policy, false);
        assert_eq!(policy.action, RuleAction::Bypass);

        let mut enabled_policy = ResolvedPolicy::default();
        apply_domain_cache_switch(&mut enabled_policy, true);
        assert_eq!(enabled_policy.action, RuleAction::Cache);
    }

    #[test]
    fn domain_version_hash_is_stable_and_changes_with_config() {
        assert_eq!(domain_version_hash("v1"), domain_version_hash("v1"));
        assert_ne!(domain_version_hash("v1"), domain_version_hash("v2"));
    }
}

/// Build facts from an Axum request. Pingora has its own context builder but
/// passes the same fields to `RequestFacts`.
pub fn facts_from_request<'a>(
    req: &'a Request<Body>,
    host: &'a str,
    path_only: &'a str,
    path_and_query: &'a str,
    client_ip: &'a str,
    is_https: bool,
    start: Instant,
) -> RequestFacts<'a> {
    RequestFacts {
        method: req.method(),
        host,
        path_only,
        path_and_query,
        headers: req.headers(),
        client_ip,
        is_https,
        start,
    }
}

fn query_of(path_and_query: &str) -> &str {
    path_and_query
        .split_once('?')
        .map(|(_, query)| query)
        .unwrap_or("")
}
