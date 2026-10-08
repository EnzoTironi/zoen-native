# ADR 0018: Space ownership (rendezvous + fencing)

Status: accepted (S4)

## Context
A Space's log must stay linear. FoundationDB already serializes appends on the
head key (ADR 0008), so two sync nodes racing never fork. What they still need
is **affinity**: one preferred owner per Space, so membership caches, outbox
forwarding and (later) owner-side batching stay local. The plan calls for
rendezvous hashing over a live shard set and a lease with a fencing token that
storage checks on append and on outbox clear.

## Decision
- The cell is carved into **4096 partitions**. A Space maps to one of them by
  `sha256(space)[0..4] as u32 % 4096` (O(1) on the publish path). A power-of-two
  split remaps half the Spaces by bit.
- A **node** maps to the partitions it owns by rendezvous score over
  `(node, partition)`, given the live node set. Adding a node moves about 1/N of
  partitions (and the Spaces under them).
- The owner of a partition holds a **lease** `(owner, token, expiry)`. Claiming
  or renewing bumps `token`. An action (append affinity check, outbox clear)
  must present the current token; a stale owner is refused even if it still
  thinks it is live.
- Ordering remains FDB's conflict on the head key. The lease never is the
  authority: two owners racing produce one conflict and one retry.
- **Today (S4):** one relay process claims every partition at boot with a long
  TTL (`NodeOwner::claim_all`) and refuses an append for a Space whose
  partition it does not hold. The placement and fencing code is the same one
  multi-node will use.
- **Next (S5):** leases move to NATS KV (`lease/{partition}`), nodes renew on a
  short TTL, and the live set is the KV watchers. Outbox forwarding checks the
  fencing token in the clearing transaction.

## Consequences
- Journeys and staging stay single-process; their behaviour is unchanged.
- A second relay started against the same cell would (once S5 lands) take half
  the partitions and serve only those Spaces; until then a second process would
  also claim all partitions in memory and both would append, with FDB deciding.
- `partition_of` is pure and language-stable, so edges can route a publish to
  the owning sync without asking it first.

## At 1B users
- 4096 partitions over ~200 sync nodes → ~20 partitions per node, each holding
  on the order of 5M Spaces at 1B users. Rendezvous rebalance on a node join
  moves ~0.5% of Spaces.
- Lease renewals are one KV put per partition per TTL/3 (a few hundred puts per
  node per second at a 2 s TTL); negligible next to appends.
- Owner-side batching (many appends per FDB transaction for one hot Space)
  lands on top of this without changing placement.
