-- Release download token for private upgrade repositories (console-managed).
ALTER TABLE console_settings
    ADD COLUMN IF NOT EXISTS update_token TEXT NOT NULL DEFAULT '';
