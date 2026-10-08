# ADR 0002: Authors sign content, the relay signs nothing and chains it

Status: superseded by ADR 0010 (format v3 signs exact protobuf bytes and a causal link)

## Context
Format v1 signed `(space, seq, prev, …)`: the author had to know the sequence number and the
previous hash. That only works when one device writes alone. With many devices, offline
writes and E2EE, the author can't know its `seq` when it signs.

## Decision
Split the event in two:
- `SignedContent { v, space, client_id, author, device, at_ms, body }`: what the author
  signs (with the identity key, or a device key plus the identity's certificate). For
  sealed (MLS) payloads the signature covers the ciphertext's hash instead of the body.
- `ChainLink { space, seq, prev, wire }`: what the relay adds. `hash = SHA-256` of it,
  where `wire` is the hash of the signed envelope. The relay can't change content (the
  author's signature would break) and can't reorder or drop silently (every device checks
  the chain).

`client_id` (a ULID made on the device) makes retries idempotent: the relay keeps one row
per `(space, author, client_id)` and answers a repeat with the stored copy.

Format v1 logs are not migrated: before this change every log on a device was the seeded
demo, so `migrate_event_format` drops v1 data once on open and the demo (behind its flag)
reseeds in v2.

## Consequences
- Optimistic UI: the entry's id is the `client_id`, stable from "Sending…" to delivered.
- The relay is trusted for order and availability, not for content.
- MLS fits without another format change (`Payload::Sealed`).

## At 1B users
20B events a day each carry one Ed25519 signature (about 50 µs to verify, so 700k/s
needs about 35 cores across the fleet) and one SHA-256 chain link. The relay never re-encodes
what it verifies: from S1 the signature covers the author's exact protobuf bytes (ADR 0010),
so mixed relay and client versions in a rolling deploy can't break signatures. `seq` is per
Space and owned by that Space's shard, so there's no global counter to contend on.
