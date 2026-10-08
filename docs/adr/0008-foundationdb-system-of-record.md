# ADR 0008: FoundationDB is the system of record for logs and sync state

Status: accepted. Supersedes D9 in repensado.md ("Postgres partitioned by Space now,
FoundationDB past about 30k writes/s"). Supersedes the Postgres event tables of ADR 0001.

## Context
D9 planned a migration to FoundationDB once Postgres ran out. The 1B model needs 700k
appends/s at peak and 600 TB hot; a migration of the log store under live traffic is the
most expensive change a messaging system can make. Doing it now costs a week; doing it later
costs a quarter and a risky cutover.

## Decision
- FoundationDB 7.3 (Rust `foundationdb` crate) holds per-Space logs, heads, idempotency keys,
  the membership index, MLS key packages, the outbox to NATS, partition leases and rate
  counters. Keys use the tuple layer under one directory per cell (layout in
  system-design.md). Values stay under 100 KB; transactions under 5 s and 10 MB.
- Ordering: one transaction reads `head/{space}`, writes `log/{space}/{seq+1}`, the new head,
  the dedupe key and a versionstamped outbox entry. FDB's conflict detection serializes
  appends within a Space, so there is no sequencer service. Sequence numbers stay dense per
  Space because clients detect gaps with them and the hash chain needs the predecessor; the
  versionstamp orders the outbox across Spaces.
- Space ownership in sync becomes affinity (batching, membership cache, outbox forwarding),
  not the ordering authority. Two owners racing cause a conflict and a retry, never a fork.
- Postgres stays for accounts, profiles, handles, the catalog, billing, FTS and pgvector.
- A watch on `outbox_tick/{partition}` (atomic add per append) wakes the outbox forwarder, with
  a 250 ms poll as the fallback, because watches see one key, not new keys under a prefix.
- An in-memory `LogStore` exists only in unit tests. Journeys run against a real fdbserver
  (`scripts/fdb.sh up`).

## Implementation (S3, 2026-10-08)
`crates/zoen-relay/src/log/`: `LogStore` (mod.rs), the admission rules as a pure function of
the facts one transaction reads (admission.rs), and `FdbLog` (fdb.rs). Keys, tuple-encoded
under `("zoen", cell)`:

| key | value |
|---|---|
| `("s", space, "meta")` | `(kind, privacy, created_by)` |
| `("s", space, "head")` | `(seq, hash)` |
| `("s", space, "log", seq)` | the sequenced entry, protobuf (`Sequenced`: seq, prev, hash, the author's exact bytes, sig, cert) |
| `("s", space, "dedupe", author, client_id)` | `seq` |
| `("s", space, "m", identity)` | role |
| `("i", identity, space)` | membership by identity (catch-up, presence audience) |
| `("inv", sha256(code))` | `(space, role, created_by, expires_ms, max_uses, uses)` |

One append = one transaction: dedupe check, head, Space kind, the author's and target's
roles, the invite, the causal link's referenced entry; `admit()` decides; the entry, head,
dedupe key and membership change commit together. A commit with an unknown result retries
into the dedupe key and answers `Duplicate`, so a retried envelope is never sequenced twice.
Membership reads that decide admission take conflict ranges; catch-up reads are snapshot
reads. Postgres keeps the directory (identities, handles, devices; migration 0003 drops its
log tables). Cells: `ZOEN_FDB_CELL` selects the subspace; journeys each get their own.
Envelopes are capped at 90 KB so every entry stays under FDB's 100 KB value limit; larger
payloads travel as blobs (ADR 0007).

Measured on the box (8 vCPU shared, one `fdbserver` process, `ssd-2`, release build,
`cargo run --release -p zoen-relay --example log_bench`), every append through admission:

| shape | appends/s | p50 | p99 |
|---|---|---|---|
| 256 Spaces, 64 writers, 20k events | 4,220 | 14.3 ms | 28.7 ms |
| 1 hot Space, 16 writers, 2k events | 261 | 2.0 ms | 543 ms |

Catch-up reads 110k to 228k events/s from one client in pages of 500. The hot-Space row
confirms the per-head ceiling predicted below (a few hundred unbatched appends/s, with
conflict retries in the tail); owner-side batching (S4) is what lifts it. A single
development process is not a capacity number: S8 measures a real cluster.

Two lessons kept in tests: FoundationDB returns range reads in partial batches, so a page
must follow `more` until it is full (`catch_up_returns_every_event_past_partial_batches`);
and its ratekeeper stops admitting writes when a storage disk has less than about 5% free,
which looks like a hang from the client (docs/infra.md).

## Alternatives
- Postgres partitioned by Space (D9): simplest today, a migration later.
- ScyllaDB: no multi-key transactions; ordering would need a sequencer per Space.
- FoundationDB: ordered keys, strict serializability, simulation-tested; no managed offering.

## Operations
No managed FoundationDB exists. Self-hosted with the fdb-kubernetes-operator on nodes with
local NVMe (FoundationDBCluster CRD, backups to object storage via backup agents, daily
restore drills). Cost per cell (5 NVMe nodes of 8 vCPU, 64 GB at about $650 each) is about
$3.3k/month; one cell carries about 70k appends/s, enough for 50M DAU.

## At 1B users
700k appends/s at peak: about 10 cells of 30 storage processes each, 300 NVMe nodes, about
$195k/month on i4i.2xlarge-class nodes (8 vCPU, 64 GB, 1.875 TB NVMe). FDB holds 30 days hot:
50 TB of logs a month, 150 TB triple-replicated. The 300 nodes have 562 TB raw, 394 TB at a
70% fill ceiling, leaving room for indexes, dedupe, transaction logs and recovery. Older
ranges are compacted into immutable encrypted segments in object storage (600 TB a year,
read through the CDN cache), which is where the other eleven months live. Each append is one transaction of about 5 small writes; FDB clusters sustain this
class of load in production at Apple and Snowflake scale. The per-Space head key limits one
Space to a few hundred unbatched appends/s, which T2/T3 batching raises to thousands; no Space
in the model needs more.
