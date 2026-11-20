-- Static DNS records for hosted / schedule zones (Phase 2).
--
-- `cdn_proxied` marks names that should be answered via GeoDNS (/schedule)
-- instead of the static content. Relative names use "@" for the zone apex.

CREATE TABLE IF NOT EXISTS dns_records (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    zone_id      UUID NOT NULL REFERENCES dns_zones(id) ON DELETE CASCADE,
    name         TEXT NOT NULL,
    rtype        TEXT NOT NULL
                 CHECK (rtype IN ('A', 'AAAA', 'CNAME', 'TXT', 'MX', 'NS')),
    content      TEXT NOT NULL,
    ttl          INTEGER NOT NULL DEFAULT 30
                 CHECK (ttl BETWEEN 5 AND 86400),
    priority     INTEGER,
    cdn_proxied  BOOLEAN NOT NULL DEFAULT FALSE,
    enabled      BOOLEAN NOT NULL DEFAULT TRUE,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT dns_records_priority_mx CHECK (
        (rtype = 'MX' AND priority IS NOT NULL)
        OR (rtype <> 'MX' AND priority IS NULL)
    )
);

CREATE INDEX IF NOT EXISTS dns_records_zone_name_idx
    ON dns_records (zone_id, name, rtype);

CREATE UNIQUE INDEX IF NOT EXISTS dns_records_unique_idx
    ON dns_records (
        zone_id,
        name,
        rtype,
        content,
        COALESCE(priority, -1)
    );
