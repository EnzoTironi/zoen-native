-- Optional authenticated peer-clock evidence. Legacy space_receipts remains unchanged.
CREATE TABLE space_read_times (
    space TEXT NOT NULL,
    identity TEXT COLLATE "C" NOT NULL,
    after_seq NUMERIC(20,0) NOT NULL CHECK(after_seq>=0),
    through_seq NUMERIC(20,0) NOT NULL CHECK(through_seq>after_seq AND through_seq<=18446744073709551615),
    reported_at_ms BIGINT NOT NULL CHECK(reported_at_ms>0 AND reported_at_ms<=253402300799999),
    observed_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    PRIMARY KEY(space,identity,after_seq,through_seq)
);
CREATE INDEX read_time_retention ON space_read_times(observed_at);
CREATE INDEX read_time_peer ON space_read_times(space,identity,observed_at DESC);
