-- Site (= domains row) may serve multiple hostnames that share origin / CDN /
-- WAF settings. The primary hostname stays in domains.name for backwards
-- compatibility; every hostname — including the primary — lives here so the
-- edge config publisher can expand one site into N Host lookups.

CREATE TABLE IF NOT EXISTS domain_hostnames (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    domain_id   UUID NOT NULL REFERENCES domains(id) ON DELETE CASCADE,
    hostname    TEXT NOT NULL,
    is_primary  BOOLEAN NOT NULL DEFAULT FALSE,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Canonical uniqueness: same normalization the edge applies to Host.
CREATE UNIQUE INDEX IF NOT EXISTS idx_domain_hostnames_hostname_canonical
    ON domain_hostnames (lower(rtrim(btrim(hostname), '.')));

CREATE INDEX IF NOT EXISTS idx_domain_hostnames_domain_id
    ON domain_hostnames (domain_id);

-- Backfill from existing domains.name. One primary per site.
INSERT INTO domain_hostnames (domain_id, hostname, is_primary)
SELECT id, lower(rtrim(btrim(name), '.')), TRUE
FROM domains d
WHERE NOT EXISTS (
    SELECT 1 FROM domain_hostnames h WHERE h.domain_id = d.id
);
