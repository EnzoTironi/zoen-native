# ADR 0023: Per-Space sequencing in memory, batched commits to FoundationDB

Status: accepted (S9)

## Context
ADR 0022 measured the append path: about half a dozen sequential FoundationDB reads per
message (dedupe, head, meta, author role, target role, seen entry, invite) plus a range read of
every member for the audience, all in one transaction per envelope. Two writers on one Space
conflict on its head and retry. The cost showed up twice. Relay CPU was 0.58 ms per message,
mostly spent waiting on and decoding those reads. And a hot Space tipped into queueing: in
406515f, sixteen people sending into one Space top out near 440 msgs/s and then collapse into
conflict retries (0.28 retries per append, latency in seconds).

## Decision
The relay that owns a Space (ADR 0018) orders its appends in memory and commits them in
batches. `LogStore` is unchanged; this lives inside `FdbLog`.

- **One queue and one worker per active Space** (`log/sequencer.rs`). `append` puts the
  envelope on the Space's queue and waits for its result. The worker takes everything queued,
  up to 64 envelopes or 1 MiB, and commits it as one transaction. While one batch commits, the
  next fills (group commit), so appends per commit grow with load and latency stays at about
  one commit. Queue order is commit order. A session waits for each publish before the next,
  so its own envelopes keep their order. A worker exits after 30 s idle, dropping its state, so
  memory follows active Spaces.
- **A head-validated cache** (`SpaceState`: kind, members, the hashes of the last 1,024
  entries). Every write under a Space also writes its head, so `(seq, hash)` names the whole
  Space state. Each batch reads the head *without* snapshot (the fence), in the same round trip
  as every dedupe and invite key it needs. If the head equals the cache's, admission needs no
  further reads. Otherwise (first use, another relay wrote, a retry) the state is reloaded with
  snapshot reads, which are safe behind the fence. Seen hashes older than the cache are fetched
  together, once per batch.
- **Admission in order on a working copy.** Each envelope is checked against the state as left
  by the ones before it in the batch: a member added earlier in the batch can post later in it,
  a second copy of an envelope answers `Duplicate` with the first's entry, an invite's uses
  count across the batch. Each entry, dedupe key and membership change is written; the head is
  written once. The audience comes from the cached members.
- **The cache survives only a committed batch.** It moves into the first attempt of the
  transaction and comes back only with a committed result. A retry, a conflict or an unknown
  commit result reloads from the head the next attempt reads, and a commit that actually
  landed answers `Duplicate` through the dedupe keys, as before.
- Two relays writing one Space (ownership moving, or a stale owner) stay correct: the fence
  conflicts or the head differs, and the cache reloads. Ownership keeps that rare, it doesn't
  make it unsafe.
- Observability: `fdb.append` stays a child of `publish` and covers queue wait plus commit,
  with `batch` (envelopes in the transaction) and `attempts`.
  `zoen_relay_append_batches_total` against `zoen_relay_events_sequenced_total` gives appends
  per transaction.

## Proof
- `log_store.rs` (real FoundationDB) adds two contracts: *two relays on one space share one
  chain and one membership* (8 writers split across two `FdbLog`s build one gapless chain, then
  a member added through one relay can post through the other's warm cache, and a removal
  through the second binds the first); *duplicates in one batch get the one stored copy* (16
  copies meet in one batch every time on a single-threaded runtime: one is sequenced, all answer
  with it).
- `sequencer::tests`: queue order is commit order, batches cap at 64, every caller gets its own
  result, Spaces get their own state, idle workers leave and a new one starts fresh.
- Every existing contract test and journey passes unchanged, including `journey_telemetry`.

## Results (same harness, same box, 2026-10-08)
`scripts/bench-load.sh` as in ADR 0022, plus three hot-Space scenarios. The hot-Space ones lift
the per-device and per-account publish limits (20/s and 40/s), as the sweep already lifts
the per-address ones. Otherwise sixteen people are capped at 320/s by the limiter and the
scenario measures S6, not the log. The 406515f relay ran the same scenarios, with the same
harness, through `RELAY_BIN`. "Before" for the other scenarios is ADR 0022's final sweep.

| scenario | offered | before (406515f) | after (S9) |
|---|---|---|---|
| rate-250 | 250/s | all; p50 3.9, p99 8.2 ms; relay 0.43, FDB 0.27 cores | all; 3.0 / 9.5 ms; 0.25 / 0.19 |
| rate-500 | 500/s | all; 4.0 / 10.9 ms; 0.77 / 0.49 | all; 3.7 / 14.5 ms; 0.44 / 0.34 |
| rate-1000 | 1,000/s | all; 3.7 / 14.4 ms; 1.18 / 0.68 | all; 3.7 / 14.0 ms; 0.74 / 0.47 |
| rate-2000 | 2,000/s | all; 5.6 / 26.9 ms; 1.79 / 0.85 | all; 5.4 / 15.9 ms; 1.17 / 0.61 |
| rate-4000 | 4,000/s | **saturated at 2,490/s**, seconds of queueing; 2.59 / 1.00 | **all 4,000/s**; 48 / 324 ms; 1.77 / 0.64 |
| fanout-2 | 500/s | all; 3.3 / 13.4 ms; 0.55 / 0.45 | all; 4.0 / 20.0 ms; 0.35 / 0.35 |
| fanout-32 | 500/s | all; 5.2 / 20.7 ms; 0.99 / 0.42 | all; 7.2 / 37.6 ms; 0.66 / 0.28 |
| fanout-128 | 500/s | **saturated at 175/s** | **all 63,500 deliveries/s**; 10.2 / 39.2 ms; 1.62 / 0.26 |
| conns-10k | 200/s | all; 4.4 / 160.8 ms; 0.32 / 0.25 | all; 4.0 / 11.1 ms; 0.21 / 0.17 |
| hot-space-1000 | 1,000/s | **441/s**, p50 12.9 s; 0.28 retries per append | **all**; 3.1 / 10.4 ms; 0 retries; 1.4 appends per transaction |
| hot-space-2000 | 2,000/s | **291/s**, p50 21.6 s | **all**; 5.1 / 25.0 ms; 3.9 appends per transaction |
| hot-space-4000 | 4,000/s | stalls (13,790 sequenced, 3,410 retries, then no progress) | **3,182/s** (ceiling); 8.0 appends per transaction; 0 retries |
| two-nodes-1000 | 1,000/s | all; 7.9 / 54.3 ms; 1.08 (two relays) / 0.59 | all; 5.0 / 37.4 ms; 0.84 / 0.47 |

Low-load p99 moves with the shared box (load average 2 to 16 during these runs), so the two
fan-out scenarios were rerun back to back on both builds (`recheck/` in the proof):

| scenario | 406515f p50 / p99, relay / FDB cores | S9 |
|---|---|---|
| fanout-2 | 3.8 / 10.9 ms; 0.56 / 0.46 | 2.8 / 6.4 ms; 0.40 / 0.37 |
| fanout-32 | 3.5 / 12.3 ms; 1.13 / 0.47 | 3.4 / 17.2 ms; 0.91 / 0.36 |

fanout-32's p99 is about 5 ms higher on S9, with p50 unchanged. An append now crosses
to its Space's worker task, and on a runtime busy writing 15,500 deliveries/s that hop
sometimes waits behind them. This is a tail cost, kept on watch. The fix, if it matters, is to
run the sequencer workers on their own small runtime.

What changed, per message, at the same offered load: relay CPU fell by 35 to 45% and
FoundationDB CPU by 25 to 40%. That comes from the cache: the reads are gone even where
batches stay near one envelope (2,000 people in groups of 8 are 250 quiet Spaces). In a hot
Space the batching does the rest: 0 conflict retries, and commits carry up to 8 envelopes at
the ceiling.

### What limits a hot Space now
A session has one publish in flight, so a batch can't hold more envelopes than people
sending at that moment. Sixteen senders cap a batch at 16, and the Space at sixteen per commit
round trip, about 3,200 msgs/s on this box. Real hot Spaces have many more senders, each much
slower (the publish limit is 20/s per device), so batches fill from breadth, not from a few
fast clients. Past that, delivery dominates: 3,200 msgs/s into 16 people is 48,000 deliveries/s,
the same fan-out work as fanout-128 above.

### Harness limit found
Against a relay overloaded more than 10× (406515f at 4,000/s into one Space), `zoen-load`
deadlocks with it. The client stops reading deliveries while it is blocked sending a publish
the relay isn't reading. The run is stopped and reported as a stall. Only the old code reaches
that state, so no S9 number depends on it. Follow-up: give each simulated person a separate
writer task.

## Consequences
- The capacity units in ADR 0022 improve. Fitted to rate-2000 and fanout-32 (S9): about
  **0.37 ms of relay CPU per message + 30 µs per delivery**, against 0.58 ms + 45 µs. And
  **3,300 appends per FoundationDB core** at rate-2000 (6,250 at rate-4000, where batches
  grow), against 2,500, all roles colocated. `plan-real.md` carries the new units.
- The 1B estimate of ADR 0022 moves by about $17k a month. Relay CPU: 700k × 0.37 ms + 3.5M
  × 30 µs = 364 cores, +50% = 550 vCPU, so $18k instead of $28k. FoundationDB compute: 700k ÷
  3,300 × 3 = 640 vCPU, so $21k instead of $28k. Storage and egress dominate and don't
  change, so the total stays about $450k.
- Memory: one `SpaceState` per active Space, with members (identity strings) and 1,024 hashes,
  about 100 KB at worst. Idle Spaces cost nothing.
- Envelopes now wait in memory on the owner. A relay crash loses queued envelopes that never
  committed. Their senders get no ack and resend the same envelope from the outbox, and dedupe
  makes that safe. Nothing that was acked is ever lost.
- The trait boundary held: Postgres-free, FDB-only change, no protocol or client change.

## At 1B users
700k messages/s at peak spread over hundreds of millions of Spaces: almost all are quiet, and
for them S9 is the cache (no reads per append, a third less CPU on both tiers). The few
celebrity or event Spaces with thousands of active senders are where batching pays: one
owner commits up to 64 envelopes per round trip, a few thousand per second per Space, with
zero conflicts. Beyond that a single log is the wrong shape anyway. Such Spaces become broadcast
channels: few writers, many readers, delivery-bound, which ADR 0009 (fan-out by logs, not
inboxes) already covers. Per-Space state is bounded by active Spaces per owner. 160M connections
over ~1,000 relays is ~160k active Spaces per relay at most, about 1–2 GB of cache at the
worst case, inside the RAM those nodes already carry for connections.
