-- Keep the last restorable backup active until its replacement object is durable.
ALTER TABLE backup_vaults ADD COLUMN generation TEXT;
ALTER TABLE backup_pending ADD COLUMN generation TEXT;
ALTER TABLE backup_pending ADD COLUMN device TEXT;

CREATE TABLE backup_setups (
    identity TEXT PRIMARY KEY REFERENCES identities(id),
    generation TEXT NOT NULL CHECK (length(generation) = 64),
    mode TEXT NOT NULL,
    oprf_key BYTEA,
    verifier BYTEA NOT NULL,
    wrapped_key BYTEA NOT NULL,
    kdf JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
