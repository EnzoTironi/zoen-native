-- Push tokens for content-free wakes (ADR 0047). One per device; a new token replaces it.
CREATE TABLE push_tokens (
    identity    TEXT NOT NULL,
    device      TEXT NOT NULL,
    platform    TEXT NOT NULL,
    token       TEXT NOT NULL,
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (identity, device)
);
CREATE INDEX push_tokens_token ON push_tokens (token);
