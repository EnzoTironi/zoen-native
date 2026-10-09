-- Never reuse an object path, even when a device retries identical ciphertext.
-- A delayed cloud DELETE must not be able to delete a newer active upload.
ALTER TABLE backup_vaults ADD COLUMN blob_key TEXT;
UPDATE backup_vaults SET blob_key = 'backups/v2/' || identity || '/' || blob_sha
    WHERE generation IS NOT NULL AND blob_sha IS NOT NULL;
-- Existing wrappers still name their original random backup key. Assign an opaque
-- generation without changing the wrapper or object; a successful restore learns it.
UPDATE backup_vaults SET generation = replace(gen_random_uuid()::text, '-', '')
    || replace(gen_random_uuid()::text, '-', '') WHERE generation IS NULL;
