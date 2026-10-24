-- A live immutable mailbox can acknowledge only its original publication.
-- Existing rows keep NULL provenance and remain ambiguous on sender retry.
ALTER TABLE link_boxes ADD COLUMN publisher_identity TEXT;
ALTER TABLE link_boxes ADD COLUMN publisher_device TEXT;
ALTER TABLE link_boxes ADD COLUMN target_device TEXT;
ALTER TABLE link_boxes ADD COLUMN target_certificate TEXT;
ALTER TABLE link_boxes ADD CONSTRAINT link_box_publisher_pair
    CHECK ((publisher_identity IS NULL) = (publisher_device IS NULL));
ALTER TABLE link_boxes ADD CONSTRAINT link_box_target_pair
    CHECK ((target_device IS NULL) = (target_certificate IS NULL));
