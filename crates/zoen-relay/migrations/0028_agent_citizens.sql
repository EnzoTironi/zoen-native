-- First-class agent citizens: a durable runtime device may claim an Agent identity
-- so the relay can wake it while its owner is offline. Grants no Space membership,
-- spending budget or tool permission (ADR 0049).

CREATE TABLE agent_citizens (
    agent           TEXT PRIMARY KEY REFERENCES identities(id),
    device          TEXT NOT NULL REFERENCES devices(device),
    definition_version INTEGER NOT NULL DEFAULT 1,
    -- Creator | User. The runtime interprets the sealed definition blob.
    kind            TEXT NOT NULL CHECK (kind IN ('Creator', 'User')),
    -- Opaque versioned definition (persona, rules, free JSON state). Relay never parses it.
    definition      BYTEA NOT NULL,
    -- Hand-drawn animated avatar hint until the art asset is content-addressed.
    avatar_style    TEXT NOT NULL DEFAULT 'hand-drawn-animated',
    avatar_glyph    TEXT,
    avatar_tint     TEXT,
    avatar_asset    TEXT,
    claimed_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_wake_at    TIMESTAMPTZ,
    last_seen_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX agent_citizens_by_device ON agent_citizens (device);

-- Per-Space wake cursor so agentd resumes after reconnect without replaying forever.
CREATE TABLE agent_space_cursors (
    agent       TEXT NOT NULL REFERENCES agent_citizens(agent) ON DELETE CASCADE,
    space       TEXT NOT NULL,
    last_seq    BIGINT NOT NULL DEFAULT -1,
    PRIMARY KEY (agent, space)
);
