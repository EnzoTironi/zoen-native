# Storage, queues and caches

[System design](system-design.md#data-placement)

## Data placement

FoundationDB keyspace (tuple layer, one directory per cell):

| key | value | written |
|---|---|---|
| `log/{space}/{seq}` | envelope bytes (≤ 100 KB, larger content goes to blobs) | per message |
| `head/{space}` | seq, chain hash, membership version | per message (the only per-Space hot key) |
| `dedupe/{space}/{author}/{client_id}` | seq | per message, expired by a job after 7 days |
| `members/{space}/{identity}` and `spaces_of/{identity}/{space}` | role | per membership change |
| `keypkg/{device}/{id}` | key package | per publish, deleted on claim |
| `outbox/{partition}/{versionstamp}/{index}` | space, seq (the index tells apart the appends one batched transaction shares a versionstamp with) | per message, cleared after forwarding |
| `outbox_tick/{partition}` | counter, atomic add on every append | what the forwarder watches; it also polls every 250 ms, so a missed watch only adds latency |
| `lease/{partition}` | owner, fencing token, expiry | per lease renewal |
| `rl/{account}/{window}` | tokens left | the edge leases 50 tokens per serializable read-modify-write, so overshoot is bounded by one lease per edge; per-device buckets live in edge memory |

Ordering. An append reads `head/{space}`, writes `log/{space}/{seq+1}`, the new head, the
dedupe key and an outbox entry under a versionstamp, in one transaction. FDB's conflict
detection on the head key serializes appends within a Space, so no sequencer service exists.
Inside the owning relay, appends to a Space queue in memory and commit in batches of up to 64
behind a head-validated cache of its members (ADR 0023); the head read is still the fence.
Sequence numbers stay dense per Space (clients detect gaps, the hash chain needs the
predecessor); the versionstamp orders the outbox across Spaces. Partition ownership in sync
is an affinity for batching and caches, never the ordering authority: two owners racing
produce one FDB conflict and one retry, never a fork. ADR 0008.

Postgres holds what is relational and low-churn. Object storage holds blobs (ADR 0007) and
cold log segments.

## Queues and streams

One primitive: NATS. Core subjects for latency-critical pub/sub, JetStream for anything that
must survive a crash, KV for small shared state with TTL.

| flow | NATS construct | delivery | ordering | failure handling |
|---|---|---|---|---|
| online fan-out | core subject `sp.<space>` | at most once (clients resync by cursor on any gap) | per Space by seq | lost frame: a gap triggers a pull; a lost *last* frame is caught by head reconciliation (`Ready`, every resubscribe, and a 30 s heartbeat carry the heads of the device's open Spaces); the forwarder publishes only from committed outbox entries, so a crash between commit and publish is replayed |
| durable event stream | JetStream `EV_<n>`, 64 stream shards per cell (each its own Raft group), subjects `ev.<partition>.<space>`, R3, 24 h, `discard: new` at a byte cap; on a separate NATS cluster from core fan-out so a full disk never touches chat; replay older than 24 h comes from FDB | at least once, dedupe by `Nats-Msg-Id = space:seq` | per subject | consumers are idempotent by `(space, seq)` |
| push dispatch | consumer on `EV` plus work queue `PUSH` | at least once, idempotency key `(device, space, seq)` | per device | exponential retry, then `PUSH.dlq` after 8 attempts |
| agent jobs | work queue `AGENT` | at least once, ack wait 30 s extended by progress acks | per agent conversation | cancel by KV flag; checkpoint after each tool call; poison job to `AGENT.dlq` after 3 crashes |
| media post-processing | work queue `MEDIA` | at least once by blob hash | none needed | idempotent outputs keyed by hash |
| search, moderation, metering | consumers on `EV` (Closed and Public subjects only) | at least once | per Space | lag alarms; replay from stream or from FDB |
| webhooks and MCP callbacks | work queue `HOOK` | at least once with an idempotency header | per target | per-target circuit breaker, DLQ |
| analytics | stream `AN`, sampled, no content | at least once | none | dropped first under load |
| presence | KV `presence_<cell>`, memory storage, R1, key per foreground device, TTL 90 s, refreshed every 60 s | best effort | n/a | expires on its own; memory storage never touches disk. About 30M foreground devices at peak: 500k refreshes/s across cells |

Outbox from FDB to JetStream. The append transaction writes `outbox/{partition}/{versionstamp}`.
The partition owner's forwarder reads its range in order, publishes each to `EV` with
`Nats-Msg-Id = space:seq` (JetStream drops duplicates inside its window), then clears the
forwarded range in one FDB transaction. A crash between publish and clear republishes, and
the message id drops the copy. Exactly-once is not claimed anywhere; every consumer gets
at-least-once plus an idempotency key. Sync also publishes on the core subject right after
commit, so online delivery doesn't wait for the forwarder.

Backpressure and load shedding:
- Per connection, edge keeps a bounded send queue (256 frames or 1 MB). On overflow it stops
  pushing events for that connection and sends `Resync`; the client pulls with cursors at its
  own pace. A slow phone never slows a Space.
- Clients grant credits for pulled pages; sync serves at most 500 events per page.
- Edge refuses new connections with `Retry-After` plus jitter above 85% CPU or memory.
- Sync sheds by priority: appends and catch-up reads first, profile lookups and search last.
- JetStream consumers have max-ack-pending limits; producers see backpressure as publish
  latency and the stream's limits drop analytics first.

## Caches

| what | where | invalidation | stampede control |
|---|---|---|---|
| session | edge memory per connection; reconnect uses a 24 h resumption token signed by the edge's key (no shared store) | token expiry; device revocation list in NATS KV | none needed |
| presence and typing | NATS core and memory-storage KV (R1) with TTL, never written to disk | TTL 90 s; disconnect deletes | n/a |
| Space membership and role | sync process LRU keyed by `(space, membership_version)` | versioned key: the head carries the version, so a stale entry can't match | singleflight per Space |
| device to Spaces (for edge subscriptions) | edge memory per connection | membership events arrive on the Space subject | loaded once per login |
| routing (partition to sync owner) | every edge and sync, from NATS KV `leases` | KV watch | n/a |
| key packages | not cached (single use) | n/a | n/a |
| profiles and handles | edge LRU, 5 min TTL with jitter | profile-changed events | singleflight per identity |
| Store catalog | CDN plus `ETag`; catalog service LRU | versioned URLs per catalog revision | CDN request collapsing |
| blobs | CDN, immutable by content hash | never invalidated | CDN request collapsing |
| client | SQLite on the device is the cache and the source of truth for the user | events | n/a |
