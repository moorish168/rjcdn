-- Continent + multi-country match for DNS lines / WAF regions.
ALTER TABLE geo_regions
    ADD COLUMN IF NOT EXISTS continent TEXT NOT NULL DEFAULT '',
    ADD COLUMN IF NOT EXISTS countries TEXT NOT NULL DEFAULT '';

-- Allow kind=continent (ISO/MaxMind continent code lives in `continent`).
ALTER TABLE geo_regions DROP CONSTRAINT IF EXISTS geo_regions_kind_check;
ALTER TABLE geo_regions
    ADD CONSTRAINT geo_regions_kind_check
    CHECK (kind IN ('default', 'country', 'continent', 'province', 'isp', 'custom'));

-- Node-managed CDN 主域 rows must be static (non-proxied) A/AAAA per line.
UPDATE dns_records
SET cdn_proxied = FALSE, updated_at = NOW()
WHERE managed_by_node_id IS NOT NULL
  AND cdn_proxied = TRUE;
