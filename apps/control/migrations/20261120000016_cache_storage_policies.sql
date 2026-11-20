-- Node-oriented cache storage policies (GoEdge-style HTTP cache capacity).
-- Distinct from cache_policies / cache_policy_rules, which are reusable
-- path/TTL rule templates for sites.

CREATE TABLE IF NOT EXISTS cache_storage_policies (
    id                   UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name                 TEXT NOT NULL,
    note                 TEXT NOT NULL DEFAULT '',
    enabled              BOOLEAN NOT NULL DEFAULT TRUE,
    -- Disk large-object cache (maps to LARGE_CACHE_* on the edge).
    large_cache_enabled  BOOLEAN NOT NULL DEFAULT TRUE,
    disk_dir             TEXT NOT NULL DEFAULT '/var/lib/lagran/large-cache',
    -- 0 = leave LARGE_CACHE_MAX_BYTES to the node hardware profile.
    disk_max_bytes       BIGINT NOT NULL DEFAULT 0
                         CHECK (disk_max_bytes >= 0),
    -- Optional memory-side hint (compressed / in-memory cache budget).
    -- 0 = leave to edge defaults / profile.
    memory_max_bytes     BIGINT NOT NULL DEFAULT 0
                         CHECK (memory_max_bytes >= 0),
    created_at           TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at           TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX IF NOT EXISTS cache_storage_policies_name_uidx
    ON cache_storage_policies (lower(name));

ALTER TABLE edge_nodes
    ADD COLUMN IF NOT EXISTS cache_storage_policy_id
        UUID REFERENCES cache_storage_policies(id) ON DELETE SET NULL;

-- Seed one default policy operators can bind to nodes.
INSERT INTO cache_storage_policies (name, note, large_cache_enabled, disk_dir, disk_max_bytes, memory_max_bytes)
SELECT
    '默认磁盘缓存',
    '节点磁盘大对象缓存；容量为 0 时沿用硬件画像推算',
    TRUE,
    '/var/lib/lagran/large-cache',
    0,
    0
WHERE NOT EXISTS (
    SELECT 1 FROM cache_storage_policies WHERE lower(name) = lower('默认磁盘缓存')
);
