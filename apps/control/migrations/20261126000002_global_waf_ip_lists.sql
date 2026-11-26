ALTER TABLE ip_lists
    ADD COLUMN IF NOT EXISTS applies_globally BOOLEAN NOT NULL DEFAULT FALSE;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'ip_lists_global_black_only_check'
    ) THEN
        ALTER TABLE ip_lists
            ADD CONSTRAINT ip_lists_global_black_only_check
            CHECK (NOT applies_globally OR (project_id IS NULL AND list_type = 'black'));
    END IF;
END $$;

CREATE INDEX IF NOT EXISTS ip_lists_global_enabled_idx
    ON ip_lists (enabled) WHERE applies_globally = TRUE;
