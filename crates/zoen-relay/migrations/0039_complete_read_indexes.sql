-- Unique keyset order, independent of tied/late transaction creation timestamps.
-- No new authority/revision/storage table: only indexes on existing owner-key rows.
CREATE INDEX devices_complete_read_keyset ON devices (identity, device COLLATE "C");
CREATE INDEX agent_cursors_complete_read_keyset ON agent_space_cursors (agent, space COLLATE "C");
