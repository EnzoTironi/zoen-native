-- Format v3: the relay stores each author's exact signed bytes (protobuf) and never
-- re-encodes them. v2 logs were signed over JSON and can't be carried over, and v2
-- clients wipe themselves on upgrade, so this pre-launch reset is a clean break: every
-- table restarts. From v3 on, unknown fields and kinds survive verbatim, so no later
-- format change needs a reset.
TRUNCATE invites, members, events, spaces, devices, identities;

ALTER TABLE events DROP COLUMN envelope;
ALTER TABLE events ADD COLUMN content BYTEA NOT NULL;
ALTER TABLE events ADD COLUMN sig TEXT NOT NULL;
ALTER TABLE events ADD COLUMN cert TEXT;
