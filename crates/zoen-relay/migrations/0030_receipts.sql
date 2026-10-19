-- Fresh prelaunch baseline: full protocol UInt64, exact PostgreSQL decimal.
CREATE TABLE space_receipts (
    space TEXT NOT NULL,
    identity TEXT COLLATE "C" NOT NULL,
    delivered NUMERIC(20,0) NOT NULL DEFAULT 0
        CHECK (delivered >= 0 AND delivered <= 18446744073709551615),
    read NUMERIC(20,0) NOT NULL DEFAULT 0
        CHECK (read >= 0 AND read <= 18446744073709551615 AND read <= delivered),
    PRIMARY KEY(space,identity)
);
