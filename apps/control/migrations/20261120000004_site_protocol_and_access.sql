-- Per-site protocol / optimization toggles, DNS access mode, and auto-HTTPS state.

ALTER TABLE domains
    ADD COLUMN IF NOT EXISTS websocket_enabled BOOLEAN NOT NULL DEFAULT TRUE,
    ADD COLUMN IF NOT EXISTS http2_enabled BOOLEAN NOT NULL DEFAULT TRUE,
    ADD COLUMN IF NOT EXISTS http3_enabled BOOLEAN NOT NULL DEFAULT FALSE,
    ADD COLUMN IF NOT EXISTS gzip_enabled BOOLEAN NOT NULL DEFAULT TRUE,
    ADD COLUMN IF NOT EXISTS image_webp_enabled BOOLEAN NOT NULL DEFAULT FALSE,
    ADD COLUMN IF NOT EXISTS access_mode TEXT NOT NULL DEFAULT 'cname',
    ADD COLUMN IF NOT EXISTS dns_zone_id UUID REFERENCES dns_zones(id) ON DELETE SET NULL,
    ADD COLUMN IF NOT EXISTS auto_https BOOLEAN NOT NULL DEFAULT TRUE,
    ADD COLUMN IF NOT EXISTS cert_status TEXT NOT NULL DEFAULT 'none',
    ADD COLUMN IF NOT EXISTS cert_last_error TEXT NOT NULL DEFAULT '';

ALTER TABLE domains
    DROP CONSTRAINT IF EXISTS domains_access_mode_check;
ALTER TABLE domains
    ADD CONSTRAINT domains_access_mode_check
    CHECK (access_mode IN ('cname', 'ns'));

ALTER TABLE domains
    DROP CONSTRAINT IF EXISTS domains_cert_status_check;
ALTER TABLE domains
    ADD CONSTRAINT domains_cert_status_check
    CHECK (cert_status IN ('none', 'pending', 'active', 'error'));

CREATE INDEX IF NOT EXISTS idx_domains_auto_https_pending
    ON domains (access_mode, auto_https, cert_status)
    WHERE auto_https = TRUE AND cert_status IN ('pending', 'error', 'none');
