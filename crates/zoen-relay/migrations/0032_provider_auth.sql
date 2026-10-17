-- Provider assertions authorize an explicit binding, never an email-based account merge.
CREATE TABLE provider_challenges (
    id TEXT PRIMARY KEY,
    provider TEXT NOT NULL CHECK (provider IN ('apple','google')),
    client TEXT NOT NULL CHECK (client IN ('ios','web')),
    audience TEXT NOT NULL,
    presenter TEXT,
    origin TEXT,
    nonce TEXT NOT NULL,
    state TEXT NOT NULL,
    created_ms BIGINT NOT NULL,
    expires_ms BIGINT NOT NULL,
    consumed_ms BIGINT
);
CREATE INDEX provider_challenges_expiry ON provider_challenges(expires_ms);

CREATE TABLE provider_bindings (
    provider TEXT NOT NULL,
    issuer TEXT NOT NULL,
    subject TEXT NOT NULL,
    identity TEXT NOT NULL REFERENCES identities(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY(provider,issuer,subject),
    UNIQUE(identity,provider)
);

CREATE TABLE provider_proofs (
    token_hash TEXT PRIMARY KEY,
    challenge TEXT NOT NULL UNIQUE REFERENCES provider_challenges(id) ON DELETE CASCADE,
    provider TEXT NOT NULL,
    issuer TEXT NOT NULL,
    subject TEXT NOT NULL,
    origin TEXT,
    expires_ms BIGINT NOT NULL,
    consumed_ms BIGINT,
    bound_fingerprint TEXT,
    bound_identity TEXT,
    bound_device TEXT,
    bound_handle TEXT
);
CREATE INDEX provider_proofs_expiry ON provider_proofs(expires_ms);
