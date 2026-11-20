-- White-label console branding (system display name).
CREATE TABLE IF NOT EXISTS console_settings (
    id          INTEGER PRIMARY KEY CHECK (id = 1),
    system_name TEXT NOT NULL DEFAULT '控制台',
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

INSERT INTO console_settings (id, system_name)
VALUES (1, '控制台')
ON CONFLICT (id) DO NOTHING;
