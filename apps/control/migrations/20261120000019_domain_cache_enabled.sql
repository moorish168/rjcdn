-- Site-level cache master switch. Default on; turning off purges that site's cache.
ALTER TABLE domains
    ADD COLUMN IF NOT EXISTS cache_enabled BOOLEAN NOT NULL DEFAULT TRUE;
