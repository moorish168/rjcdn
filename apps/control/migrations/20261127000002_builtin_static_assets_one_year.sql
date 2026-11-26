-- Built-in 「默认缓存条件」: static assets are kept at the edge for 1 year
-- instead of 7 days. Only the rule as seeded is updated; a TTL or note an
-- administrator changed is left alone.
--
-- Browser TTL is not touched: a purge clears the edge cache but cannot reach
-- objects browsers already hold.
UPDATE cache_policy_rules
SET ttl_seconds = 31536000,
    note = '静态资源 1 年'
WHERE policy_id = 'a0000000-0000-4000-8000-000000000001'::uuid
  AND match_type = 'regex'
  AND ttl_seconds = 604800
  AND note = '静态资源 7 天';
