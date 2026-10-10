-- FDB proves non-admission; SQL prevents a reservation arriving after cleanup.
-- A retained native run already references an installed original policy/period.
CREATE TABLE runtime_predispatch_closures (
    attempt TEXT PRIMARY KEY CHECK (length(attempt) BETWEEN 1 AND 128),
    owner TEXT NOT NULL,
    period_start BIGINT NOT NULL,
    binding_digest TEXT NOT NULL CHECK (binding_digest ~ '^[0-9a-f]{64}$'),
    deployment BOOLEAN NOT NULL DEFAULT TRUE CHECK (deployment),
    FOREIGN KEY (owner, period_start) REFERENCES runtime_periods(owner, period_start),
    FOREIGN KEY (deployment) REFERENCES runtime_deployment_binding(singleton)
);
CREATE TRIGGER immutable_closure BEFORE UPDATE OR DELETE ON runtime_predispatch_closures
    FOR EACH ROW EXECUTE FUNCTION runtime_history_immutable();
CREATE TRIGGER immutable_closure_truncate BEFORE TRUNCATE ON runtime_predispatch_closures
    FOR EACH STATEMENT EXECUTE FUNCTION runtime_history_immutable();

CREATE FUNCTION runtime_before_closure() RETURNS TRIGGER LANGUAGE plpgsql AS $$
DECLARE a RECORD;
BEGIN
    PERFORM 1 FROM runtime_periods
      WHERE owner=NEW.owner AND period_start=NEW.period_start FOR UPDATE;
    IF NOT FOUND THEN
      RAISE EXCEPTION 'runtime closure lacks original period' USING ERRCODE='23514';
    END IF;
    SELECT owner,period_start,binding_digest,deployment INTO a
      FROM runtime_attempts WHERE attempt=NEW.attempt;
    IF FOUND AND (a.owner,a.period_start,a.binding_digest,a.deployment)
      IS DISTINCT FROM (NEW.owner,NEW.period_start,NEW.binding_digest,NEW.deployment) THEN
      RAISE EXCEPTION 'runtime closure binding mismatch' USING ERRCODE='23514';
    END IF;
    IF EXISTS(SELECT 1 FROM runtime_admission_witnesses WHERE attempt=NEW.attempt)
      OR EXISTS(SELECT 1 FROM runtime_dispositions WHERE attempt=NEW.attempt AND kind='settle') THEN
      RAISE EXCEPTION 'runtime admitted obligation cannot close' USING ERRCODE='23514';
    END IF;
    RETURN NEW;
END;
$$;
CREATE TRIGGER check_closure BEFORE INSERT ON runtime_predispatch_closures
    FOR EACH ROW EXECUTE FUNCTION runtime_before_closure();

CREATE FUNCTION runtime_before_attempt_or_claim() RETURNS TRIGGER LANGUAGE plpgsql AS $$
DECLARE o TEXT; s BIGINT;
BEGIN
    IF TG_TABLE_NAME='runtime_attempts' THEN
      o := NEW.owner; s := NEW.period_start;
    ELSE
      SELECT owner,period_start INTO STRICT o,s FROM runtime_attempts WHERE attempt=NEW.attempt;
    END IF;
    PERFORM 1 FROM runtime_periods WHERE owner=o AND period_start=s FOR UPDATE;
    IF EXISTS(SELECT 1 FROM runtime_predispatch_closures WHERE attempt=NEW.attempt) THEN
      RAISE EXCEPTION 'runtime attempt permanently closed' USING ERRCODE='23514';
    END IF;
    RETURN NEW;
END;
$$;
CREATE TRIGGER check_attempt_closure BEFORE INSERT ON runtime_attempts
    FOR EACH ROW EXECUTE FUNCTION runtime_before_attempt_or_claim();
CREATE TRIGGER check_claim_closure BEFORE INSERT ON runtime_claims
    FOR EACH ROW EXECUTE FUNCTION runtime_before_attempt_or_claim();

CREATE FUNCTION runtime_before_admission_or_settle() RETURNS TRIGGER LANGUAGE plpgsql AS $$
DECLARE o TEXT; s BIGINT;
BEGIN
    IF TG_TABLE_NAME='runtime_dispositions' THEN
      IF NEW.kind='release' THEN
        RETURN NEW;
      END IF;
    END IF;
    SELECT owner,period_start INTO STRICT o,s FROM runtime_attempts WHERE attempt=NEW.attempt;
    -- DirectoryGuard already holds this lock until its known witness commit.
    PERFORM 1 FROM runtime_periods WHERE owner=o AND period_start=s FOR SHARE;
    IF EXISTS(SELECT 1 FROM runtime_predispatch_closures WHERE attempt=NEW.attempt) THEN
      RAISE EXCEPTION 'runtime closed attempt cannot admit or settle' USING ERRCODE='23514';
    END IF;
    RETURN NEW;
END;
$$;
CREATE TRIGGER check_admission_closure BEFORE INSERT ON runtime_admission_witnesses
    FOR EACH ROW EXECUTE FUNCTION runtime_before_admission_or_settle();
CREATE TRIGGER check_settle_closure BEFORE INSERT ON runtime_dispositions
    FOR EACH ROW EXECUTE FUNCTION runtime_before_admission_or_settle();
