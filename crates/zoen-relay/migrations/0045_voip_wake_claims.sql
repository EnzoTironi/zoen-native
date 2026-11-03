-- A committed event can trigger at most one VoIP dispatch across relay replicas.
CREATE TABLE voip_wake_claims (
    space TEXT NOT NULL,
    sequence BIGINT NOT NULL CHECK (sequence >= 0),
    expires_ms BIGINT NOT NULL,
    PRIMARY KEY (space, sequence)
);
CREATE INDEX voip_wake_claims_expiry ON voip_wake_claims (expires_ms);
