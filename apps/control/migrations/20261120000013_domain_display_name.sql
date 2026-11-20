-- User-facing site label. Hostname stays in `name` / domain_hostnames.
ALTER TABLE domains
    ADD COLUMN IF NOT EXISTS display_name TEXT NOT NULL DEFAULT '';
