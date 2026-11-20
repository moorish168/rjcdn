-- Make automatic HTTPS safe to roll out on an existing installation.
--
-- 20261120000004 added `auto_https BOOLEAN DEFAULT TRUE`, which turned the
-- feature on for every site that already existed. The background job would then
-- order a new Let's Encrypt certificate for each of them, supersede certificates
-- that operators had uploaded, and retry failures every minute until the ACME
-- account hit its rate limits.
--
-- Sites that existed before the feature keep their current certificates: they
-- are opted out and can be switched on per site in the console. Rows created by
-- the new code are already `pending` (or further along) and are not touched.
UPDATE domains
SET auto_https = FALSE
WHERE cert_status = 'none';

-- Report the certificate an existing site already has.
UPDATE domains d
SET cert_status = 'active'
WHERE d.cert_status = 'none'
  AND EXISTS (
      SELECT 1
      FROM certificates c
      WHERE (c.domain_id = d.id OR lower(c.domain_name) = lower(d.name))
        AND c.status = 'active'
        AND (c.not_after IS NULL OR c.not_after > NOW())
  );

-- Failure backoff for automatic issuance. Let's Encrypt allows only a handful
-- of failed validations per hostname per hour; retrying every minute gets the
-- whole ACME account throttled.
ALTER TABLE domains
    ADD COLUMN IF NOT EXISTS cert_attempts INTEGER NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS cert_next_attempt_at TIMESTAMPTZ;

DROP INDEX IF EXISTS idx_domains_auto_https_pending;
CREATE INDEX IF NOT EXISTS idx_domains_auto_https_due
    ON domains (cert_next_attempt_at)
    WHERE auto_https = TRUE AND cert_status IN ('pending', 'error', 'none');

-- Region rules are stored in waf_geo_rules (kind region_allow / region_deny);
-- this table was created but never read or written.
DROP TABLE IF EXISTS waf_region_rules;
