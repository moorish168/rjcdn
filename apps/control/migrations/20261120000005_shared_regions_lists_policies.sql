-- Shared geo/line catalog for Smart DNS + WAF, reusable IP lists, and cache policies.

-- ─── Shared region / line catalog (DNS 线路 + WAF 区域) ─────────────────────
CREATE TABLE IF NOT EXISTS geo_regions (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    code        TEXT NOT NULL,
    name        TEXT NOT NULL,
    -- default | country | province | isp | custom
    kind        TEXT NOT NULL DEFAULT 'custom'
                CHECK (kind IN ('default', 'country', 'province', 'isp', 'custom')),
    country     TEXT NOT NULL DEFAULT '',
    province    TEXT NOT NULL DEFAULT '',
    city        TEXT NOT NULL DEFAULT '',
    isp         TEXT NOT NULL DEFAULT '',
    is_default  BOOLEAN NOT NULL DEFAULT FALSE,
    enabled     BOOLEAN NOT NULL DEFAULT TRUE,
    sort_order  INTEGER NOT NULL DEFAULT 100,
    note        TEXT NOT NULL DEFAULT '',
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX IF NOT EXISTS geo_regions_code_uidx
    ON geo_regions (lower(code));
-- Exactly one default line for DNS fallback.
CREATE UNIQUE INDEX IF NOT EXISTS geo_regions_one_default_uidx
    ON geo_regions ((is_default))
    WHERE is_default = TRUE;

INSERT INTO geo_regions (code, name, kind, country, province, city, isp, is_default, sort_order, note)
SELECT v.code, v.name, v.kind, v.country, v.province, v.city, v.isp, v.is_default, v.sort_order, v.note
FROM (VALUES
    ('default', '默认线路', 'default', '', '', '', '', TRUE, 0, 'DNS 未命中专线时的降级线路'),
    ('cn', '中国大陆', 'country', 'CN', '', '', '', FALSE, 10, ''),
    ('cn-telecom', '中国电信', 'isp', 'CN', '', '', '电信', FALSE, 20, ''),
    ('cn-unicom', '中国联通', 'isp', 'CN', '', '', '联通', FALSE, 21, ''),
    ('cn-mobile', '中国移动', 'isp', 'CN', '', '', '移动', FALSE, 22, ''),
    ('cn-gd', '广东', 'province', 'CN', '广东', '', '', FALSE, 30, ''),
    ('cn-bj', '北京', 'province', 'CN', '北京', '', '', FALSE, 31, ''),
    ('cn-sh', '上海', 'province', 'CN', '上海', '', '', FALSE, 32, ''),
    ('cn-zj', '浙江', 'province', 'CN', '浙江', '', '', FALSE, 33, ''),
    ('cn-js', '江苏', 'province', 'CN', '江苏', '', '', FALSE, 34, ''),
    ('overseas', '海外', 'custom', '', '', '', '', FALSE, 90, '非中国大陆兜底标签')
) AS v(code, name, kind, country, province, city, isp, is_default, sort_order, note)
WHERE NOT EXISTS (
    SELECT 1 FROM geo_regions g WHERE lower(g.code) = lower(v.code)
);

-- DNS record line selection (NULL = 使用默认线路)
ALTER TABLE dns_records
    ADD COLUMN IF NOT EXISTS line_id UUID REFERENCES geo_regions(id) ON DELETE SET NULL;

DROP INDEX IF EXISTS dns_records_unique_idx;
CREATE UNIQUE INDEX IF NOT EXISTS dns_records_unique_idx
    ON dns_records (
        zone_id,
        name,
        rtype,
        content,
        COALESCE(priority, -1),
        COALESCE(line_id, '00000000-0000-0000-0000-000000000000'::uuid)
    );

CREATE INDEX IF NOT EXISTS dns_records_line_idx ON dns_records (line_id);

-- Edge node binding to shared region/line
ALTER TABLE edge_nodes
    ADD COLUMN IF NOT EXISTS region_id UUID REFERENCES geo_regions(id) ON DELETE SET NULL;

CREATE INDEX IF NOT EXISTS edge_nodes_region_idx ON edge_nodes (region_id);

-- ─── Reusable IP lists (for WAF) ────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS ip_lists (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name        TEXT NOT NULL,
    -- black | white
    list_type   TEXT NOT NULL CHECK (list_type IN ('black', 'white')),
    project_id  UUID REFERENCES projects(id) ON DELETE CASCADE,
    note        TEXT NOT NULL DEFAULT '',
    enabled     BOOLEAN NOT NULL DEFAULT TRUE,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX IF NOT EXISTS ip_lists_name_scope_uidx
    ON ip_lists (lower(name), COALESCE(project_id, '00000000-0000-0000-0000-000000000000'::uuid));

CREATE TABLE IF NOT EXISTS ip_list_entries (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    list_id     UUID NOT NULL REFERENCES ip_lists(id) ON DELETE CASCADE,
    cidr        TEXT NOT NULL,
    note        TEXT NOT NULL DEFAULT '',
    enabled     BOOLEAN NOT NULL DEFAULT TRUE,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX IF NOT EXISTS ip_list_entries_uidx
    ON ip_list_entries (list_id, cidr);
CREATE INDEX IF NOT EXISTS ip_list_entries_list_idx ON ip_list_entries (list_id);

-- Domain WAF references to shared IP lists
CREATE TABLE IF NOT EXISTS waf_ip_list_bindings (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    domain_id   UUID NOT NULL REFERENCES domains(id) ON DELETE CASCADE,
    list_id     UUID NOT NULL REFERENCES ip_lists(id) ON DELETE CASCADE,
    enabled     BOOLEAN NOT NULL DEFAULT TRUE,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (domain_id, list_id)
);

-- WAF region allow/deny against shared geo_regions
CREATE TABLE IF NOT EXISTS waf_region_rules (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    domain_id   UUID NOT NULL REFERENCES domains(id) ON DELETE CASCADE,
    region_id   UUID NOT NULL REFERENCES geo_regions(id) ON DELETE CASCADE,
    -- allow | deny
    kind        TEXT NOT NULL CHECK (kind IN ('allow', 'deny')),
    note        TEXT NOT NULL DEFAULT '',
    enabled     BOOLEAN NOT NULL DEFAULT TRUE,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (domain_id, region_id, kind)
);

CREATE INDEX IF NOT EXISTS waf_region_rules_domain_idx ON waf_region_rules (domain_id);

-- Also allow region_* on legacy geo table for UI convenience (optional kinds).
ALTER TABLE waf_geo_rules DROP CONSTRAINT IF EXISTS waf_geo_rules_kind_check;
ALTER TABLE waf_geo_rules
    ADD CONSTRAINT waf_geo_rules_kind_check CHECK (
        kind IN (
            'country_allow', 'country_deny',
            'asn_allow', 'asn_deny',
            'region_allow', 'region_deny'
        )
    );

-- ─── Reusable cache policies (for domains / nodes) ──────────────────────────
CREATE TABLE IF NOT EXISTS cache_policies (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name        TEXT NOT NULL,
    project_id  UUID REFERENCES projects(id) ON DELETE CASCADE,
    note        TEXT NOT NULL DEFAULT '',
    enabled     BOOLEAN NOT NULL DEFAULT TRUE,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX IF NOT EXISTS cache_policies_name_scope_uidx
    ON cache_policies (lower(name), COALESCE(project_id, '00000000-0000-0000-0000-000000000000'::uuid));

CREATE TABLE IF NOT EXISTS cache_policy_rules (
    id                   UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    policy_id            UUID NOT NULL REFERENCES cache_policies(id) ON DELETE CASCADE,
    path_pattern         TEXT NOT NULL,
    match_type           TEXT NOT NULL DEFAULT 'glob',
    ttl_seconds          INTEGER NOT NULL DEFAULT 60,
    priority             INTEGER NOT NULL DEFAULT 0,
    action               TEXT NOT NULL DEFAULT 'cache',
    swr_seconds          INTEGER NOT NULL DEFAULT 0,
    enabled              BOOLEAN NOT NULL DEFAULT TRUE,
    match_methods        TEXT NOT NULL DEFAULT '',
    match_query          TEXT NOT NULL DEFAULT '',
    browser_ttl_seconds  INTEGER NOT NULL DEFAULT 0,
    ignore_query         BOOLEAN,
    ignore_cookie        BOOLEAN NOT NULL DEFAULT FALSE,
    ignore_key_headers   TEXT,
    match_content_type   TEXT NOT NULL DEFAULT '',
    note                 TEXT NOT NULL DEFAULT '',
    created_at           TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS cache_policy_rules_policy_idx
    ON cache_policy_rules (policy_id, priority DESC);

ALTER TABLE domains
    ADD COLUMN IF NOT EXISTS cache_policy_id UUID REFERENCES cache_policies(id) ON DELETE SET NULL;

ALTER TABLE edge_nodes
    ADD COLUMN IF NOT EXISTS cache_policy_id UUID REFERENCES cache_policies(id) ON DELETE SET NULL;
