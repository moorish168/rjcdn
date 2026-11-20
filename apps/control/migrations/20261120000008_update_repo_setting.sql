-- Upgrade source is configured in the console; no hardcoded upstream repo.
ALTER TABLE console_settings
    ADD COLUMN IF NOT EXISTS update_repo TEXT NOT NULL DEFAULT '';
