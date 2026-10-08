# Zoen architecture (the real one)

This is how Zoen works now that chats leave the device. The product thinking behind it is
in [repensado.md](repensado.md); decisions with their trade-offs are in [adr/](adr/).

```
 iPhone / Mac app (SwiftUI)                         zoen-relay (Rust, axum + tokio)
 ┌──────────────────────────┐                       ┌───────────────────────────────┐
 │ AppModel / SyncModel     │                       │ /v1/sync  WebSocket           │
 │   ▲ CoreListener (UniFFI)│   wss  ClientFrame →  │   session: login, rate limit  │
 │ RodaEngine (roda-ffi)    │ ◄──────────────────►  │   hub: online devices, fan-out│
 │   projection · grants    │      ← ServerFrame    │ log: LogStore (one FDB txn    │
 │   net task (tokio)       │                       │   per append: admit + chain)  │
 │ roda-store (SQLite)      │                       │ FoundationDB: Space logs,     │
 │   events · outbox · FTS5 │                       │   heads, dedupe, members,     │
 │ Keychain: device key     │                       │   invites                     │
 └──────────────────────────┘                       │ Postgres: identities, handles,│
                                                    │   devices                     │
                                                    │ /healthz /readyz /metrics     │
                                                    └───────────────────────────────┘
```

## Crates

| crate | what it is |
|---|---|
| `roda-types` | Primitives (Identity, Space, Member, Item, Grant) and `Event` (format v3: signed protobuf bytes plus views, ADR 0010). |
| `roda-log` | Signing and verification: `Author` (identity root key or certified device key), content hash, relay chain hash, `SpaceLog`. |
| `roda-proto` | The wire protocol: `Envelope` (what an author signs), `Sequenced` (what the relay adds), client/server frames. |
| `roda-store` | SQLite on the device: events, identities, media blobs, FTS5 search, and the sync tables (`outbox`, `synced_spaces`). |
| `roda-ffi` | The core the apps link (UniFFI): projection, grants, agents, and `sync.rs`/`net.rs`/`api.rs` for accounts and the relay connection. |
| `zoen-relay` | The server: orders, verifies, stores and fans out envelopes. Never invents content. |
| `zoen-cli` | `zoen`, the same core in a terminal. Used by the journey tests and for poking at a relay. |

## One message, end to end

1. You type in a shared chat. `send_message` → `append_at` sees the Space is relay-ordered,
   signs a `SignedContent` with this device's key (certified by your identity key), gives it
   a ULID `client_id`, stores it in the **outbox** and projects it right away (`Sending…`).
2. The net task (a tokio runtime inside the core) is woken and sends `Publish { env }`.
   Offline? It stays in the outbox, survives relaunches, and goes out on the next login.
3. The relay checks the session is that author and device, verifies the signature, then in
   one FoundationDB transaction reads the Space head, the roles and invite involved and the
   entry the event's causal link names, runs the admission rules, assigns `seq`, computes
   `hash = H(space, seq, prev, wire_hash)` and commits the entry, head, dedupe key and any
   membership change together (ADR 0008). A repeated `client_id` returns the stored copy
   (idempotent retries); two relays appending to one Space conflict and retry, never fork.
4. It answers `Accepted` and fans the `Sequenced` event to every online device of every
   member (including your other devices).
5. Each device verifies author signature **and** chain link before storing (`SpaceLog::accept`).
   A gap triggers a targeted `Sync`; a bad chain is refused, not stored.
6. Your copy is confirmed: the outbox row goes, the entry keeps its id (the `client_id`), so
   the UI doesn't flicker; `Sending…` disappears.

Reconnects run `Hello → Challenge → Auth(signed nonce) → Ready → Sync(cursors) → flush outbox`
with jittered exponential backoff. Typing, "what I'm doing" and presence are ephemeral frames:
forwarded to online members, never written anywhere (see ADR 0005).

## Local vs relay Spaces

A Space is **local** (sequenced on the device, the way everything worked before) or **synced**
(sequenced by the relay). Your personal Space and the DM with your own Zoen agent stay local
until the agent runtime exists (milestone 3). DMs and groups with people are synced. The
same `Event` type and verification code serve both.

## Accounts

No email or password. Creating an account makes two Ed25519 keys on the device: the identity
(root) key and this device's key, plus a certificate (identity signs device), and an X25519
agreement key for receiving profile keys. All three secrets go to the Keychain through the
`SecretVault` callback; SQLite only has public data. The relay learns your @handle the
first time it's reachable. See ADR 0003.

## Profiles

Name, bio and photo are encrypted under a per-person profile key. The relay stores only the
ciphertext, a version and a device signature. The key reaches the people you share a Space
with as sealed shares inside that Space's log; everyone else sees your @handle. Blocking
someone rotates the key. See ADR 0016 and [api-profile.md](api-profile.md).

## Running it

```
scripts/dev-stack.sh                   # FoundationDB + Postgres + relay on :8787
scripts/run.sh ios                     # app in the simulator, talks to 127.0.0.1:8787
target/debug/zoen --home /tmp/b init --name Bruno --handle bruno
target/debug/zoen --home /tmp/b watch  # see what arrives
```

`-RodaDemo YES` brings the old demo story back (screenshots); a normal launch is real.

## Tests that prove it

`crates/zoen-cli/tests/journey_m1.rs` spins up a fresh Postgres database, a fresh
FoundationDB cell and a real relay process per test and drives `zoen` processes like people would: DM round trip across
relaunches, offline writes that flush once, catch-up after being away, groups and invite
codes, typing that never touches the log, and an attacker who tries to post where they
aren't a member or as someone else. What the relay stored is read back with
`zoen-relay log read|spaces`. `crates/zoen-relay/tests/log_store.rs` holds the store to its
contract on a real cluster (gapless chains under concurrent writers, full catch-up,
idempotent retries, atomic membership, bounded invites). `journey_cluster.rs` runs two relay
nodes over NATS, `journey_limits.rs` the abuse controls, and `journey_telemetry.rs` follows a
message across both nodes in one trace while proving no identifier, handle, message text or
client address reaches the OTLP export or the debug log (ADR 0021). Run with

```
scripts/fdb.sh up && eval "$(scripts/fdb.sh env)"
scripts/nats.sh up && eval "$(scripts/nats.sh env)"
ZOEN_TEST_PG=postgres://user@host/postgres cargo test -p zoen-cli -p zoen-relay
```

## What's next (milestones)

2. MLS (OpenMLS): `Payload::Sealed` already exists in the protocol; the relay stores opaque
   ciphertext and checks only envelope signatures and membership proposals.
3. Agent runtime as an MLS member, model gateway, Grants for tool calls.
4. Media: client-side encrypted blobs in S3-compatible storage. Photo backgrounds already
   work this way (ADR 0007); voice notes and files come next.
5. Push: APNs gateway + Notification Service Extension.
6. Store/catalog API.
7. Deploy: Dockerfile, Fly.io, metrics and alerts.
