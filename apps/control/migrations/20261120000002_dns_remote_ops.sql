-- Remote install for authoritative DNS nodes, reusing global ssh_grants.

CREATE TABLE IF NOT EXISTS dns_node_ssh (
    node_id     UUID PRIMARY KEY REFERENCES dns_nodes(id) ON DELETE CASCADE,
    host        TEXT NOT NULL,
    port        INTEGER NOT NULL DEFAULT 22,
    grant_id    UUID REFERENCES ssh_grants(id) ON DELETE RESTRICT,
    host_key    TEXT NOT NULL DEFAULT '',
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS dns_node_ssh_grant_idx ON dns_node_ssh (grant_id);

CREATE TABLE IF NOT EXISTS dns_node_ops (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    node_id      UUID NOT NULL REFERENCES dns_nodes(id) ON DELETE CASCADE,
    action       TEXT NOT NULL,
    status       TEXT NOT NULL DEFAULT 'queued',
    params       TEXT NOT NULL DEFAULT '{}',
    log          TEXT NOT NULL DEFAULT '',
    error        TEXT NOT NULL DEFAULT '',
    created_by   TEXT NOT NULL DEFAULT '',
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    started_at   TIMESTAMPTZ,
    finished_at  TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS dns_node_ops_node_idx ON dns_node_ops (node_id, created_at DESC);
CREATE INDEX IF NOT EXISTS dns_node_ops_created_idx ON dns_node_ops (created_at DESC);
CREATE UNIQUE INDEX IF NOT EXISTS dns_node_ops_one_active_idx
    ON dns_node_ops (node_id) WHERE status IN ('queued', 'running');
