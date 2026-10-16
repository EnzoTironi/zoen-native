-- psql "$ZOEN_TEST_PG" -X -v ON_ERROR_STOP=1 -f crates/zoen-relay/tests/link_box_cleanup_bench.sql
-- Session-local temporary table only; both measured DELETEs are rolled back.
\set ON_ERROR_STOP on
\timing on
BEGIN;
SET LOCAL statement_timeout = '30s';
SET LOCAL lock_timeout = '2s';
SET LOCAL search_path = pg_temp;
CREATE TEMP TABLE link_boxes (
    id text PRIMARY KEY,
    sealed bytea NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now()
) ON COMMIT DROP;
CREATE INDEX link_boxes_created_at_bench_idx ON link_boxes (created_at);
INSERT INTO link_boxes (id, sealed, created_at)
SELECT 'expired-' || lpad(i::text, 6, '0'), decode('00', 'hex'),
    now() - interval '16 minutes' - i * interval '1 millisecond'
FROM generate_series(1, 100000) AS g(i);
INSERT INTO link_boxes (id, sealed, created_at) VALUES ('live', decode('00', 'hex'), now());
ANALYZE link_boxes;
\echo 'BEFORE: unbounded cleanup, 100000 expired rows'
SAVEPOINT before_cleanup;
EXPLAIN (ANALYZE, BUFFERS, TIMING OFF)
DELETE FROM link_boxes WHERE created_at < now() - interval '15 minutes';
DO $$ BEGIN
    IF (SELECT count(*) FROM link_boxes) <> 1
       OR NOT EXISTS (SELECT 1 FROM link_boxes WHERE id = 'live') THEN
        RAISE EXCEPTION 'baseline cleanup regression';
    END IF;
END $$;
ROLLBACK TO SAVEPOINT before_cleanup;
RELEASE SAVEPOINT before_cleanup;
\echo 'AFTER: bounded cleanup on restored fixture; requested id outside oldest batch'
SAVEPOINT after_cleanup;
EXPLAIN (ANALYZE, BUFFERS, TIMING OFF)
WITH expired AS (
    SELECT id FROM link_boxes
    WHERE created_at < now() - interval '15 minutes'
    ORDER BY created_at LIMIT 128 FOR UPDATE SKIP LOCKED
), candidates AS (
    SELECT id FROM expired
    UNION
    SELECT id FROM link_boxes
    WHERE id = 'expired-000001' AND created_at < now() - interval '15 minutes'
)
DELETE FROM link_boxes b USING candidates c WHERE b.id = c.id;
DO $$ BEGIN
    IF (SELECT count(*) FROM link_boxes) <> 99872 THEN
        RAISE EXCEPTION 'expected exactly 129 deleted rows';
    END IF;
    IF EXISTS (SELECT 1 FROM link_boxes WHERE id = 'expired-000001') THEN
        RAISE EXCEPTION 'requested expired ID not cleaned';
    END IF;
    IF NOT EXISTS (SELECT 1 FROM link_boxes WHERE id = 'live') THEN
        RAISE EXCEPTION 'live box deleted';
    END IF;
END $$;
ROLLBACK TO SAVEPOINT after_cleanup;
RELEASE SAVEPOINT after_cleanup;
DO $$ BEGIN
    IF (SELECT count(*) FROM link_boxes) <> 100001 THEN
        RAISE EXCEPTION 'rollback failed to restore fixture';
    END IF;
END $$;
ROLLBACK;
