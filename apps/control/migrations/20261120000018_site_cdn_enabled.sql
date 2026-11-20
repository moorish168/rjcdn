-- Site-level CDN acceleration toggle (Cloudflare-style orange/grey cloud).
-- When off, NS-hosted DNS records stay but answer with the origin (cdn_proxied=false).

ALTER TABLE domains
    ADD COLUMN IF NOT EXISTS cdn_enabled BOOLEAN NOT NULL DEFAULT TRUE;

ALTER TABLE dns_records
    ADD COLUMN IF NOT EXISTS managed_by_site_id UUID REFERENCES domains(id) ON DELETE SET NULL;

CREATE INDEX IF NOT EXISTS dns_records_managed_site_idx
    ON dns_records (managed_by_site_id)
    WHERE managed_by_site_id IS NOT NULL;

-- Claim existing proxied site records so grey-cloud / delete still finds them.
UPDATE dns_records r
SET managed_by_site_id = d.id
FROM domains d
JOIN dns_zones z ON z.id = d.dns_zone_id
WHERE r.zone_id = z.id
  AND r.cdn_proxied = TRUE
  AND r.managed_by_site_id IS NULL
  AND upper(r.rtype) IN ('A', 'AAAA', 'CNAME')
  AND (
    lower(rtrim(btrim(d.name), '.')) = lower(
      CASE
        WHEN r.name IN ('@', '') THEN z.name
        ELSE r.name || '.' || z.name
      END
    )
    OR EXISTS (
      SELECT 1 FROM domain_hostnames h
      WHERE h.domain_id = d.id
        AND lower(rtrim(btrim(h.hostname), '.')) = lower(
          CASE
            WHEN r.name IN ('@', '') THEN z.name
            ELSE r.name || '.' || z.name
          END
        )
    )
  );
