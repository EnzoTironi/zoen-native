-- MLS key packages (ADR 0026): how a device is added to an end-to-end Space while it's
-- offline. Each is single-use, except one last-resort package per device that claims fall
-- back to and never delete. Rows hold public keys only.
CREATE TABLE key_packages (
    id           BIGSERIAL PRIMARY KEY,
    identity     TEXT NOT NULL REFERENCES identities(id),
    device       TEXT NOT NULL,
    data         BYTEA NOT NULL,
    last_resort  BOOLEAN NOT NULL DEFAULT false,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX key_packages_by_device ON key_packages (identity, device, last_resort, id);
CREATE UNIQUE INDEX key_packages_one_last_resort ON key_packages (identity, device) WHERE last_resort;
