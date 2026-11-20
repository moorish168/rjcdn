-- Edge nodes maintain proxied A/AAAA rows on the CDN schedule zone.
-- Answers still come from GeoDNS; these rows mirror inventory for operators
-- and stay out of static multi-A answers (cdn_proxied = true).
ALTER TABLE dns_records
    ADD COLUMN IF NOT EXISTS managed_by_node_id UUID
        REFERENCES edge_nodes(id) ON DELETE CASCADE;

CREATE UNIQUE INDEX IF NOT EXISTS dns_records_managed_node_uidx
    ON dns_records (managed_by_node_id)
    WHERE managed_by_node_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS dns_records_managed_node_idx
    ON dns_records (managed_by_node_id)
    WHERE managed_by_node_id IS NOT NULL;
