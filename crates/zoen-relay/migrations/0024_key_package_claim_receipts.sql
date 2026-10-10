-- Claim replies are public MLS packages. Keep the exact reply with the destructive
-- DELETE, scoped to the authenticated source device. Operation ULIDs expire after
-- 24 h, so GC cannot turn an old replay into another destructive claim.
CREATE TABLE key_package_claim_receipts (
    source_identity TEXT NOT NULL REFERENCES identities(id),
    source_device   TEXT NOT NULL REFERENCES devices(device),
    operation_id    TEXT NOT NULL CHECK (length(operation_id) = 26),
    targets         TEXT[] NOT NULL,
    response        BYTEA NOT NULL CHECK (octet_length(response) <= 8388608),
    expires_at_ms   BIGINT NOT NULL,
    PRIMARY KEY (source_identity, source_device, operation_id)
);
CREATE INDEX key_package_claim_receipts_expiry
    ON key_package_claim_receipts (expires_at_ms);
