-- WAF 地区名单: one rule now covers several continents and countries, picked
-- from the same continent → country tree as DNS lines. WAF no longer points
-- at DNS lines, and the 网络号 (ASN) lists are gone.
--
-- country_allow / country_deny keep their kind. `value` becomes a comma list:
-- whole continents as "@XX" first, then ISO country codes, each part sorted
-- (e.g. "@EU,CN,US"). A single country ("CN") is the same encoding.

-- 1. region_allow / region_deny rows referenced a DNS line by code. Work out
--    the equivalent 地区 rule for each.
CREATE TEMP TABLE waf_geo_region_conv AS
SELECT w.id,
       w.domain_id,
       w.enabled,
       CASE w.kind WHEN 'region_allow' THEN 'country_allow' ELSE 'country_deny' END AS new_kind,
       CASE
           -- A whole-continent line (every country of it ticked) means the continent.
           WHEN g.kind = 'continent' AND g.continent <> '' THEN '@' || upper(trim(g.continent))
           WHEN trim(g.countries) <> '' THEN (
               SELECT string_agg(c, ',' ORDER BY c)
               FROM (
                   SELECT DISTINCT upper(trim(x)) AS c
                   FROM unnest(string_to_array(g.countries, ',')) AS x
                   WHERE trim(x) <> ''
               ) s
           )
           WHEN trim(g.country) <> '' THEN upper(trim(g.country))
           WHEN trim(g.continent) <> '' THEN '@' || upper(trim(g.continent))
           ELSE ''
       END AS new_value,
       -- The edge never had province / city / ISP data, so these never matched.
       (g.province <> '' OR g.city <> '' OR g.isp <> '') AS unsupported,
       g.name AS line_name
FROM waf_geo_rules w
JOIN geo_regions g ON lower(g.code) = lower(w.value)
WHERE w.kind IN ('region_allow', 'region_deny');

-- 2. Rows whose line is gone, or that name no place, matched nothing: drop them.
DELETE FROM waf_geo_rules w
WHERE w.kind IN ('region_allow', 'region_deny')
  AND NOT EXISTS (
      SELECT 1 FROM waf_geo_region_conv c WHERE c.id = w.id AND c.new_value <> ''
  );

-- 3. A converted rule may equal an existing 地区 rule. Keep the existing row,
--    switched on if the region rule it replaces was in force.
UPDATE waf_geo_rules e
SET enabled = TRUE
FROM waf_geo_region_conv c
WHERE e.domain_id = c.domain_id
  AND e.kind = c.new_kind
  AND e.value = c.new_value
  AND c.new_value <> ''
  AND c.enabled
  AND NOT c.unsupported
  AND NOT e.enabled;

DELETE FROM waf_geo_rules w
USING waf_geo_region_conv c
WHERE w.id = c.id
  AND EXISTS (
      SELECT 1 FROM waf_geo_rules e
      WHERE e.domain_id = c.domain_id
        AND e.kind = c.new_kind
        AND e.value = c.new_value
  );

-- 4. Two region rules may convert to the same rule; keep the one in force.
DELETE FROM waf_geo_rules w
USING (
    SELECT id,
           row_number() OVER (
               PARTITION BY domain_id, new_kind, new_value
               ORDER BY (enabled AND NOT unsupported) DESC, id
           ) AS rn
    FROM waf_geo_region_conv
    WHERE new_value <> ''
) d
WHERE w.id = d.id AND d.rn > 1;

-- 5. Rewrite what is left in place. Rules from province / ISP lines come over
--    switched off, so converting one never starts blocking a whole country.
UPDATE waf_geo_rules w
SET kind = c.new_kind,
    value = c.new_value,
    enabled = w.enabled AND NOT c.unsupported,
    note = CASE
        WHEN c.unsupported THEN
            trim(w.note || ' 原 DNS 线路「' || c.line_name || '」含省份/城市/运营商，边缘无法识别，已停用')
        WHEN w.note = '' THEN '原 DNS 线路「' || c.line_name || '」'
        ELSE w.note
    END
FROM waf_geo_region_conv c
WHERE w.id = c.id
  AND w.kind IN ('region_allow', 'region_deny');

DROP TABLE waf_geo_region_conv;

-- 6. 网络号 (ASN) rules. No GeoLite2-ASN database ships with the edge, so an
--    ASN deny entry never matched and an ASN allow entry blocked every
--    visitor. The console no longer offers them; drop what is stored.
DELETE FROM waf_geo_rules WHERE kind IN ('asn_allow', 'asn_deny');

ALTER TABLE waf_geo_rules DROP CONSTRAINT IF EXISTS waf_geo_rules_kind_check;
ALTER TABLE waf_geo_rules
    ADD CONSTRAINT waf_geo_rules_kind_check CHECK (
        kind IN ('country_allow', 'country_deny')
    );

-- 7. DNS lines: a whole-continent line stored the countries the console
--    catalog listed at the time (85 in all), so most of each continent fell
--    through to the default line. Keep only the continent code; it is matched
--    against the continent GeoLite2 reports for the visitor.
UPDATE geo_regions
SET countries = '', updated_at = NOW()
WHERE kind = 'continent'
  AND continent <> ''
  AND countries <> ''
  AND province = ''
  AND city = ''
  AND isp = '';
