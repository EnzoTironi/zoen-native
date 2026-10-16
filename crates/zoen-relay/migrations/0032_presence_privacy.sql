-- Account policy is shared by every device. Presence signals stay ephemeral; only
-- the last observed offline time and the visibility preference are durable.
CREATE TABLE presence_preferences (
    identity TEXT PRIMARY KEY REFERENCES identities(id) ON DELETE CASCADE,
    hide_last_seen BOOLEAN NOT NULL DEFAULT false,
    last_seen_ms BIGINT CHECK (last_seen_ms >= 0)
);
