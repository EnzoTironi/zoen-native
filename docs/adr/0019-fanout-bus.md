# ADR 0019: Fan-out bus between relay nodes

Status: accepted (S5)

## Context
One relay process held every online session in memory (`Hub`), so it was the whole cell:
a second process on the same Postgres and FoundationDB would converge stored events through
the shared log, but nothing live (typing, presence, an event pushed to an open app) would
reach a device connected to the other process. ADR 0009 picked per-Space logs with pub/sub
fan-out, and the decisions log picked NATS as the one fan-out primitive.

## Decision
- Every frame a session should see goes through `Fanout::send(to, frame, skip)`: the local
  `Hub` delivers to sessions on this node, and a `Bus` carries the same frame to the others.
- `LocalBus` is the single-node case and does nothing. `NatsBus` is the cluster case:
  - Every identity with at least one session on a node is one NATS subscription on that node
    (refcounted per session). A publish reaches exactly the nodes where the recipient is
    connected (NATS interest routing), never the others.
  - Subjects are `zoen.<cell>.to.<sha256("zoen-subject-v1" ‖ identity)[..16]>`. NATS sees a
    pseudonym, not an identity, and cells never hear each other.
  - Frames travel in the same protobuf encoding as the WebSocket (`ServerFrame::encode`), with
    a `Zoen-Origin` header so a node ignores its own echo.
  - Publishing is non-blocking: frames go into one ordered outbound queue per node (65 536
    deep); a full queue drops and counts (`zoen_relay_bus_dropped_total`).
  - Presence is the same subscription answering a `Zoen-Kind: ping` request. A reply means
    "online on another node"; NATS "no responders" means offline everywhere. No KV, no
    heartbeat state to go stale.
- Delivery is at most once. A device that misses a frame catches up by cursor with `Sync`,
  exactly as after a reconnect; stored events never depend on the bus.
- Node ids are per process (`n<12 hex>`) and feed Space ownership (ADR 0018), so two relays
  behind one public name are still two nodes.
- `ZOEN_NATS_URL` (or `--nats`) turns the bus on. Dev runs a private NATS with
  `scripts/nats.sh`; k8s runs a three-server NATS cluster (`infra/k8s/base/nats.yaml`).

## Proof
`crates/zoen-cli/tests/journey_cluster.rs`: two relay processes, one Postgres, one FDB cell.
Ana is on node A and Bruno watches from node B. Ana's presence, her typing (never stored) and
her message all reach Bruno live. The control runs the same two nodes without a bus: Bruno
still reads the message from the shared log, but no typing and no presence cross.

## Consequences
- Staging stays one relay with `LocalBus`; NATS on Fly (≈ US$3.69/month, ADR 0015) is only
  added when staging runs a second relay. The gate stays under US$50.
- Presence checks on login cost one request per co-member who isn't local, sent
  concurrently with a 750 ms cap.

## At 1B users
- Per-identity subjects make a frame cost one publish per recipient. For DMs and small groups
  (the overwhelming majority) that is the cheapest exact routing there is. Spaces above a
  member threshold (≈ 256) switch to one subject per Space, `zoen.<cell>.sp.<space>`, which
  each edge subscribes to while any local member is online; that is one publish per event
  regardless of size (system-design.md, "Hot Spaces").
- Subscription count equals online identities per node (~1M per edge at 1B users with ~1000
  edges): within NATS limits, with interest propagated across the cluster by gateways per
  region.
- Presence pings are batched per destination node once the node directory lives in NATS KV
  (lease bucket from ADR 0018), turning O(co-members) requests into O(nodes holding them).
- Durable consumers (push, search indexing, agents) use JetStream streams `EV_<n>` on a
  separate cluster, never this live path.
