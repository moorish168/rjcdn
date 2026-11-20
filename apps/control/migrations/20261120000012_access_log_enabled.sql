-- Per-site access log switch. Existing sites stay on (the edge used to log every site).

ALTER TABLE domains
    ADD COLUMN IF NOT EXISTS access_log_enabled BOOLEAN NOT NULL DEFAULT TRUE;
