-- First-class agent citizens: a durable runtime device may claim an Agent identity
-- so the relay can wake it while its owner is offline. Grants no Space membership,
-- spending budget or tool permission (ADR 0049).

CREATE TABLE agent_citizens (
    agent           TEXT PRIMARY KEY REFERENCES identities(id) ON DELETE CASCADE,
    -- The Person who answers for the agent: the only one who may define, pause or revoke it.
    owner           TEXT NOT NULL REFERENCES identities(id) ON DELETE CASCADE,
    -- The runtime device. Losing the device row unclaims the agent.
    device          TEXT NOT NULL REFERENCES devices(device) ON DELETE CASCADE,
    -- Creator | User, set with the first definition (NULL until then).
    kind            TEXT CHECK (kind IN ('Creator', 'User')),
    -- Current version; full history in agent_definitions. 0 = not defined yet.
    definition_version INTEGER NOT NULL DEFAULT 0,
    -- Opaque versioned definition (persona, rules, free JSON state). Relay never parses it.
    definition      BYTEA NOT NULL DEFAULT ''::bytea,
    -- Hand-drawn animated avatar hint until the art asset is content-addressed.
    avatar_style    TEXT NOT NULL DEFAULT 'hand-drawn-animated',
    avatar_glyph    TEXT,
    avatar_tint     TEXT,
    avatar_asset    TEXT,
    -- The owner paused it: the runtime stays silent but keeps its cursors moving.
    paused_at       TIMESTAMPTZ,
    -- The owner revoked it: final; its runtime device is revoked in the same transaction.
    revoked_at      TIMESTAMPTZ,
    claimed_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_wake_at    TIMESTAMPTZ,
    last_seen_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX agent_citizens_by_device ON agent_citizens (device);
CREATE INDEX agent_citizens_by_owner ON agent_citizens (owner);

-- Every definition an owner ever approved, so a change is auditable and reversible.
CREATE TABLE agent_definitions (
    agent       TEXT NOT NULL REFERENCES agent_citizens(agent) ON DELETE CASCADE,
    version     INTEGER NOT NULL CHECK (version > 0),
    kind        TEXT NOT NULL CHECK (kind IN ('Creator', 'User')),
    definition  BYTEA NOT NULL,
    defined_by  TEXT NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (agent, version)
);

-- Per-Space wake cursor: the single source of truth for where the runtime resumes.
-- Spaces and membership live in FoundationDB (0003), so `space` can't be a foreign key:
-- the relay checks membership on every advance and drops the cursors of Spaces the agent
-- left when it serves them (crate::agents::state), which is the cascade.
CREATE TABLE agent_space_cursors (
    agent       TEXT NOT NULL REFERENCES agent_citizens(agent) ON DELETE CASCADE,
    space       TEXT NOT NULL,
    last_seq    BIGINT NOT NULL,
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (agent, space)
);
