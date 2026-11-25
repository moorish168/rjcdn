UPDATE domains
SET cache_policy_id = 'a0000000-0000-4000-8000-000000000001'::uuid,
    updated_at = NOW()
WHERE cache_policy_id IS NULL;

ALTER TABLE domains
    ALTER COLUMN cache_policy_id
    SET DEFAULT 'a0000000-0000-4000-8000-000000000001'::uuid;
