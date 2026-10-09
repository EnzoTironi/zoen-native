-- Linking and unlinking devices (ADR 0045).

-- An unlinked device can't sign in again; its row stays so it can't come back quietly.
ALTER TABLE devices ADD COLUMN revoked_at TIMESTAMPTZ;

-- One-time sealed boxes for a device being linked, under SHA-256 of the link secret in
-- its QR code. Taken (and deleted) by the first fetch; older than 15 minutes, gone.
CREATE TABLE link_boxes (
    id          TEXT PRIMARY KEY,
    sealed      BYTEA NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
