-- Publication opportunistically removes a bounded batch of the oldest expired boxes.
-- Without this index even an empty cleanup scans every live box.
CREATE INDEX link_boxes_expiry ON link_boxes (created_at);
