# ADR 0001: The relay is a Rust service on Postgres

Status: accepted (milestone 1)

## Context
Chats need a server that puts each Space's events in one order, keeps them, and fans them
out. The plan (repensado.md, D-decisions) already picked Rust for the core. Options for the
server were Matrix (Synapse/Conduit), a hosted BaaS (Supabase/Firebase), or our own relay.

## Decision
Our own relay, `zoen-relay`: axum + tokio for HTTP/WebSocket, sqlx on Postgres. It reuses
`roda-types`, `roda-log` and `roda-proto`, so client and server verify with the same code.
Per-Space ordering is a `SELECT … FOR UPDATE` on the Space row inside the insert
transaction: writers to one Space serialize, different Spaces run in parallel.

## Why not the others
- Matrix brings its own event model, room state resolution and federation we don't want,
  and MLS support there is still experimental. Mapping Zoen's Items/Grants onto it is
  more work than the relay itself.
- A BaaS can't verify our signatures or enforce membership rules in the write path without
  re-implementing them in its function runtime, and ties hosting to it.

## Consequences
- One binary, `cargo run`, testable with a throwaway database per test.
- Single node for now: the in-memory hub only knows its own sessions. Scaling out means a
  fan-out bus (Postgres LISTEN/NOTIFY or NATS) between nodes; the protocol doesn't change.
- `postgresql_embedded` (Zonky binaries) gives a private Postgres for dev and tests without
  installing anything.

## At 1B users
700k messages/s at peak and 160M sockets. One Postgres can't hold that, and this ADR
doesn't pretend it does: the relay talks to storage through traits (ADR 0011) with Postgres
hash-partitioned by Space, and each Space has one owner shard (ADR 0008). Rust and tokio
matter here for connection density: at about 30 KB per TLS WebSocket, 150k connections fit
in 4.5 GB, so roughly 1,100 edge nodes carry peak. Sequencing is cheap (a few hundred bytes
per append); the measured per-core rate from `zoen-load` (S8) sets the shard count.
