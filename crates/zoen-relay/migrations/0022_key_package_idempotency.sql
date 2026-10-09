-- Replay receipts survive consumption of a single-use package. New receipts expire
-- with the signed MLS lifetime. Backfilled receipts use the accepted maximum lifetime
-- of 84 days plus 1 h, which covers every still-valid pre-upgrade package accepted now.
CREATE TABLE key_package_publications (
    identity     TEXT NOT NULL REFERENCES identities(id),
    device       TEXT NOT NULL,
    package_hash BYTEA NOT NULL CHECK (octet_length(package_hash) = 32),
    expires_at   BIGINT NOT NULL,
    PRIMARY KEY (identity, device, package_hash)
);
CREATE INDEX key_package_publications_expiry ON key_package_publications (expires_at);

INSERT INTO key_package_publications (identity, device, package_hash, expires_at)
SELECT DISTINCT identity, device, sha256(data),
       floor(extract(epoch FROM now() + interval '84 days 1 hour'))::bigint
FROM key_packages WHERE NOT last_resort;

-- Old timed-out publications could already have produced duplicate active rows.
DELETE FROM key_packages k USING key_packages earlier
WHERE NOT k.last_resort AND NOT earlier.last_resort
  AND k.identity = earlier.identity AND k.device = earlier.device
  AND k.data = earlier.data AND k.id > earlier.id;
