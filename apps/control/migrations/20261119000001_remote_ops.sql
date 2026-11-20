-- Remote node management over SSH: reusable credentials ("SSH 认证"), the
-- SSH endpoint of each node, and a log of remote operations.

-- Reusable SSH credentials. Secrets are AES-256-GCM encrypted by the control
-- plane (see service/remote/crypto.rs); the database never holds plaintext.
CREATE TABLE IF NOT EXISTS ssh_grants (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name            TEXT NOT NULL,
    -- password | key
    method          TEXT NOT NULL,
    username        TEXT NOT NULL,
    -- password (method=password) or PEM private key (method=key), encrypted
    secret_enc      TEXT NOT NULL DEFAULT '',
    -- private key passphrase, encrypted; empty when the key has none
    passphrase_enc  TEXT NOT NULL DEFAULT '',
    -- run commands through sudo (non-root users)
    use_sudo        BOOLEAN NOT NULL DEFAULT FALSE,
    description     TEXT NOT NULL DEFAULT '',
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX IF NOT EXISTS ssh_grants_name_idx ON ssh_grants (name);

-- SSH endpoint of an edge node. Kept out of edge_nodes so the hot node queries
-- (heartbeat, scheduling) are unaffected.
CREATE TABLE IF NOT EXISTS node_ssh (
    node_id     UUID PRIMARY KEY REFERENCES edge_nodes(id) ON DELETE CASCADE,
    host        TEXT NOT NULL,
    port        INTEGER NOT NULL DEFAULT 22,
    -- RESTRICT: a credential in use cannot be deleted by accident
    grant_id    UUID REFERENCES ssh_grants(id) ON DELETE RESTRICT,
    -- pinned server host key fingerprint (SHA256:...), recorded on first use
    host_key    TEXT NOT NULL DEFAULT '',
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS node_ssh_grant_idx ON node_ssh (grant_id);

-- Remote operations (install / upgrade / reinstall / start / stop / uninstall).
CREATE TABLE IF NOT EXISTS node_ops (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    node_id      UUID NOT NULL REFERENCES edge_nodes(id) ON DELETE CASCADE,
    action       TEXT NOT NULL,
    -- queued | running | success | failed
    status       TEXT NOT NULL DEFAULT 'queued',
    -- operation inputs (release version, control URL), JSON text
    params       TEXT NOT NULL DEFAULT '{}',
    log          TEXT NOT NULL DEFAULT '',
    error        TEXT NOT NULL DEFAULT '',
    created_by   TEXT NOT NULL DEFAULT '',
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    started_at   TIMESTAMPTZ,
    finished_at  TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS node_ops_node_idx ON node_ops (node_id, created_at DESC);
CREATE INDEX IF NOT EXISTS node_ops_created_idx ON node_ops (created_at DESC);
-- At most one active operation per node, enforced by the database so two
-- concurrent clicks cannot both start one.
CREATE UNIQUE INDEX IF NOT EXISTS node_ops_one_active_idx
    ON node_ops (node_id) WHERE status IN ('queued', 'running');
