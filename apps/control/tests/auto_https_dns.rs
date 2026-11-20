//! Integration tests for automatic HTTPS scheduling and the DNS records it
//! manages: the retry/backoff state machine, ACME DNS-01 TXT values and the
//! proxied records of NS-mode sites.

mod common;

use chrono::{Duration, Utc};
use common::TestDb;
use control::repository;
use uuid::Uuid;

async fn insert_domain(db: &TestDb, name: &str) -> Uuid {
    sqlx::query_scalar(
        r#"
        INSERT INTO domains (name, origin_host, origin_port, origin_proto, status)
        VALUES ($1, 'origin.test', 80, 'http', 'active')
        RETURNING id
        "#,
    )
    .bind(name)
    .fetch_one(&db.pool)
    .await
    .expect("insert domain")
}

async fn insert_project(db: &TestDb, owner_email: &str) -> Uuid {
    let owner: Uuid =
        sqlx::query_scalar("INSERT INTO users (email, password_hash) VALUES ($1, 'x') RETURNING id")
            .bind(owner_email)
            .fetch_one(&db.pool)
            .await
            .expect("insert user");
    sqlx::query_scalar("INSERT INTO projects (name, owner_id) VALUES ('p', $1) RETURNING id")
        .bind(owner)
        .fetch_one(&db.pool)
        .await
        .expect("insert project")
}

async fn insert_zone(db: &TestDb, name: &str, kind: &str, project_id: Option<Uuid>) -> Uuid {
    sqlx::query_scalar(
        r#"
        INSERT INTO dns_zones (name, kind, soa_mname, soa_rname, project_id)
        VALUES ($1, $2, 'ns1.platform.test', 'hostmaster.platform.test', $3)
        RETURNING id
        "#,
    )
    .bind(name)
    .bind(kind)
    .bind(project_id)
    .fetch_one(&db.pool)
    .await
    .expect("insert zone")
}

async fn insert_record(db: &TestDb, zone: Uuid, name: &str, rtype: &str, content: &str, proxied: bool) {
    let priority: Option<i32> = (rtype == "MX").then_some(10);
    sqlx::query(
        r#"
        INSERT INTO dns_records (zone_id, name, rtype, content, priority, cdn_proxied)
        VALUES ($1, $2, $3, $4, $5, $6)
        "#,
    )
    .bind(zone)
    .bind(name)
    .bind(rtype)
    .bind(content)
    .bind(priority)
    .bind(proxied)
    .execute(&db.pool)
    .await
    .expect("insert record");
}

/// `(name, rtype, content, cdn_proxied)` sorted, for whole-zone assertions.
async fn records(db: &TestDb, zone: Uuid) -> Vec<(String, String, String, bool)> {
    sqlx::query_as(
        r#"
        SELECT name, rtype, content, cdn_proxied FROM dns_records
        WHERE zone_id = $1 ORDER BY name, rtype, content
        "#,
    )
    .bind(zone)
    .fetch_all(&db.pool)
    .await
    .expect("records")
}

async fn serial(db: &TestDb, zone: Uuid) -> i64 {
    sqlx::query_scalar("SELECT serial FROM dns_zones WHERE id = $1")
        .bind(zone)
        .fetch_one(&db.pool)
        .await
        .expect("serial")
}

async fn attempts(db: &TestDb, id: Uuid) -> i32 {
    sqlx::query_scalar("SELECT cert_attempts FROM domains WHERE id = $1")
        .bind(id)
        .fetch_one(&db.pool)
        .await
        .expect("attempts")
}

async fn is_due(db: &TestDb, id: Uuid) -> bool {
    repository::list_domains_due_for_auto_https(&db.pool)
        .await
        .expect("due list")
        .iter()
        .any(|d| d.id == id)
}

fn assert_about(actual: chrono::DateTime<Utc>, expected_from_now: Duration) {
    let delta = actual - Utc::now();
    let slack = Duration::seconds(60);
    assert!(
        delta > expected_from_now - slack && delta < expected_from_now + slack,
        "next attempt in {delta}, expected about {expected_from_now}"
    );
}

/// Point the platform at a schedule zone with two NS/glue pairs.
async fn platform_with_ns(db: &TestDb) -> Uuid {
    let schedule = insert_zone(db, "platform.test", "schedule", None).await;
    for (i, (name, ip)) in [("ns1.platform.test", "192.0.2.1"), ("ns2.platform.test", "192.0.2.2")]
        .iter()
        .enumerate()
    {
        sqlx::query("INSERT INTO dns_zone_ns (zone_id, name, ip, sort_order) VALUES ($1, $2, $3, $4)")
            .bind(schedule)
            .bind(name)
            .bind(ip)
            .bind(i as i32)
            .execute(&db.pool)
            .await
            .expect("insert ns");
    }
    sqlx::query("UPDATE dns_settings SET schedule_zone_id = $1 WHERE id = 1")
        .bind(schedule)
        .execute(&db.pool)
        .await
        .expect("set schedule zone");
    schedule
}

#[tokio::test]
async fn auto_https_backoff_grows_caps_and_resets() {
    let db = TestDb::new().await;
    db_or_skip!(db);

    let id = insert_domain(&db, "site.test").await;
    sqlx::query("UPDATE domains SET auto_https = TRUE, cert_status = 'pending' WHERE id = $1")
        .bind(id)
        .execute(&db.pool)
        .await
        .expect("enable auto https");
    assert!(is_due(&db, id).await, "a pending site is picked up");

    let first = repository::auto_https_mark_failed(&db.pool, id, "boom")
        .await
        .expect("mark failed")
        .expect("next attempt");
    assert_about(first, Duration::minutes(15));
    assert!(!is_due(&db, id).await, "a failed site waits for its backoff");

    let second = repository::auto_https_mark_failed(&db.pool, id, "boom")
        .await
        .expect("mark failed")
        .expect("next attempt");
    assert_about(second, Duration::minutes(30));
    assert_eq!(attempts(&db, id).await, 2);

    sqlx::query("UPDATE domains SET cert_attempts = 20 WHERE id = $1")
        .bind(id)
        .execute(&db.pool)
        .await
        .expect("many attempts");
    let capped = repository::auto_https_mark_failed(&db.pool, id, "boom")
        .await
        .expect("mark failed")
        .expect("next attempt");
    assert_about(capped, Duration::hours(24));

    // A retry clears the wait; only an explicit reset forgets the attempts.
    repository::auto_https_queue(&db.pool, id, false)
        .await
        .expect("queue");
    assert!(is_due(&db, id).await);
    assert_eq!(attempts(&db, id).await, 21);
    repository::auto_https_queue(&db.pool, id, true)
        .await
        .expect("queue reset");
    assert_eq!(attempts(&db, id).await, 0);

    // Waiting for delegation re-checks soon without counting as a failure.
    repository::auto_https_mark_waiting(&db.pool, id, "waiting for NS", 300)
        .await
        .expect("mark waiting");
    assert!(!is_due(&db, id).await);
    let next = repository::auto_https_next_attempt(&db.pool, id)
        .await
        .expect("next attempt")
        .expect("scheduled");
    assert_about(next, Duration::minutes(5));
    assert_eq!(attempts(&db, id).await, 0);

    repository::auto_https_mark_active(&db.pool, id)
        .await
        .expect("mark active");
    assert!(!is_due(&db, id).await, "an active certificate is not reissued here");
    assert!(repository::auto_https_next_attempt(&db.pool, id)
        .await
        .expect("next attempt")
        .is_none());
    db.close().await;
}

#[tokio::test]
async fn auto_https_skips_disabled_and_inactive_sites() {
    let db = TestDb::new().await;
    db_or_skip!(db);

    let off = insert_domain(&db, "off.test").await;
    let paused = insert_domain(&db, "paused.test").await;
    sqlx::query("UPDATE domains SET auto_https = FALSE, cert_status = 'pending' WHERE id = $1")
        .bind(off)
        .execute(&db.pool)
        .await
        .expect("disable");
    sqlx::query(
        "UPDATE domains SET auto_https = TRUE, cert_status = 'pending', status = 'paused' WHERE id = $1",
    )
    .bind(paused)
    .execute(&db.pool)
    .await
    .expect("pause");
    assert!(!is_due(&db, off).await);
    assert!(!is_due(&db, paused).await);
    db.close().await;
}

#[tokio::test]
async fn acme_txt_values_replace_and_clean_up() {
    let db = TestDb::new().await;
    db_or_skip!(db);

    let zone = insert_zone(&db, "example.test", "hosted", None).await;
    insert_record(&db, zone, "@", "TXT", "v=spf1 -all", false).await;
    let before = serial(&db, zone).await;

    // Apex and wildcard share `_acme-challenge` and need two values at once.
    repository::publish_acme_txt(
        &db.pool,
        zone,
        &[
            ("_acme-challenge".into(), "apex-token".into()),
            ("_acme-challenge".into(), "wildcard-token".into()),
            ("_acme-challenge.www".into(), "www-token".into()),
        ],
    )
    .await
    .expect("publish");
    assert!(serial(&db, zone).await > before, "publishing bumps the serial");
    let txt = |rows: Vec<(String, String, String, bool)>| -> Vec<(String, String)> {
        rows.into_iter()
            .filter(|r| r.1 == "TXT")
            .map(|r| (r.0, r.2))
            .collect()
    };
    assert_eq!(
        txt(records(&db, zone).await),
        vec![
            ("@".to_string(), "v=spf1 -all".to_string()),
            ("_acme-challenge".to_string(), "apex-token".to_string()),
            ("_acme-challenge".to_string(), "wildcard-token".to_string()),
            ("_acme-challenge.www".to_string(), "www-token".to_string()),
        ]
    );

    // A new order replaces stale values at the owners it publishes.
    repository::publish_acme_txt(&db.pool, zone, &[("_acme-challenge".into(), "fresh".into())])
        .await
        .expect("republish");
    assert_eq!(
        txt(records(&db, zone).await),
        vec![
            ("@".to_string(), "v=spf1 -all".to_string()),
            ("_acme-challenge".to_string(), "fresh".to_string()),
            ("_acme-challenge.www".to_string(), "www-token".to_string()),
        ]
    );

    repository::remove_acme_txt(
        &db.pool,
        zone,
        &["_acme-challenge".into(), "_acme-challenge.www".into()],
    )
    .await
    .expect("remove");
    assert_eq!(
        txt(records(&db, zone).await),
        vec![("@".to_string(), "v=spf1 -all".to_string())],
        "only the challenge records are removed"
    );
    db.close().await;
}

#[tokio::test]
async fn ns_zone_is_scoped_to_its_project() {
    let db = TestDb::new().await;
    db_or_skip!(db);

    platform_with_ns(&db).await;
    let owner = insert_project(&db, "owner@example.test").await;
    let other = insert_project(&db, "other@example.test").await;

    let hosts = vec![
        "example.test".to_string(),
        "www.example.test".to_string(),
        "elsewhere.test".to_string(),
    ];
    let zone = repository::ensure_ns_hosted_zone(&db.pool, "Example.Test.", &hosts, Some(owner))
        .await
        .expect("create zone");
    let meta = repository::get_dns_zone_meta(&db.pool, zone)
        .await
        .expect("meta")
        .expect("zone exists");
    assert_eq!(meta.name, "example.test");
    assert_eq!(meta.kind, "hosted");
    assert_eq!(meta.project_id, Some(owner));
    assert_eq!(
        repository::dns_zone_ns_ips(&db.pool, zone).await.expect("ns ips"),
        vec!["192.0.2.1".to_string(), "192.0.2.2".to_string()]
    );
    assert_eq!(
        records(&db, zone).await,
        vec![
            ("@".to_string(), "A".to_string(), "0.0.0.0".to_string(), true),
            ("www".to_string(), "A".to_string(), "0.0.0.0".to_string(), true),
        ],
        "names outside the zone are not written into it"
    );

    // The owning project reuses its zone.
    let again = repository::ensure_ns_hosted_zone(&db.pool, "example.test", &hosts, Some(owner))
        .await
        .expect("reuse zone");
    assert_eq!(again, zone);

    // Another tenant cannot add names to it, and nobody can host sites in the
    // platform schedule zone.
    let api = vec!["api.example.test".to_string()];
    assert!(repository::ensure_ns_hosted_zone(&db.pool, "example.test", &api, Some(other))
        .await
        .is_err());
    assert!(repository::ensure_ns_hosted_zone(&db.pool, "example.test", &api, None)
        .await
        .is_err());
    assert!(repository::ensure_ns_hosted_zone(
        &db.pool,
        "platform.test",
        &["cdn.platform.test".to_string()],
        Some(owner)
    )
    .await
    .is_err());
    assert!(
        !records(&db, zone).await.iter().any(|r| r.0 == "api"),
        "a rejected request must not write records"
    );
    db.close().await;
}

#[tokio::test]
async fn proxied_records_respect_user_records() {
    let db = TestDb::new().await;
    db_or_skip!(db);

    platform_with_ns(&db).await;
    let zone = insert_zone(&db, "example.test", "hosted", None).await;
    // The user's own address keeps priority; MX/TXT never block the site.
    insert_record(&db, zone, "shop", "A", "203.0.113.9", false).await;
    insert_record(&db, zone, "mail", "MX", "mx.example.test", false).await;
    insert_record(&db, zone, "mail", "TXT", "v=spf1 -all", false).await;
    // A proxied CNAME from an earlier build becomes a proxied A.
    insert_record(&db, zone, "legacy", "CNAME", "edge.platform.test", true).await;

    let hosts: Vec<String> = ["shop", "mail", "legacy"]
        .iter()
        .map(|h| format!("{h}.example.test"))
        .collect();
    repository::ensure_ns_hosted_zone(&db.pool, "example.test", &hosts, None)
        .await
        .expect("ensure");
    assert_eq!(
        records(&db, zone).await,
        vec![
            ("legacy".to_string(), "A".to_string(), "0.0.0.0".to_string(), true),
            ("mail".to_string(), "A".to_string(), "0.0.0.0".to_string(), true),
            ("mail".to_string(), "MX".to_string(), "mx.example.test".to_string(), false),
            ("mail".to_string(), "TXT".to_string(), "v=spf1 -all".to_string(), false),
            ("shop".to_string(), "A".to_string(), "203.0.113.9".to_string(), false),
        ]
    );

    // Removing hostnames drops only the platform-managed records.
    let removed = repository::remove_proxied_hostnames(&db.pool, zone, "example.test", &hosts)
        .await
        .expect("remove");
    assert_eq!(removed, 2);
    assert_eq!(
        records(&db, zone).await,
        vec![
            ("mail".to_string(), "MX".to_string(), "mx.example.test".to_string(), false),
            ("mail".to_string(), "TXT".to_string(), "v=spf1 -all".to_string(), false),
            ("shop".to_string(), "A".to_string(), "203.0.113.9".to_string(), false),
        ]
    );
    db.close().await;
}

#[tokio::test]
async fn detaching_a_site_leaves_its_neighbours() {
    let db = TestDb::new().await;
    db_or_skip!(db);

    let zone = insert_zone(&db, "example.test", "hosted", None).await;
    let site = insert_domain(&db, "example.test").await;
    let neighbour = insert_domain(&db, "blog.example.test").await;
    for id in [site, neighbour] {
        sqlx::query("UPDATE domains SET access_mode = 'ns', dns_zone_id = $2 WHERE id = $1")
            .bind(id)
            .bind(zone)
            .execute(&db.pool)
            .await
            .expect("ns mode");
    }
    // Only the extra hostname has a row: the primary must still be found.
    sqlx::query("INSERT INTO domain_hostnames (domain_id, hostname) VALUES ($1, 'www.example.test')")
        .bind(site)
        .execute(&db.pool)
        .await
        .expect("hostname");
    for name in ["@", "www", "blog"] {
        insert_record(&db, zone, name, "A", "0.0.0.0", true).await;
    }

    repository::detach_site_dns(&db.pool, site)
        .await
        .expect("detach");
    assert_eq!(
        records(&db, zone).await,
        vec![("blog".to_string(), "A".to_string(), "0.0.0.0".to_string(), true)]
    );
    db.close().await;
}

#[tokio::test]
async fn hostname_visibility_includes_extra_hostnames() {
    let db = TestDb::new().await;
    db_or_skip!(db);

    let a = insert_domain(&db, "a.test").await;
    let _b = insert_domain(&db, "b.test").await;
    sqlx::query("INSERT INTO domain_hostnames (domain_id, hostname) VALUES ($1, ' WWW.A.test. ')")
        .bind(a)
        .execute(&db.pool)
        .await
        .expect("hostname");

    let mut mine = repository::hostnames_by_ids(&db.pool, &[a])
        .await
        .expect("by ids");
    mine.sort();
    assert_eq!(mine, vec!["a.test".to_string(), "www.a.test".to_string()]);
    assert!(repository::hostnames_by_ids(&db.pool, &[])
        .await
        .expect("empty")
        .is_empty());

    let all = repository::all_hostnames(&db.pool).await.expect("all");
    for name in ["a.test", "www.a.test", "b.test"] {
        assert!(all.iter().any(|h| h == name), "{name} missing from {all:?}");
    }
    db.close().await;
}

#[tokio::test]
async fn view_version_moves_when_a_lower_serial_zone_changes() {
    let db = TestDb::new().await;
    db_or_skip!(db);

    let schedule = platform_with_ns(&db).await;
    let hosted = insert_zone(&db, "hosted.test", "hosted", None).await;
    let set_serial = |zone: Uuid, serial: i64| {
        let pool = db.pool.clone();
        async move {
            sqlx::query("UPDATE dns_zones SET serial = $1 WHERE id = $2")
                .bind(serial)
                .bind(zone)
                .execute(&pool)
                .await
                .expect("set serial");
        }
    };
    // A hosted zone far ahead of the schedule zone, as after many record edits.
    set_serial(hosted, 50).await;
    set_serial(schedule, 6).await;
    let version = |pool: sqlx::PgPool| async move {
        control::api::dns_admin::view_version(&pool)
            .await
            .expect("view version")
    };
    assert_eq!(version(db.pool.clone()).await, 56);

    // Saving the CDN 主域 bumps only the schedule zone; the highest serial
    // stays 50, so a max-based version would not move and every node would
    // look up to date before pulling the change.
    set_serial(schedule, 7).await;
    assert_eq!(version(db.pool.clone()).await, 57);

    // Disabled zones are not served, so they do not count.
    sqlx::query("UPDATE dns_zones SET enabled = FALSE WHERE id = $1")
        .bind(hosted)
        .execute(&db.pool)
        .await
        .expect("disable zone");
    assert_eq!(version(db.pool.clone()).await, 7);

    db.close().await;
}

#[tokio::test]
async fn ns_names_on_one_ip_are_one_node_named_after_the_first() {
    let db = TestDb::new().await;
    db_or_skip!(db);

    let pairs = vec![
        ("ns1.platform.test".to_string(), "192.0.2.53".to_string()),
        ("ns2.platform.test".to_string(), "192.0.2.53".to_string()),
        ("ns3.platform.test".to_string(), "192.0.2.54".to_string()),
    ];
    // Saving twice must not flip the shared node's name to the last entry.
    for _ in 0..2 {
        control::service::dns_probe::upsert_from_ns(&db.pool, &pairs)
            .await
            .expect("upsert nodes from ns");
    }
    let rows: Vec<(String, String)> =
        sqlx::query_as("SELECT name, public_ip FROM dns_nodes ORDER BY public_ip")
            .fetch_all(&db.pool)
            .await
            .expect("list nodes");
    assert_eq!(
        rows,
        vec![
            ("ns1.platform.test".to_string(), "192.0.2.53".to_string()),
            ("ns3.platform.test".to_string(), "192.0.2.54".to_string()),
        ]
    );

    db.close().await;
}

async fn zone_ns(db: &TestDb, zone: Uuid) -> Vec<String> {
    sqlx::query_scalar("SELECT name FROM dns_zone_ns WHERE zone_id = $1 ORDER BY sort_order, name")
        .bind(zone)
        .fetch_all(&db.pool)
        .await
        .expect("zone ns")
}

async fn set_zone_ns(db: &TestDb, zone: Uuid, names: &[&str]) {
    sqlx::query("DELETE FROM dns_zone_ns WHERE zone_id = $1")
        .bind(zone)
        .execute(&db.pool)
        .await
        .expect("clear ns");
    for (i, name) in names.iter().enumerate() {
        sqlx::query("INSERT INTO dns_zone_ns (zone_id, name, ip, sort_order) VALUES ($1, $2, '192.0.2.9', $3)")
            .bind(zone)
            .bind(name)
            .bind(i as i32)
            .execute(&db.pool)
            .await
            .expect("insert ns");
    }
}

#[tokio::test]
async fn hosted_zones_without_ns_get_the_platform_set() {
    let db = TestDb::new().await;
    db_or_skip!(db);

    // Added on the 域名与记录 page before the fix: no NS, SOA naming itself.
    let orphan: Uuid = sqlx::query_scalar(
        "INSERT INTO dns_zones (name, kind, soa_mname, soa_rname) \
         VALUES ('orphan.test', 'hosted', 'ns1.orphan.test', 'hostmaster.orphan.test') RETURNING id",
    )
    .fetch_one(&db.pool)
    .await
    .expect("insert orphan zone");

    // Nothing to copy before the CDN 主域 has NS servers.
    assert_eq!(repository::repair_hosted_zones_without_ns(&db.pool).await.unwrap(), 0);
    assert!(zone_ns(&db, orphan).await.is_empty());

    platform_with_ns(&db).await;
    assert_eq!(repository::repair_hosted_zones_without_ns(&db.pool).await.unwrap(), 1);
    assert_eq!(zone_ns(&db, orphan).await, vec!["ns1.platform.test", "ns2.platform.test"]);
    let (mname, serial): (String, i64) =
        sqlx::query_as("SELECT soa_mname, serial FROM dns_zones WHERE id = $1")
            .bind(orphan)
            .fetch_one(&db.pool)
            .await
            .expect("zone row");
    assert_eq!(mname, "ns1.platform.test");
    assert!(serial > 1, "nodes must see the change");

    // A second run finds nothing left to repair.
    assert_eq!(repository::repair_hosted_zones_without_ns(&db.pool).await.unwrap(), 0);

    db.close().await;
}

#[tokio::test]
async fn platform_ns_changes_follow_zones_that_use_them_only() {
    let db = TestDb::new().await;
    db_or_skip!(db);

    let schedule = platform_with_ns(&db).await;
    let follower = insert_zone(&db, "follower.test", "hosted", None).await;
    set_zone_ns(&db, follower, &["NS2.platform.test.", "ns1.platform.test"]).await;
    let vanity = insert_zone(&db, "vanity.test", "hosted", None).await;
    set_zone_ns(&db, vanity, &["ns1.vanity.test"]).await;
    let orphan = insert_zone(&db, "orphan.test", "hosted", None).await;

    // The CDN 主域 moves to new NS names.
    let previous = vec!["ns1.platform.test".to_string(), "ns2.platform.test".to_string()];
    set_zone_ns(&db, schedule, &["dns1.newplat.test", "dns2.newplat.test"]).await;
    let updated = repository::propagate_schedule_ns_to_ns_zones(&db.pool, &previous)
        .await
        .expect("propagate");

    assert_eq!(updated, 2, "the follower and the NS-less zone");
    assert_eq!(zone_ns(&db, follower).await, vec!["dns1.newplat.test", "dns2.newplat.test"]);
    assert_eq!(zone_ns(&db, orphan).await, vec!["dns1.newplat.test", "dns2.newplat.test"]);
    assert_eq!(zone_ns(&db, vanity).await, vec!["ns1.vanity.test"], "own NS names are kept");

    db.close().await;
}
