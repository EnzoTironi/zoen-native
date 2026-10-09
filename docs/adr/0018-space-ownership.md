# ADR 0018: Persisted Space ownership and transaction fencing

Status: implemented, with production release gates below

## Context

Each cell has 4096 Space partitions. FoundationDB serializes a Space's head and
its dedupe keys, but the former relay ownership check used a private in-memory
table and a one-year lease. Two processes could each claim every partition.
There was no shared ownership authority or storage check of a fencing token.

## Decision

Store both the live node registry and partition leases under the existing
FoundationDB cell root, `("zoen", cell)`:

- `("ownership", "nodes", node)` contains the node's expiry version.
- `("ownership", "lease", partition)` contains its owner, generation and expiry
  version. Never delete a lease on handoff, so its generation cannot reset.

A process uses a fresh random 128-bit node ID. Space hashing and rendezvous
placement remain unchanged. Each node heartbeats and renews its preferred
partitions every third of the configured lease period. A change in preference
stops renewal of those partitions. The preferred replacement claims them after
expiry. The node registry removes expired entries under conflict reads and caps
its live/retained scan at 4096 entries. Leases cap partition state at 4096 keys.

Renewal of an unexpired lease keeps its generation. Takeover or reacquisition
of an expired lease increments it, including reacquisition by the same node.
Loss of the forwarding connection also stops renewal and refuses new mutations,
so a relay isolated from NATS can hand over through FoundationDB expiry. An
overflow fails closed. Keeping a generation on ordinary renewal prevents
renewals from rejecting already queued work from the same owner.

Expiry uses the database read version, not a relay's wall clock. The default
lease span is 15 million versions. `ZOEN_OWNER_LEASE_MS` sets a span at 1000
versions per requested millisecond, from 1000 through 60000 milliseconds. This
is a liveness setting, not a wall-clock SLA. FoundationDB 7.3 defaults to a
million versions per second, and recovery can advance versions. The fencing
check and takeover conflict remain the safety mechanism regardless of timing.
See [FoundationDB's version settings](https://github.com/apple/foundationdb/blob/release-7.3/fdbclient/ServerKnobs.cpp)
and [transaction conflicts](https://apple.github.io/foundationdb/developer-guide.html#conflict-ranges).

Every queued envelope carries the partition, owner and generation resolved at
its ingress. The append transaction reads the persisted lease without snapshot
isolation and checks the generation and expiry before dedupe, admission or any
membership/pruning mutation. A takeover or renewal that commits before this
transaction invalidates that conflict read. A retry rechecks authority. Invalid
jobs receive a retryable rejection, including duplicates submitted by a stale
owner. Log ordering, MLS admission, membership, pruning and durable dedupe use
the existing transaction and head check.

Invite creation uses the same fence. A forwarded retry keeps the same random
invite code and returns the existing expiry rather than minting a second
capability. Membership checks still occur in its FoundationDB transaction.

## Forwarding and bounds

The ingress resolves the current persisted lease. It sequences locally or sends
one request to that owner's cell-scoped NATS subject. The receiving owner never
forwards the request again. It checks the signed envelope and current enrolled
device at its own Postgres directory, recomputes target registration, and passes
the supplied generation into the append transaction. Invite requests carry the
identity already authenticated by the ingress and still require the owner's
transactional membership check. NATS must remain a private, authenticated cell
service with permissions restricted to trusted relay publishers and receivers.

Each ingress performs at most two attempts with the same envelope/client ID.
An uncertain answer is safe to retry through durable dedupe. RPC attempts time
out after two seconds. Ownership and append transactions have a two-second
transaction timeout and five retry limit. The complete mutation routing path,
including maintenance lock waits, has a six-second deadline. A caller timing
out can retry its durable client ID. A queued job keeps its capacity until it
finishes. Queued requests recheck forwarding health before renewing, so they
cannot keep an isolated owner alive indefinitely. Saturation rejects without
waiting for capacity:

| Resource | Limit |
| --- | --- |
| Requests resolving/routing at one relay | 4096 and 64 MiB of admitted envelope bytes plus accounting |
| Queued/executing sequencer jobs | 4096 and 64 MiB of stored bytes plus accounting |
| Queued jobs per Space | 128, plus one executing batch of at most 64 |
| Active sequencer workers | 4096, with 30-second idle expiry |
| Forwarding requests, outgoing plus executing incoming | 64 and 32 MiB |
| NATS owner subscription queue | 32 messages |
| NATS owner client command queue | 128 commands |

These bound work admitted by this relay path. They are not a whole-process RSS
limit, a TLS connection limit, a bandwidth benchmark, or a billion-user capacity
result. The NATS broker's payload limit also applies to forwarded frames.

Without NATS a cell supports a single relay owner. A second node cannot bypass
an unexpired foreign lease. It returns a retryable error when forwarding is
required. Reads and catch-up remain shared. Live fan-out remains best effort;
a process or bus failure after storage commit can delay live delivery until
catch-up. Stored events and replay acknowledgments remain durable.

`/readyz` requires a current shared node heartbeat and, when forwarding is
configured, a connected forwarding client, in addition to Postgres availability.

## Verification and release gates

`cargo test -p zoen-relay --test ownership` runs two actual relay processes,
WebSockets, Postgres, FoundationDB and NATS. It checks renewal over multiple
lease periods, forwarding in both directions, MLS admission, an independent NATS network partition and readiness loss, paused-owner
expiry takeover, stale writes/invites, replay after resume and process death,
a gapless deduplicated durable log, and independent revoked-device rejection.
A paused destination receives 128 concurrent probes; the 64-request limit
rejects excess work and all attempts terminate. A separate real transaction
checks that admission before takeover cannot commit after takeover: it expires
the fixture lease in the store, then lets the other node claim it, and requires
FDB's conflict error (1020), rather than an old-transaction error.

The native outbox holds events that cite an unconfirmed genesis until that
genesis is confirmed locally. A temporary creation rejection during relay
restart must leave its dependent messages queued. The offline CLI journey and
a retry-and-relaunch core regression verify that ordering. The no-bus control
routes the saved client account to the second relay and checks both relays use
the local bus before asserting live traffic stays on its origin node.

Sequencer unit tests hold the batch destination, abandon callers, fill a Space
queue, check retryable excess rejection, and check that capacity returns only
when queued jobs finish. Count and byte limits also apply across Spaces.
Existing log-store and device-revocation tests remain regression gates.

Production release still requires a coordinated drain of older relay writers,
which do not perform this fence. Running an old unfenced writer beside new
owners does not establish exclusive authority. It also requires authenticated
NATS/TLS configuration, independent FoundationDB and asymmetric network fault injection, regional
quorum/disaster recovery tests, ownership churn and mixed-load soak, and
measurement of latency, RSS, lease traffic, media and AI costs. Process pause
proves expired authority rejection; it does not prove every network partition
scenario. No modeled billion-user figures are measured evidence.
