-- People's names and bios leave the server (ADR 0016). The directory keeps what lookup
-- needs (id, handle, kind, tint); profiles become ciphertext only contacts can open.
-- Agents stay public directory entries: they are services people choose to talk to.
UPDATE identities SET profile = profile || '{"name": "", "bio": ""}'::jsonb WHERE kind = 'Person';

CREATE TABLE agreement_keys (
    identity    TEXT PRIMARY KEY REFERENCES identities(id),
    public      TEXT NOT NULL,                  -- X25519 (hex)
    device      TEXT NOT NULL,                  -- the device that signed it
    sig         TEXT NOT NULL,
    cert        TEXT NOT NULL,
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE profiles (
    identity    TEXT PRIMARY KEY REFERENCES identities(id),
    version     BIGINT NOT NULL,
    ciphertext  BYTEA NOT NULL,
    device      TEXT NOT NULL,
    sig         TEXT NOT NULL,
    cert        TEXT NOT NULL,
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
