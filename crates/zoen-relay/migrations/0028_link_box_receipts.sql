-- Link boxes wait for the recorded device's receipt (ADR 0045).
-- Pre-launch: nothing already in link_boxes is kept.
DELETE FROM link_boxes;
ALTER TABLE link_boxes ADD COLUMN device TEXT NOT NULL;
