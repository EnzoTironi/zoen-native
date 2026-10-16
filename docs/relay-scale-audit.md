# Relay scale audit after receipt batching (#101/#102)

Scope: `crates/zoen-relay` and membership lookup. Revoked-device logic belongs to #106 and is unchanged by this work. Measurements below are short laptop microbenchmarks, not production capacity claims.

## Fixed

| Hot path | Before | After |
| --- | --- | --- |
| NATS presence lookup | `join_all` starts a request/inbox for every remote recipient | Ordered buffered stream, at most 32 requests per lookup |
| Away-device push | One detached task per accepted message; slow providers accumulate tasks and cloned audiences | Per-Pusher admission cap of 32, acquired before cloning/spawning; no waiting-task queue; best-effort wake dropped on overload |
| Full local/NATS queue | Clone/encode before discovering queue full | Reserve first; no payload clone/encoding for rejected delivery |
| Co-member lookup | Serial per-space FDB reads and quadratic `Vec::contains` deduplication | At most 16 concurrent per-space reads and HashSet dedup, preserving first-seen order |
| Link-box expiry | Unindexed DELETE of every expired box on every publication | Expiry index, oldest 128 unlocked rows plus the requested expired ID; live-box and one-time retrieval semantics preserved |

The co-member improvement does **not** reduce the number of FDB transactions. It overlaps independent reads and removes quadratic CPU work. The NATS limit is per lookup; push admission caps concurrent background lookups, not all relay traffic globally.

## Reproducible short benchmarks

Use `CARGO_BUILD_JOBS=2 CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=0` on a shared laptop. Configure the existing FDB client library path for linking. No load generator or long sustained benchmark is necessary.

- `cargo test -p zoen-relay --lib presence_fanout_resource_benchmark -- --nocapture`
  - Outstanding requests for 32 / 256 / 4096 recipients: **32 / 256 / 4096 before**, **32 / 32 / 32 after**. Uses yielding mock requests; asserts equal ordered membership output. Not a NATS latency benchmark.
- `cargo test -p zoen-relay --lib wake_admission_resource_benchmark -- --nocapture`
  - 4096 attempted jobs: modeled unrestricted baseline **4096 jobs**, actual admission **32 jobs**. Test fixture audience bytes **4 MiB versus 32 KiB**; this is not RSS. Verifies completion/cancellation release and per-Pusher isolation.
- `cargo test -p zoen-relay --lib co_member_dedup_tests::before_after -- --ignored --nocapture`
  - Unoptimized test build, 20,000 memberships / 10,000 unique: **1.044009541 s before, 10.805084 ms after**; asserts identical output. Timing is informational, never a flaky pass threshold.
- `psql "$ZOEN_TEST_PG" -X -v ON_ERROR_STOP=1 -f crates/zoen-relay/tests/link_box_cleanup_bench.sql`
  - PostgreSQL 16, 100,000 expired + one live row, restored session-local temporary fixture: **12.589 ms before, 0.555 ms after**. Top-level local buffer hits: **100,736 before, 781 after** (plus one read/write after).
  - Work per publication changes deliberately: before deletes all 100,000 expired rows; after deletes 128 plus the requested ID. This is bounded request latency, **not** equivalent whole-backlog cleanup throughput. Both statements run against an indexed fixture, making the baseline conservative. Assertions cover live preservation, requested-ID reuse, row bound and rollback.

## Validation

Passed: `cargo fmt --all --check`, `git diff --check`, and `cargo clippy -p zoen-relay --all-targets -- -D warnings`. The relay library suite passed: 55 tests, zero failed, one explicit benchmark ignored (then run separately and passed). Includes slow-mailbox kick/delivery, bounded presence and push admission regressions. `link_box_cleanup` and `receipt_watermarks` PostgreSQL integrations passed. The final complete FDB contract run passed all 11 scenarios, including co-member lookup across the 16-space boundary. Two earlier runs failed the existing two-storage-client concurrent append assertion; diagnostic reproduction returned retryable `log store unavailable`. A comparison using main's unmodified FDB source/test passed, followed by the final patched run passing. The contention-sensitive failure's exact underlying FDB error is not established; no assertion or retry policy was weakened. See the PR for final CI results.

## Remaining findings

Tracked in [#110](https://github.com/EnzoTironi/zoen-native/issues/110):

1. Websocket mailboxes (1024 frames) and NATS outbound queue (65,536 frames) are count-bounded, not byte-bounded. Local delivery still deep-clones frames for successful enqueue; sharing encoded payloads needs delivery-path design work.
2. NATS queue overflow counts drops but does not kick remote clients or request resync. A missed final durable event may remain unseen until the next Sync/reconnect.
3. Receipt pending/inflight maps lack a cardinality/byte cap under persistent PostgreSQL failure. Preserve monotonic receipt correctness when adding overload handling.
4. Co-member lookup still materializes memberships and performs one transaction per space; a paginated/batched directory API would address transaction count and total result memory.

Existing websocket writers have a five-second write timeout and full local mailboxes kick clients to reconnect and Sync. SQL profile/push/receipt reads inspected in this audit already batch with `ANY` and matching key prefixes; no speculative indexes were added there. The confirmed missing expiry index is addressed above.
