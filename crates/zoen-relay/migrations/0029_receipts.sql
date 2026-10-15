-- Aggregated receipts (ADR: "read up to X"): one watermark per person per Space, never
-- one row per message. Only the newest counts, so the relay can replay it on sync.
CREATE TABLE space_receipts (
    space     TEXT NOT NULL,
    identity  TEXT NOT NULL,
    delivered BIGINT NOT NULL DEFAULT 0,
    read      BIGINT NOT NULL DEFAULT 0,
    PRIMARY KEY (space, identity)
);
