-- Built-in default cache conditions (site-side path/TTL pack).
-- Distinct from cache_storage_policies (node disk 「缓存策略」).

ALTER TABLE cache_policies
    ADD COLUMN IF NOT EXISTS is_builtin BOOLEAN NOT NULL DEFAULT FALSE;

-- Avoid unique-name clash with a prior custom pack named the same.
UPDATE cache_policies
SET name = name || ' (自定义)'
WHERE lower(name) = lower('默认缓存条件')
  AND id <> 'a0000000-0000-4000-8000-000000000001'::uuid
  AND project_id IS NULL;

-- Fixed id so new sites / backfill can reference it reliably.
INSERT INTO cache_policies (id, name, project_id, note, enabled, is_builtin)
VALUES (
    'a0000000-0000-4000-8000-000000000001'::uuid,
    '默认缓存条件',
    NULL,
    '系统内置：静态资源缓存、常见动态路径绕过。站点可选用；本站缓存规则优先级更高。',
    TRUE,
    TRUE
)
ON CONFLICT (id) DO UPDATE
SET
    name = EXCLUDED.name,
    note = EXCLUDED.note,
    enabled = TRUE,
    is_builtin = TRUE,
    updated_at = NOW();

-- Ensure only one built-in row (in case of prior manual inserts).
UPDATE cache_policies
SET is_builtin = FALSE
WHERE id <> 'a0000000-0000-4000-8000-000000000001'::uuid
  AND is_builtin = TRUE;

-- Seed rules only when the built-in pack has none yet (idempotent re-run safe).
INSERT INTO cache_policy_rules (
    policy_id, path_pattern, match_type, ttl_seconds, priority, action,
    swr_seconds, enabled, match_methods, ignore_cookie, note
)
SELECT
    'a0000000-0000-4000-8000-000000000001'::uuid,
    v.path_pattern, v.match_type, v.ttl_seconds, v.priority, v.action,
    v.swr_seconds, TRUE, v.match_methods, v.ignore_cookie, v.note
FROM (
    VALUES
        (
            '\.(css|js|mjs|map|jpg|jpeg|png|gif|webp|avif|svg|ico|woff2?|ttf|eot|otf|mp4|webm|mp3|ogg|pdf|zip)$',
            'regex', 604800, 50, 'cache', 86400, 'GET,HEAD', TRUE,
            '静态资源 7 天'
        ),
        ('/api/*', 'glob', 0, 100, 'bypass', 0, '', FALSE, 'API 绕过'),
        ('/admin/*', 'glob', 0, 100, 'bypass', 0, '', FALSE, '后台绕过')
) AS v(path_pattern, match_type, ttl_seconds, priority, action, swr_seconds, match_methods, ignore_cookie, note)
WHERE NOT EXISTS (
    SELECT 1 FROM cache_policy_rules
    WHERE policy_id = 'a0000000-0000-4000-8000-000000000001'::uuid
);

-- Existing sites without a pack: attach the built-in default.
UPDATE domains
SET cache_policy_id = 'a0000000-0000-4000-8000-000000000001'::uuid,
    updated_at = NOW()
WHERE cache_policy_id IS NULL;
