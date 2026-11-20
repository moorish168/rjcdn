//! Shared application state

use crate::config::AppConfig;
use crate::middleware::login_limit::LoginLimiter;
use crate::service::geoip::GeoIpCatalog;
use sqlx::PgPool;
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, RwLock};

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub config: Arc<AppConfig>,
    /// Hot-reloadable geo table used by the scheduler / DNS line match.
    pub geo: Arc<RwLock<crate::scheduler::GeoTable>>,
    /// On-disk GeoLite2-Country catalog (hash + path) shared with edge agents.
    pub geoip: GeoIpCatalog,
    pub schedule_ttl: u64,
    /// Short-lived snapshot of the node table used by GeoDNS scheduling.
    pub nodes: Arc<crate::scheduler::NodeSnapshotCache>,
    /// Monotonic ticket used to rotate among equally-scored nodes.
    /// Per control process: multiple replicas rotate independently.
    pub schedule_rotation: Arc<AtomicU64>,
    pub login_limiter: Arc<LoginLimiter>,
    pub captcha: Arc<crate::service::captcha::CaptchaStore>,
    pub config_cache: Arc<crate::service::config_cache::ConfigCache>,
    pub redis: Option<Arc<crate::service::redis::RedisService>>,
}

impl AppState {
    pub fn new(
        pool: PgPool,
        config: AppConfig,
        redis: Option<Arc<crate::service::redis::RedisService>>,
    ) -> Self {
        let geo =
            crate::scheduler::GeoTable::load_with_ipv6(&config.geoip_path, &config.geoip_ipv6_path);
        let geoip = GeoIpCatalog::new(config.geoip_path.clone());
        // Align catalog "loaded" with whatever the table actually opened.
        geoip.refresh_from_disk();
        let schedule_ttl = config.schedule_ttl;
        let config_cache_ttl_secs = config.config_cache_ttl_secs;
        let nodes = Arc::new(crate::scheduler::NodeSnapshotCache::new(
            config.schedule_node_cache_secs,
        ));
        Self {
            pool,
            config: Arc::new(config),
            geo: Arc::new(RwLock::new(geo)),
            geoip,
            schedule_ttl,
            nodes,
            schedule_rotation: Arc::new(AtomicU64::new(0)),
            login_limiter: Arc::new(LoginLimiter::new()),
            captcha: Arc::new(crate::service::captcha::CaptchaStore::new()),
            config_cache: Arc::new(crate::service::config_cache::ConfigCache::new(
                config_cache_ttl_secs,
            )),
            redis,
        }
    }

    pub fn replace_geo(&self, table: crate::scheduler::GeoTable) {
        if let Ok(mut g) = self.geo.write() {
            *g = table;
        }
    }
}
