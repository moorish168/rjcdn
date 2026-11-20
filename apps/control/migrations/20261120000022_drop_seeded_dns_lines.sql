-- Drop pre-seeded 「标准线路」. Catch-all is only the default line:
-- any client that does not match a custom line (including GeoIP miss) uses it.
-- dns_records.line_id / edge_nodes.region_id are ON DELETE SET NULL;
-- waf_region_rules cascade away with the region.

UPDATE geo_regions
SET note = '全部：未命中任何自定义线路时使用（含 IP 库未匹配）',
    updated_at = NOW()
WHERE code = 'default';

DELETE FROM geo_regions
WHERE lower(code) IN (
    'cn',
    'cn-telecom',
    'cn-unicom',
    'cn-mobile',
    'cn-gd',
    'cn-bj',
    'cn-sh',
    'cn-zj',
    'cn-js',
    'overseas'
);
