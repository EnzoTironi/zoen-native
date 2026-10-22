# FDB two-client investigation (#111)

The original failure remains open: two independent storage clients of one owner
occasionally returned retryable `log store unavailable` during eight concurrent
writers. A successful run does not attribute or repair that historical rejection.

The 2026-10-22 disposable FoundationDB 7.3.71 baseline ran the unchanged eight
writers × forty messages and the gapless/hash-linked 321-event chain, then checked
both warm caches against member addition/removal. It passed in 2.034 seconds.
No retained database, default cluster, PG, NATS, relay service or provider was used.

The opt-in pressure case synchronizes the same eight writers halfway through;
two independent owner clients each perform four real lease renewals during the
remaining writes. It has a sixty-second fixture cap. Production 5-retry/2-second
transaction limits, fifteen-second lease TTL, routing retries and all chain and
membership assertions are unchanged. This run passed in 2.022 seconds, but its
collector recorded two failed sequencing batches: FDB code **1020**, sizes three
and one. The existing outer append retry recovered; no retryable rejection escaped
to this caller. Code 1020 identifies a transaction conflict in the
[pinned FDB definitions](https://github.com/apple/foundationdb/blob/7.3.71/flow/include/flow/error_definitions.h#L46).
It does not distinguish which conflict key caused these two batches or establish
the cause of the historical public failure. Retry attempt counts for these
individual failures were not measured.

The test main now installs its error collector. Ownership resolution also records
only a closed phase (`maintenance`, `lease_read`, `lease_refresh`), numeric FDB
code and `fdb`/`other` category before returning the identical retryable public
rejection. Unknown non-FDB errors use code zero. Error strings, user content,
keys and identities are excluded from those new events. A synthetic writer test
asserts the actual emitted metadata, excludes private/error canaries and checks
that the public rejection remains exactly unchanged.

The ordinary custom test main still executes its original eleven cases in the
same order. The pressure scenario is opt-in; it is not an added timing-sensitive
CI gate. For an **explicitly configured disposable cluster**, use:

```sh
cargo test -p zoen-relay --test log_store -- --two-client
cargo test -p zoen-relay --test log_store -- --two-client-renewals
```

Do not run these commands against retained or production data. Every scenario
creates a random test cell and removes it after a successful contract; a panic
may retain that synthetic cell. The isolated reproduction wrapper separately
stops and reaps only its own server/test process groups. Two preliminary fixture
attempts failed on CLI log-size syntax before any contract ran; both failures
remain recorded, with the same cleanup and unchanged timeouts.

MLS epoch/ciphertext/checkpoint tests moved as a coherent family into
`tests/log_store/mls_contracts.rs`; their bodies/assertions and the original
case sequence are source-compared, not claimed re-executed here. The focused
ownership tests and strict all-target relay Clippy pass; the full eleven-case,
ownership-process/failover and production/load suites were not run in this batch.
#111 needs an observed escaped rejection with a phase/root cause before a
correctness fix or closure is justified. No retry/backoff change is proposed.
