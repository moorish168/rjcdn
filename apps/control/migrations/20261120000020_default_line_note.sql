-- Drop the internal "降级" wording from the default line note.
UPDATE geo_regions
SET note = '未命中其他地区时使用的默认解析',
    updated_at = NOW()
WHERE code = 'default'
  AND (note ILIKE '%降级%' OR note = 'DNS 未命中专线时的降级线路');
