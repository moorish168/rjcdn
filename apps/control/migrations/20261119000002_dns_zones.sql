-- Authoritative DNS zones managed by the control plane.
--
-- Phase 1: one schedule zone (GeoDNS apex) + NS/glue pairs. Phase 2 adds
-- arbitrary hosted zones and dns_records. lagran-dns pulls via /api/v1/dns/sync.

CREATE TABLE IF NOT EXISTS dns_zones (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name         TEXT NOT NULL,
    kind         TEXT NOT NULL DEFAULT 'hosted'
                 CHECK (kind IN ('schedule', 'hosted')),
    soa_mname    TEXT NOT NULL,
    soa_rname    TEXT NOT NULL,
    ttl_default  INTEGER NOT NULL DEFAULT 30
                 CHECK (ttl_default BETWEEN 5 AND 300),
    serial       BIGINT NOT NULL DEFAULT 1,
    enabled      BOOLEAN NOT NULL DEFAULT TRUE,
    project_id   UUID REFERENCES projects(id) ON DELETE SET NULL,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT dns_zones_name_unique UNIQUE (name)
);

CREATE INDEX IF NOT EXISTS dns_zones_kind_enabled_idx
    ON dns_zones (kind, enabled);

CREATE TABLE IF NOT EXISTS dns_zone_ns (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    zone_id     UUID NOT NULL REFERENCES dns_zones(id) ON DELETE CASCADE,
    name        TEXT NOT NULL,
    ip          TEXT NOT NULL,
    sort_order  INTEGER NOT NULL DEFAULT 0,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT dns_zone_ns_zone_name_unique UNIQUE (zone_id, name)
);

CREATE INDEX IF NOT EXISTS dns_zone_ns_zone_idx
    ON dns_zone_ns (zone_id, sort_order);

-- Singleton row for platform-wide DNS options.
CREATE TABLE IF NOT EXISTS dns_settings (
    id                 SMALLINT PRIMARY KEY DEFAULT 1 CHECK (id = 1),
    schedule_zone_id   UUID REFERENCES dns_zones(id) ON DELETE SET NULL,
    fallback_ip        TEXT,
    updated_at         TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

INSERT INTO dns_settings (id)
VALUES (1)
ON CONFLICT (id) DO NOTHING;
