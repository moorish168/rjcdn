ALTER TABLE edge_nodes
    ADD COLUMN IF NOT EXISTS disk_total_bytes BIGINT,
    ADD COLUMN IF NOT EXISTS disk_available_bytes BIGINT;
