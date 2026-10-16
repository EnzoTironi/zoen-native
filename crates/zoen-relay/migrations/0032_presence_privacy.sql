-- Account policy is shared by every device. Presence signals stay ephemeral; only
-- the last observed offline time and the visibility preference are durable.
CREATE SEQUENCE presence_revision;
CREATE TABLE presence_preferences (
    identity TEXT PRIMARY KEY REFERENCES identities(id) ON DELETE CASCADE,
    hide_last_seen BOOLEAN NOT NULL DEFAULT false,
    revision BIGINT NOT NULL DEFAULT 0,
    last_seen_ms BIGINT CHECK (last_seen_ms >= 0)
);
