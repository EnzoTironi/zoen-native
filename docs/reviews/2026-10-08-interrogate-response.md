# Response to the 2026-10-08 interrogate pass

> Dated source review. Figures, availability and source references retain their original review date. Current implementation and completion gates are in [roadmap status](../roadmap-status.md).

Reviewer: the `codex` CLI on the box, read-only sandbox, over system-design.md, security.md
and ADR 0008 (its report is next to this file). The `grok` CLI could not take part: it isn't
logged in on the box, and logging in needs a browser device-code flow.

| # | finding | verdict | action |
|---|---|---|---|
| 1 | Hash chain does not stop operator equivocation | accepted, critical | security.md narrowed now. S1 adds causal links: each event signs the `(seq, hash)` of the newest event its author had seen; clients reject a history that contradicts a signed link. M2 adds member-signed head checkpoints gossiped inside MLS. Journey: a relay that rehashes a fork is caught. |
| 2 | "Sent means never lost" vs async DR | accepted | Invariant 3 rewritten: sync replication across zones before the ack, and the sender keeps its outbox entry until a head proves the DR region has it; region loss heals by deduped re-publish. |
| 3 | Server agents break the operator threat model | accepted | security.md: server agents are trusted recipients and declared readers; operator-resistant Spaces use device-side agents only. M3 shows this in the agent disclosure. |
| 4 | KT deferred and insufficient | accepted | security.md says safety numbers are the guarantee until KT ships with witnessed checkpoints and an authenticated handle → identity → device chain. KT design moves into M2's scope list. |
| 5 | MLS membership/revocation incomplete | accepted | M2 design adds: membership changes as public MLS proposals plus a signed plaintext membership delta bound to the commit's epoch; the relay accepts commit, delta and Welcome in one FDB transaction; revocation is a durable per-device log entry, not a cache notification. |
| 6 | FDB credentials aren't least privilege | accepted | security.md: only sync and key services reach FDB, behind NetworkPolicy; others use their APIs. |
| 7 | FDB storage math wrong | accepted | ADR 0008: i4i.2xlarge has 1.875 TB; FDB keeps 30 days hot (150 TB replicated, fits 394 TB usable); older ranges become segments in object storage. |
| 8 | Outbox keys collide in batches; watches don't see prefixes | accepted | `outbox/{partition}/{versionstamp}/{index}` and a watched `outbox_tick` counter with a 250 ms poll fallback. |
| 9 | Lost last frame never shows a gap | accepted | Head reconciliation on `Ready`, resubscribe and a 30 s heartbeat; forwarder replays committed outbox entries. |
| 10 | JetStream capacity and isolation | accepted | 64 stream shards per cell, 24 h retention, byte caps with `discard: new`, separate NATS cluster from core fan-out; older replay from FDB. Cost model to include it (S8 measures it). |
| 11 | Reconnect-storm and subscription sizing | accepted, open | S8 adds a correlated reconnect storm (10% of simulated devices within 60 s) with bounded admission at the edge; the result goes into ADR 0009. Federated membership discovery is part of S4. |
| 12 | Presence is persisted | accepted | Memory-storage KV, R1; only foreground devices; 500k refreshes/s sized. |
| 13 | Idempotency expires; external effects repeat | accepted | 6-day client retry horizon under 7-day dedupe. Agent tool calls persist an operation id before executing and pass it as the provider's idempotency key; ambiguous results are reconciled, not retried blindly (M3). |
| 14 | Retention and erasure vs key lifecycle | accepted, open | Removal guarantees are future access only; the memory ADR's cache claim narrows to that. Per-account backup keys and the offline MLS recovery path go into the multi-device and recovery work. |
| 15 | Per-connection caps don't bound node memory | accepted | Node-wide 8 GB queue budget, 256 KB per connection, control frames bypass, shed slowest first. |
| 16 | Atomic add isn't a strict limiter | accepted | Token leases of 50 per serializable transaction (bounded overshoot), per-device buckets in edge memory. |
| 17 | Summed p99s don't prove the SLO | accepted | The budget stays as a design aid; the SLO is measured end to end by S8 under overload. Server `before` hooks count against the 200 ms budget for synchronous paths; their 300 ms limit applies only to non-chat events. |
| 18 | Log identifiers were raw prefixes | fixed in code | `zoen-relay::pseudonym`: keyed SHA-256 pseudonyms; the raw-prefix helper is gone. |
