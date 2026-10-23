-- Explicit public delivery copies. Private editing remains in encrypted Items.
CREATE TABLE site_publications (
    site_id TEXT PRIMARY KEY,
    owner_id TEXT NOT NULL REFERENCES identities(id),
    head TEXT NOT NULL,
    active BOOLEAN NOT NULL,
    object_key TEXT,
    byte_length BIGINT,
    content_sha256 TEXT,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK ((active AND object_key IS NOT NULL AND byte_length IS NOT NULL AND byte_length > 0
            AND byte_length <= 2097152 AND content_sha256 IS NOT NULL)
        OR (NOT active AND object_key IS NULL AND byte_length IS NULL
            AND content_sha256 IS NULL))
);
CREATE INDEX site_publications_owner ON site_publications(owner_id);

CREATE TABLE site_publication_receipts (
    site_id TEXT NOT NULL REFERENCES site_publications(site_id),
    request_id TEXT NOT NULL,
    intent_sha256 TEXT NOT NULL,
    receipt JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (site_id, request_id),
    UNIQUE (site_id, intent_sha256)
);
