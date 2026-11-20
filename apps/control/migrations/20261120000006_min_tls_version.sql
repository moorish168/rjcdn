-- Per-site minimum TLS version for client-facing HTTPS (default TLS 1.2).
ALTER TABLE domains
    ADD COLUMN IF NOT EXISTS min_tls_version TEXT NOT NULL DEFAULT '1.2';

UPDATE domains
SET min_tls_version = '1.2'
WHERE min_tls_version IS NULL OR trim(min_tls_version) = '';

ALTER TABLE domains
    DROP CONSTRAINT IF EXISTS domains_min_tls_version_check;

ALTER TABLE domains
    ADD CONSTRAINT domains_min_tls_version_check
    CHECK (min_tls_version IN ('1.0', '1.1', '1.2', '1.3'));
