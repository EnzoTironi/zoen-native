# ADR 0010: A versioned protobuf protocol with signed bytes kept verbatim and causal links

Status: implemented (S1). Supersedes the signing rule of ADR 0002.

## Context
The milestone 1 protocol was JSON over WebSocket text frames, and the relay re-serialized
the typed body to verify signatures. A relay one version behind dropped unknown fields and
every signature failed (seen in the photo journey). JSON also costs CPU and bytes at scale.

The interrogate review (finding 1, critical) showed that a hash chain the relay computes
doesn't stop the relay from omitting, reordering or forking history: it can rehash any
variant and every check passes.

## Decision
- **Event format v3** (`roda_log::content`). The author encodes `SignedContent` once as
  protobuf: space, client id, author, device, time, causal link, payload. The content hash
  is `SHA-256("zoen-content-v3\0" ‖ bytes)` and the signature covers it. Every other field
  of an `Event` is a view decoded from those bytes; verification rejects a view that
  disagrees with them. Nobody re-encodes the bytes, so unknown fields and unknown body
  kinds survive every hop, and verification never depends on the verifier's schema.
- **The body stays typed by the Rust enum**, encoded as JSON inside the protobuf `body`
  field. The schema of the 15 body kinds stays in one place (`roda_types::EventBody`), and
  a body kind this build doesn't know decodes to `EventBody::Unsupported { kind }`: it
  verifies, chains and syncs, it just isn't shown. A per-kind protobuf body can replace the
  JSON later behind the same oneof without touching signatures.
- **Sealed payloads** are the same `SignedContent` with a `Sealed { kind, suite, data }`
  payload: one signing rule for plaintext and MLS, and the ciphersuite id travels with it.
- **Causal links.** Every event signs `seen = (seq, hash)`, the newest relay-ordered event
  its author had applied. Devices and the relay check that `seen` names an earlier event of
  the same history. Only a Space's creation and a newcomer adding themselves may sign with
  no link. A Space the device created but the relay hasn't confirmed yet links to the
  genesis it will get (the creator's `SpaceCreated` always lands at seq 0, so its chain
  hash is known in advance); offline-first still works.
- **Wire protocol v2** (`roda_proto::wire`, `zoen.sync.v2`): one protobuf message per
  binary WebSocket frame, schema declared with `prost` derives next to the domain types so
  they can't drift. Field numbers are append-only.
- **Version negotiation.** `Hello` carries the protocol version and capabilities; the relay
  answers `Challenge` with the version it will speak (`min(client, relay)`) and the
  capability intersection, and refuses clients older than `MIN_PROTOCOL_VERSION` with
  `Error { code: UpgradeRequired }`.
- **Typed replies.** `Res` carries `Reply::{Registered, Profiles, Invite, Preview}` or an
  error string instead of free-form JSON.
- **Backpressure.** Catch-up is cursor-based and paged (500 events per query); the relay
  writes through a bounded queue per connection, so a slow socket slows its own catch-up,
  and a device that falls too far behind on live fan-out is disconnected and resumes from
  its cursors. Credit-based flow control was considered and not needed: TCP plus a bounded
  queue gives the same property with one fewer moving part.

## What it does not stop yet
Causal links catch reordering and split views whenever an author's later event reaches a
device. A relay can still withhold the newest events from someone (tail omission) or show
two groups of members disjoint histories that never cross. M2 closes that with member-signed
head checkpoints gossiped inside MLS.

## Migration
v2 logs were signed over JSON and can't be carried into v3. Before launch, the relay
migration `0002_signed_bytes.sql` resets every table and devices wipe themselves on the
format bump (the existing `event_format` marker). From v3 on, no format change needs a
reset: new fields and kinds pass through old relays and clients verbatim.

## Proof
- `roda-log`: tampered bytes, a view that disagrees with the bytes, forged authors, a newer
  client's unknown field and kind surviving JSON storage, a sequencer moving an event before
  what its author saw, words signed on one history shown on another, and the rule for
  unlinked events.
- `roda-proto`: every client and server frame round-trips; an envelope crosses the wire
  byte for byte; garbage never panics the decoder.
- `roda-ffi`: a device refuses a relay that hides a message and rehashes the chain.
- `zoen-cli/tests/journey_wire.rs` (real relay, real Postgres, real `zoen` processes): a
  newer client's event survives the relay and an older peer, who verifies it and replies on
  top; the relay refuses events signed on a history it doesn't have; an outdated client is
  told to upgrade and a newer one is talked down.
- `journey_m1.rs` (7 journeys) passes over the binary protocol, including a group created
  and written in while offline.

## At 1B users
- DAU 1B, 100 messages per DAU per day: 100B events/day, ~1.2M/s average, ~3.5M/s peak
  sequenced, and ~10M/s deliveries at peak after fan-out.
- A text message's signed content is ~250 bytes in v3 against ~420 in v2 JSON with hex
  fields; frames are protobuf, so the edge parses ~5x faster than serde_json on the
  envelope. At 10M deliveries/s that's ~1.7 GB/s less egress and a few hundred fewer edge
  cores.
- The causal link adds ~75 bytes per event (≈7.5 TB/day across the fleet before
  compression, ~2.7 PB/year raw, in line with the 1B storage model in docs/cost-model.md)
  and one point read per append on the relay (in FDB, inside the same transaction: one more
  key read, no extra round trip).
- Mixed-version rolling deploys are safe by construction: no node re-encodes signed bytes.
- Cost: under $0.0001 per user per month for the extra bytes and reads.
