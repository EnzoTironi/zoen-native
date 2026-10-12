-- Postgres owns financial authority (ADR0008). FDB owns dispatch/tombstones.
CREATE TABLE runtime_periods (
    owner TEXT NOT NULL REFERENCES identities(id),
    period_start BIGINT NOT NULL,
    period_end BIGINT NOT NULL CHECK (period_end > period_start),
    currency TEXT NOT NULL CHECK (currency = 'USD'),
    scale SMALLINT NOT NULL CHECK (scale = 6),
    held_units BIGINT NOT NULL DEFAULT 0 CHECK (held_units >= 0),
    spent_units BIGINT NOT NULL DEFAULT 0 CHECK (spent_units >= 0),
    PRIMARY KEY (owner, period_start)
);
CREATE TABLE runtime_policies (
    owner TEXT NOT NULL,
    period_start BIGINT NOT NULL,
    version BIGINT NOT NULL CHECK (version > 0),
    digest TEXT NOT NULL UNIQUE CHECK (length(digest) = 64),
    previous_digest TEXT,
    signed JSONB NOT NULL CHECK (octet_length(signed::TEXT) <= 8192),
    PRIMARY KEY (owner, period_start, version),
    FOREIGN KEY (owner, period_start) REFERENCES runtime_periods(owner, period_start)
);
CREATE TABLE runtime_attempts (
    attempt TEXT PRIMARY KEY CHECK (length(attempt) BETWEEN 1 AND 128),
    owner TEXT NOT NULL,
    period_start BIGINT NOT NULL,
    binding_digest TEXT NOT NULL CHECK (length(binding_digest) = 64),
    binding JSONB NOT NULL CHECK (octet_length(binding::TEXT) <= 8192),
    hold_units BIGINT NOT NULL CHECK (hold_units >= 0),
    FOREIGN KEY (owner, period_start) REFERENCES runtime_periods(owner, period_start)
);
CREATE TABLE runtime_claims (
    attempt TEXT PRIMARY KEY REFERENCES runtime_attempts(attempt),
    nonce TEXT NOT NULL UNIQUE CHECK (length(nonce) = 64)
);
CREATE TABLE runtime_admission_witnesses (
    attempt TEXT PRIMARY KEY REFERENCES runtime_claims(attempt),
    nonce TEXT NOT NULL CHECK (length(nonce) = 64)
);
CREATE TABLE runtime_evidence (
    attempt TEXT NOT NULL REFERENCES runtime_claims(attempt),
    digest TEXT NOT NULL CHECK (length(digest) = 64),
    sealed BYTEA NOT NULL CHECK (octet_length(sealed) BETWEEN 40 AND 98304),
    PRIMARY KEY (attempt, digest)
);
CREATE TABLE runtime_dispositions (
    attempt TEXT PRIMARY KEY REFERENCES runtime_attempts(attempt),
    kind TEXT NOT NULL CHECK (kind IN ('release','settle')),
    amount_units BIGINT NOT NULL CHECK (amount_units >= 0),
    source_digest TEXT NOT NULL CHECK (length(source_digest) = 64),
    CHECK (kind <> 'release' OR amount_units = 0)
);
CREATE TABLE runtime_journals (
    attempt TEXT NOT NULL REFERENCES runtime_attempts(attempt),
    kind TEXT NOT NULL CHECK (kind IN ('reserve','release','settle')),
    PRIMARY KEY (attempt, kind)
);
CREATE TABLE runtime_postings (
    attempt TEXT NOT NULL,
    kind TEXT NOT NULL,
    account TEXT NOT NULL CHECK (account IN ('held','clearing','expense','payable')),
    units BIGINT NOT NULL,
    PRIMARY KEY (attempt, kind, account),
    FOREIGN KEY (attempt, kind) REFERENCES runtime_journals(attempt, kind)
);
CREATE INDEX runtime_attempt_period ON runtime_attempts(owner, period_start);
CREATE TABLE runtime_financial_outbox (
    attempt TEXT NOT NULL REFERENCES runtime_attempts(attempt),
    kind TEXT NOT NULL CHECK (kind IN ('evidence','release','settle')),
    digest TEXT NOT NULL CHECK (length(digest) = 64),
    PRIMARY KEY (attempt, kind, digest)
);
CREATE TABLE runtime_financial_acks (
    attempt TEXT NOT NULL,
    kind TEXT NOT NULL,
    digest TEXT NOT NULL,
    PRIMARY KEY (attempt, kind, digest),
    FOREIGN KEY (attempt, kind, digest) REFERENCES runtime_financial_outbox(attempt, kind, digest)
);

CREATE FUNCTION runtime_history_immutable() RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    RAISE EXCEPTION 'runtime financial history is immutable' USING ERRCODE = '23514';
END;
$$;
DO $$ DECLARE t TEXT; BEGIN
    FOREACH t IN ARRAY ARRAY['runtime_policies','runtime_attempts','runtime_claims',
      'runtime_admission_witnesses','runtime_evidence','runtime_dispositions',
      'runtime_journals','runtime_postings','runtime_financial_outbox','runtime_financial_acks']
    LOOP
      EXECUTE format('CREATE TRIGGER immutable_history BEFORE UPDATE OR DELETE ON %I FOR EACH ROW EXECUTE FUNCTION runtime_history_immutable()', t);
    END LOOP;
END $$;

CREATE FUNCTION runtime_period_fixed() RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP = 'DELETE' OR (NEW.owner,NEW.period_start,NEW.period_end,NEW.currency,NEW.scale)
        IS DISTINCT FROM (OLD.owner,OLD.period_start,OLD.period_end,OLD.currency,OLD.scale) THEN
      RAISE EXCEPTION 'runtime period is immutable' USING ERRCODE = '23514';
    END IF;
    RETURN NEW;
END;
$$;
CREATE TRIGGER fixed_period BEFORE UPDATE OR DELETE ON runtime_periods
    FOR EACH ROW EXECUTE FUNCTION runtime_period_fixed();

-- Each reservation must have precisely its hold journal. A terminal disposition
-- is exclusive and must have precisely its reversal and supported full expense.
CREATE FUNCTION runtime_check_attempt() RETURNS TRIGGER LANGUAGE plpgsql AS $$
DECLARE a TEXT := NEW.attempt; h BIGINT; k TEXT; c BIGINT; n BIGINT; v NUMERIC;
BEGIN
    SELECT hold_units INTO STRICT h FROM runtime_attempts WHERE attempt = a;
    SELECT kind,amount_units INTO k,c FROM runtime_dispositions WHERE attempt = a;
    IF NOT EXISTS (SELECT 1 FROM runtime_journals WHERE attempt = a AND kind = 'reserve') THEN
      RAISE EXCEPTION 'runtime reservation lacks hold journal' USING ERRCODE = '23514';
    END IF;
    IF k IS NOT NULL AND NOT EXISTS (SELECT 1 FROM runtime_journals WHERE attempt = a AND kind = k) THEN
      RAISE EXCEPTION 'runtime disposition lacks journal' USING ERRCODE = '23514';
    END IF;
    FOR k IN SELECT kind FROM runtime_journals WHERE attempt = a LOOP
      IF k <> 'reserve' AND NOT EXISTS (SELECT 1 FROM runtime_dispositions WHERE attempt=a AND kind=k) THEN
        RAISE EXCEPTION 'runtime journal lacks matching disposition' USING ERRCODE = '23514';
      END IF;
      SELECT count(*),sum(units::NUMERIC) INTO n,v FROM runtime_postings WHERE attempt=a AND kind=k;
      IF n <> (CASE WHEN k='settle' THEN 4 ELSE 2 END) OR v <> 0 THEN
        RAISE EXCEPTION 'runtime journal must balance with exact shape' USING ERRCODE = '23514';
      END IF;
      IF EXISTS (SELECT 1 FROM runtime_postings WHERE attempt=a AND kind=k AND units::NUMERIC <>
        CASE account
          WHEN 'held' THEN CASE WHEN k='reserve' THEN h::NUMERIC ELSE -h::NUMERIC END
          WHEN 'clearing' THEN CASE WHEN k='reserve' THEN -h::NUMERIC ELSE h::NUMERIC END
          WHEN 'expense' THEN c::NUMERIC
          WHEN 'payable' THEN -c::NUMERIC END)
        OR NOT EXISTS (SELECT 1 FROM runtime_postings WHERE attempt=a AND kind=k AND account='held')
        OR NOT EXISTS (SELECT 1 FROM runtime_postings WHERE attempt=a AND kind=k AND account='clearing')
        OR (k='settle' AND NOT EXISTS (SELECT 1 FROM runtime_postings WHERE attempt=a AND kind=k AND account='expense'))
        OR (k='settle' AND NOT EXISTS (SELECT 1 FROM runtime_postings WHERE attempt=a AND kind=k AND account='payable'))
      THEN
        RAISE EXCEPTION 'runtime posting amount or account invalid' USING ERRCODE = '23514';
      END IF;
    END LOOP;
    RETURN NULL;
END;
$$;
CREATE CONSTRAINT TRIGGER check_attempt AFTER INSERT ON runtime_attempts
    DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION runtime_check_attempt();
CREATE CONSTRAINT TRIGGER check_disposition AFTER INSERT ON runtime_dispositions
    DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION runtime_check_attempt();
CREATE CONSTRAINT TRIGGER check_journal AFTER INSERT ON runtime_journals
    DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION runtime_check_attempt();
CREATE CONSTRAINT TRIGGER check_posting AFTER INSERT ON runtime_postings
    DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION runtime_check_attempt();

CREATE FUNCTION runtime_check_period() RETURNS TRIGGER LANGUAGE plpgsql AS $$
DECLARE o TEXT; s BIGINT; h BIGINT; c BIGINT; ph NUMERIC; pc NUMERIC;
BEGIN
    IF TG_TABLE_NAME = 'runtime_postings' THEN
      SELECT owner,period_start INTO o,s FROM runtime_attempts WHERE attempt=NEW.attempt;
    ELSE o := NEW.owner; s := NEW.period_start;
    END IF;
    SELECT held_units,spent_units INTO STRICT h,c FROM runtime_periods WHERE owner=o AND period_start=s;
    SELECT coalesce(sum(p.units::NUMERIC) FILTER (WHERE p.account='held'),0),
           coalesce(sum(p.units::NUMERIC) FILTER (WHERE p.account='expense'),0)
      INTO ph,pc FROM runtime_postings p JOIN runtime_attempts a USING (attempt)
      WHERE a.owner=o AND a.period_start=s;
    IF h::NUMERIC <> ph OR c::NUMERIC <> pc THEN
      RAISE EXCEPTION 'runtime aggregate differs from immutable postings' USING ERRCODE = '23514';
    END IF;
    RETURN NULL;
END;
$$;
CREATE CONSTRAINT TRIGGER check_period AFTER INSERT OR UPDATE ON runtime_periods
    DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION runtime_check_period();
CREATE CONSTRAINT TRIGGER check_posting_period AFTER INSERT ON runtime_postings
    DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION runtime_check_period();

CREATE FUNCTION runtime_check_reference() RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    IF TG_TABLE_NAME = 'runtime_admission_witnesses' THEN
      IF NOT EXISTS (SELECT 1 FROM runtime_claims WHERE attempt=NEW.attempt AND nonce=NEW.nonce) THEN
        RAISE EXCEPTION 'runtime admission witness mismatch' USING ERRCODE='23514';
      END IF;
    ELSIF NEW.kind = 'evidence' THEN
      IF NOT EXISTS (SELECT 1 FROM runtime_evidence WHERE attempt=NEW.attempt AND digest=NEW.digest) THEN
        RAISE EXCEPTION 'runtime evidence reference mismatch' USING ERRCODE='23514';
      END IF;
    ELSIF NOT EXISTS (SELECT 1 FROM runtime_dispositions WHERE attempt=NEW.attempt AND kind=NEW.kind AND source_digest=NEW.digest) THEN
      RAISE EXCEPTION 'runtime disposition reference mismatch' USING ERRCODE='23514';
    END IF;
    RETURN NULL;
END;
$$;
CREATE CONSTRAINT TRIGGER check_reference AFTER INSERT ON runtime_admission_witnesses
    DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION runtime_check_reference();
CREATE CONSTRAINT TRIGGER check_outbox_reference AFTER INSERT ON runtime_financial_outbox
    DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION runtime_check_reference();
