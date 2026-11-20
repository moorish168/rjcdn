-- User / auto-created lines should be kind=custom so pick_region ranks them
-- above seeded 标准线路 (cn, cn-telecom, provinces, …). Leave the seed rows alone.
UPDATE geo_regions
SET kind = 'custom',
    updated_at = NOW()
WHERE is_default = FALSE
  AND kind <> 'custom'
  AND lower(code) NOT IN (
      'default',
      'cn',
      'cn-telecom',
      'cn-unicom',
      'cn-mobile',
      'cn-gd',
      'cn-bj',
      'cn-sh',
      'cn-zj',
      'cn-js'
  );
