-- Authoritative DNS server registry (Phase 3).
--
-- Operators register lagran-dns instances here. Processes report heartbeat
-- after each /dns/sync so the console can show online/offline and serial lag.

CREATE TABLE IF NOT EXISTS dns_nodes (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name            TEXT NOT NULL,
    public_ip       TEXT NOT NULL,
    listen          TEXT NOT NULL DEFAULT '0.0.0.0:53',
    status          TEXT NOT NULL DEFAULT 'pending'
                    CHECK (status IN ('pending', 'online', 'offline', 'disabled')),
    auth_token      TEXT,
    version         TEXT NOT NULL DEFAULT '',
    git_hash        TEXT NOT NULL DEFAULT '',
    last_serial     BIGINT,
    cache_hits      BIGINT,
    cache_misses    BIGINT,
    zone_count      INTEGER,
    last_sync_at    TIMESTAMPTZ,
    last_seen_at    TIMESTAMPTZ,
    enabled         BOOLEAN NOT NULL DEFAULT TRUE,
    notes           TEXT NOT NULL DEFAULT '',
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT dns_nodes_name_unique UNIQUE (name),
    CONSTRAINT dns_nodes_public_ip_unique UNIQUE (public_ip)
);

CREATE INDEX IF NOT EXISTS dns_nodes_status_idx
    ON dns_nodes (enabled, status, last_seen_at DESC);
