-- Experiments, remote config and acquisition source (ADR 0044).

-- Where an account came from (first touch): a kind and an optional campaign id, never the
-- friend or the invite behind it. Deleted with the account row after 35 days.
ALTER TABLE metrics_accounts ADD COLUMN source TEXT;
ALTER TABLE metrics_accounts ADD COLUMN campaign TEXT;

-- Which arm an account was shown, from the first day it was. Deleted 35 days after.
CREATE TABLE metrics_exposures (
    pid        BYTEA NOT NULL,
    flag       TEXT  NOT NULL,
    variant    TEXT  NOT NULL,
    first_day  DATE  NOT NULL,
    PRIMARY KEY (pid, flag)
);
CREATE INDEX metrics_exposures_flag ON metrics_exposures (flag, variant);

-- Remote config versions; the newest is served. Admins add rows, nothing is rewritten.
CREATE TABLE remote_config (
    version     BIGSERIAL PRIMARY KEY,
    document    JSONB NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
