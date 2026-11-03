-- A shared runtime uses the existing detector row, not a parallel balance or
-- checkpoint ledger. Existing single-device deployments retain profile 2.
ALTER TABLE runtime_image_checkpoint
    ADD COLUMN profile SMALLINT NOT NULL DEFAULT 2 CHECK (profile IN (2, 3)),
    ALTER COLUMN agent DROP NOT NULL,
    ALTER COLUMN device DROP NOT NULL,
    ADD CONSTRAINT runtime_checkpoint_scope CHECK (
        (profile = 2 AND agent IS NOT NULL AND device IS NOT NULL)
        OR (profile = 3 AND agent IS NULL AND device IS NULL)
    );
