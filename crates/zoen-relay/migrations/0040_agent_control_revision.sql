-- Lifecycle stays in the canonical citizen row. A stale operation cannot cross a
-- later revision, including a legacy control from another owner device.
ALTER TABLE agent_citizens
 ADD COLUMN control_revision BIGINT NOT NULL DEFAULT 1 CHECK (control_revision > 0),
 ADD COLUMN control_operation TEXT NOT NULL DEFAULT '',
 ADD COLUMN control_action TEXT NOT NULL DEFAULT '',
 ADD COLUMN control_device TEXT NOT NULL DEFAULT '';
