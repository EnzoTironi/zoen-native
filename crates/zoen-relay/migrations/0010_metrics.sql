-- Product metrics (ADR 0043, docs/product/metricas.md). Counts only: no content, no
-- handles, no raw ids. `pid`/`sid` are keyed pseudonyms (HMAC-SHA256 with ZOEN_METRICS_KEY,
-- 16 bytes) that the directory can't be joined against without the key. Per-account and
-- per-Space rows are deleted after 35 days; only daily aggregates are kept.

-- One row per account per UTC day it did something.
CREATE TABLE metrics_activity (
    day       DATE     NOT NULL,
    pid       BYTEA    NOT NULL,
    kind      SMALLINT NOT NULL,              -- 0 person, 1 agent
    test      BOOLEAN  NOT NULL,              -- QA/test account (ZOEN_METRICS_TEST_HANDLES)
    cohort    DATE,                           -- signup day, when within the window
    sent      INTEGER  NOT NULL DEFAULT 0,    -- messages sent (plain MessagePosted or MLS application)
    syncs     INTEGER  NOT NULL DEFAULT 0,    -- catch-up syncs completed
    sessions  INTEGER  NOT NULL DEFAULT 0,    -- authenticated sessions opened
    peers     BIGINT   NOT NULL DEFAULT 0,    -- 64-bit sketch of who it wrote to (1 bit per pseudonym)
    PRIMARY KEY (day, pid)
);

-- Accounts younger than 35 days: activation and invite facts.
CREATE TABLE metrics_accounts (
    pid               BYTEA PRIMARY KEY,
    signup_at         TIMESTAMPTZ NOT NULL,
    kind              SMALLINT NOT NULL,
    test              BOOLEAN NOT NULL,
    invited           BOOLEAN NOT NULL DEFAULT false,
    first_message_at  TIMESTAMPTZ
);
CREATE INDEX metrics_accounts_signup ON metrics_accounts (signup_at);

-- One row per Space per day it carried a message.
CREATE TABLE metrics_spaces (
    day       DATE    NOT NULL,
    sid       BYTEA   NOT NULL,
    members   INTEGER NOT NULL,
    messages  INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (day, sid)
);

-- Global daily counters (invites, Spaces created, approvals, failures, latency buckets).
CREATE TABLE metrics_counters (
    day    DATE   NOT NULL,
    name   TEXT   NOT NULL,
    value  BIGINT NOT NULL,
    PRIMARY KEY (day, name)
);

-- The closed days' numbers, kept for good (aggregates only).
CREATE TABLE metrics_daily (
    day             DATE PRIMARY KEY,
    stats           JSONB NOT NULL,
    computed_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    exported_at     TIMESTAMPTZ
);

-- Dev fallback for the pseudonym key when ZOEN_METRICS_KEY isn't set (staging/prod set it).
CREATE TABLE metrics_key (
    one  BOOLEAN PRIMARY KEY DEFAULT true CHECK (one),
    key  BYTEA NOT NULL
);
