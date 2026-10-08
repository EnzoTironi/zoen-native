-- Zoen Sync relay, schema v1.
-- The relay stores signed envelopes and the per-Space order. It never rewrites content.

CREATE TABLE identities (
    id          TEXT PRIMARY KEY,               -- Ed25519 identity key (hex)
    handle      TEXT UNIQUE,                    -- normalized, lowercase
    kind        TEXT NOT NULL,                  -- Person | Agent
    owner       TEXT REFERENCES identities(id), -- agents: who answers for them
    profile     JSONB NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX identities_handle_prefix ON identities (handle text_pattern_ops);

CREATE TABLE devices (
    device      TEXT PRIMARY KEY,               -- Ed25519 device key (hex)
    identity    TEXT NOT NULL REFERENCES identities(id),
    cert        TEXT NOT NULL,                  -- identity's signature over the device key
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_seen   TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX devices_by_identity ON devices (identity);

CREATE TABLE spaces (
    id          TEXT PRIMARY KEY,
    kind        TEXT NOT NULL,                  -- Direct | Group | Community | Personal
    privacy     TEXT NOT NULL,                  -- EndToEnd | Closed | Public
    created_by  TEXT NOT NULL REFERENCES identities(id),
    head_seq    BIGINT NOT NULL DEFAULT -1,
    head_hash   TEXT NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE members (
    space       TEXT NOT NULL REFERENCES spaces(id),
    identity    TEXT NOT NULL REFERENCES identities(id),
    role        TEXT NOT NULL,                  -- Owner | Admin | Member | Reader
    since_seq   BIGINT NOT NULL,
    PRIMARY KEY (space, identity)
);
CREATE INDEX members_by_identity ON members (identity);

-- The log. One row per sequenced envelope; (space, seq) is the total order.
-- In E2EE Spaces `envelope` holds ciphertext only (payload.k = 'sealed').
CREATE TABLE events (
    space       TEXT NOT NULL REFERENCES spaces(id),
    seq         BIGINT NOT NULL,
    prev        TEXT NOT NULL,
    hash        TEXT NOT NULL,
    author      TEXT NOT NULL,
    device      TEXT,
    client_id   TEXT NOT NULL,
    at_ms       BIGINT NOT NULL,
    envelope    JSONB NOT NULL,
    received_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (space, seq),
    UNIQUE (space, author, client_id)
);

CREATE TABLE invites (
    code_hash   TEXT PRIMARY KEY,               -- SHA-256 of the code; the code itself is never stored
    space       TEXT NOT NULL REFERENCES spaces(id),
    role        TEXT NOT NULL,
    created_by  TEXT NOT NULL REFERENCES identities(id),
    max_uses    INTEGER NOT NULL,
    uses        INTEGER NOT NULL DEFAULT 0,
    expires_at  TIMESTAMPTZ NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX invites_by_space ON invites (space);
