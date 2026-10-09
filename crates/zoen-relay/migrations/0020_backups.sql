-- Encrypted server backups (ADR 0045). The relay holds a vault per person that guards the
-- backup key; it never holds the key, the password or anything readable.
CREATE TABLE backup_vaults (
    identity     TEXT PRIMARY KEY REFERENCES identities(id),
    mode         TEXT NOT NULL,                 -- passphrase | recovery_key
    oprf_key     BYTEA,                         -- passphrase: OPRF key sealed by the vault (HSM boundary)
    verifier     BYTEA NOT NULL,                -- sha256(auth_key)
    wrapped_key  BYTEA NOT NULL,                -- the backup key, sealed by the device
    kdf          JSONB NOT NULL,
    guesses      INTEGER NOT NULL DEFAULT 0,
    armed        BOOLEAN NOT NULL DEFAULT false, -- a guess was just spent on an evaluation; the next open is that guess
    locked       BOOLEAN NOT NULL DEFAULT false,
    blob_sha     TEXT,
    blob_bytes   BIGINT,
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);
-- A password setup before the first vault exists.
CREATE TABLE backup_pending (
    identity     TEXT PRIMARY KEY REFERENCES identities(id),
    pending_key  BYTEA NOT NULL,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);
