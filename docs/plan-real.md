# Making Zoen real: the plan

Zoen started as a native app over a seeded demo. This plan turns every surface into the
real thing: accounts, sync, end-to-end encryption, agents, media, push, a store and a
deployable server. Each milestone is a vertical slice that ends in an end-to-end journey on
real clients (the `zoen` CLI and the iOS/Mac apps against a real relay and Postgres).

Decisions with trade-offs live in [adr/](adr/). The running trail of what was decided, why
and on what evidence is [decisions-log.md](decisions-log.md). Accounts created for the
project are in [accounts.md](accounts.md).

## Rules for every milestone

- No stubs or fakes in shipped code paths. Test doubles live under `tests/` only.
- The demo story stays behind `-RodaDemo YES` (ADR 0006). A normal launch is real.
- Real persistence and real migrations: SQLite `user_version` steps on the device, `sqlx`
  migrations on the relay. Nothing is dropped on upgrade except the pre-v2 demo logs.
- Real crypto from audited libraries: Ed25519 (ed25519-dalek), OpenMLS, XChaCha20-Poly1305.
  Device secrets live in the Keychain, wrapped by a Secure Enclave key on hardware (M2).
- Every operation that crosses the network is idempotent (ULID `client_id`, content-hashed
  blobs, `ON CONFLICT DO NOTHING`), so retries and crashes converge.
- A milestone is done when its journey passes on real clients and its ADR is written.
- Anything that needs Enzo's accounts (Apple, APNs, domain, paid hosting, model keys) is
  built completely against the real API and listed under "Needs Enzo" below.

## Scale: built for a billion people

Every design choice is checked against one model of a billion users. The numbers are
deliberately round so anyone can redo them. Each ADR has an "At 1B users" section that uses
this model; the load generator (S8) replaces the per-node guesses with measurements.

| quantity | assumption | value |
|---|---|---|
| monthly users | | 1B |
| daily users (DAU) | 50% of monthly | 500M |
| messages sent | 40 per DAU per day | 20B/day, 230k/s average, 700k/s peak (3x) |
| device deliveries | 5 devices per message on average (2.2 people, 1.6 devices each, groups skew it up) | 100B/day, 3.5M/s peak |
| connected devices | 25% of DAU online at peak, 1.3 devices each | 160M WebSockets |
| connections per edge node | 150k (tokio, about 30 KB each with TLS) | about 1,100 edge nodes, 1,500 with headroom |
| relay log | 1 KB per envelope with MLS overhead, kept 30 days for catch-up | 20 TB/day, 600 TB hot, 1.8 PB with 3 replicas |
| media | 2 photos per DAU per day at 200 KB | 200 TB/day, 73 PB/year in object storage behind a CDN |
| pushes | half of deliveries go to offline devices, collapsed per chat burst | about 10B/day after collapsing |
| infrastructure cost | edge plus sequencers plus hot storage, before media egress | about $2M/month, $0.002 per user per month |

What the model forces:
- **Stateless edge, sharded relay.** Edges terminate WebSockets and hold no durable state.
  Each Space has exactly one owner shard, chosen by rendezvous hashing over the live shard
  set. The owner holds a lease with a monotonically increasing fencing token, and storage
  rejects an append carrying a stale token, so a paused old owner can't fork a log. Today
  one process is the edge and the only shard; the seams are in place to split them.
- **Per-Space logs, not per-device inboxes, on the hot path.** A message is one append to
  its Space's log no matter how many members it has. Online devices get it through a
  pub/sub subject per Space; offline devices catch up with cursors. Nothing writes
  O(members) rows per message. ADR 0009.
- **A versioned binary protocol from day one.** Protobuf frames over WebSocket binary
  messages, signatures over the exact bytes the author produced (the relay never
  re-encodes), cursor-based sync with page limits, and credit-based backpressure.
  ADR 0010.
- **Globally unique, sortable ids.** ULIDs for Spaces, events, invites and blobs' metadata;
  identity and device ids are public keys. No auto-increment and no global counter. The
  only counter is a Space's own `seq`, owned by that Space's shard.
- **A swappable storage seam.** The relay talks to `LogStore`, `Directory` and `Mailbox`
  traits. Postgres is hash-partitioned by Space; a second, ordered key-value backend with
  the key layout FoundationDB or Scylla would use runs the same journeys. ADR 0011.
- **MLS within its limits.** End-to-end encrypted groups are capped at 1,000 members (D10 in
  [repensado.md](repensado.md)); bigger communities are Closed. Key packages are replenished
  when a device drops below a threshold, and commit races resolve by relay order.
- **Media on content-addressed blobs** that a CDN can cache forever (ADR 0007).
- **Push through an idempotent, batching gateway** keyed by `(device, space, seq)`.
- **Rate limits per device and per account**, enforced at the edge (device) and at the
  Space owner (account), so one abuser can't flood a Space from many devices.
- **OpenTelemetry** traces and metrics from the start.
- **Region-pinned Spaces.** Each Space has a home region (São Paulo first); its owner shard
  runs there. Multi-region is more shards with a region in the placement key.

## Scale units

These come before M2, because MLS, agents and push all ride on the wire format and the
storage seams. Each unit ends with the full journey suite green.

1. **S1 binary protocol. Done.** Event format v3 (signed protobuf bytes kept verbatim,
   causal links), wire protocol v2 (`zoen.sync.v2`, binary frames, version negotiation,
   typed replies), paged cursor sync with bounded-queue backpressure. Fixes the
   forward-compatibility bug in M1.4 by construction and closes the reorder and split-view
   half of interrogate finding 1 (ADR 0010).
2. **S2 sortable ids. Done.** `new_id` returns a lowercase ULID; `new_secret_id`
   keeps 128 random bits for capability-bearing ids; `id_time_ms` recovers the
   millisecond (ADR 0017).
3. **S3 FoundationDB log store. Done.** `LogStore` trait; Space logs, heads, dedupe,
   membership and invites in FoundationDB with admission inside the append transaction;
   Postgres keeps the directory; journeys and a store contract suite on a real cluster;
   staging FoundationDB on Fly; fdb-operator manifests for k8s (ADR 0008, 0011).
   **Encrypted profiles. Done** (slotted in after S3): Signal-style profile keys, sealed
   shares in Space logs, ciphertext-only storage on the relay, rotation on block (ADR 0016).
4. **S4 ownership. Done.** Rendezvous placement over 4096 partitions, in-process
   leases with fencing tokens, single node claims every partition at boot; multi-node
   lease exchange waits on S5 (ADR 0018).
5. **S5 fan-out bus. Done.** `Bus` trait with `LocalBus` and `NatsBus` (pseudonymous
   per-identity subjects, ping presence); two-node journey on NATS plus a no-bus control (ADR 0019).
6. **S6 abuse controls. Done.** GCRA buckets per device, account and address (connect,
   register, publish, ephemeral, requests, lookups, invites, blob bytes); `RateLimited` with a
   retry hint the client honors (ADR 0020).
7. **S7 telemetry. Done.** OTLP traces and logs from the relay's own code only, pseudonymous
   fields, W3C context across the NATS bus, Prometheus metrics scraped by a cell collector
   that scrubs address and URL attributes (ADR 0021).
8. **S8 load generator.** `zoen-load` drives simulated clients over the real protocol and
   reports messages per second per core, p50 and p99 delivery latency, and connections
   per node.

## Where things stand

| milestone | state | proof |
|---|---|---|
| M1 relay, accounts, sync | backend done; app journey in progress | `crates/zoen-cli/tests/journey_m1.rs` (7 journeys), `apple/UITests/RealSyncJourneyTests.swift` |
| M4 encrypted media | core and relay done, app switch pending | `photo_background_travels_encrypted` |
| Staging (Fly gru + Cloudflare) | Release app proven | `scripts/journey-staging.sh`: fresh Release install on one simulator, UI onboarding, CLI peer over relay.tryzoen.com, reply; artifacts in roda-shots/real-staging |
| S1 binary protocol, causal links | done | `journey_wire.rs` (3 journeys), roda-log and roda-proto tests, ADR 0010 |
| S3 FoundationDB log store | done | `log_store.rs` (5 contract tests on real FDB), all journeys on FDB, `log_bench` numbers in ADR 0008, journey-sim on protocol v2 |
| S2 sortable ids | done | `roda-types` `ids_sort_by_creation_and_carry_their_time`, ADR 0017 |
| Encrypted profiles | done | `journey_profiles.rs` (contact reads bio and photo, stranger sees the handle, Postgres holds only ciphertext, live change event, group join, block rotation, unblock), ADR 0016, docs/api-profile.md |
| S4 space ownership | done | `ownership::` rendezvous + fencing tests, ADR 0018 |
| S5 fan-out bus | done | `journey_cluster` (2 relays over NATS + control), ADR 0019 |
| S6 abuse controls | done | `journey_limits` (fast sender loses nothing, flood, caps), ADR 0020 |
| S7 telemetry | done | `journey_telemetry` (two nodes, cross-node trace, 18 secrets absent from OTLP bytes and debug stdout), real otelcol-contrib run in roda-shots/real-s7, ADR 0021 |
| S8 load generator | next | |
| M2, M3, M5, M6, M7 | planned below | |

## M1. Relay, real accounts, sync

Goal: two people on two devices exchange messages through a real relay and keep them across
relaunches. No seeded data outside the dev flag.

Done: relay (axum, Postgres, per-Space sequencing, hash chain, fan-out, ephemeral signals),
account creation with identity and device keys in the Keychain, challenge login, outbox,
catch-up, invites, CLI journeys, the app's onboarding profile step, new chat sheet, account
section in You, delivery marks, typing, presence.

Remaining, in order:
1. Simulator journey green: the app as Ana, the CLI as Bruno, message both ways, kill and
   relaunch. Found and fixed so far: the row tap area, and `wipe()` deleting the event-format
   marker so the next launch erased the new account.
2. Two devices, one relay: the app on one simulator and a headless real client (the `zoen`
   CLI on the same Rust core) chatting both ways, recorded (`scripts/journey-sim.sh`). One
   simulator at a time is a hard constraint on the development Mac.
3. Subtract the dev relay: delete `tools/dev-relay` and the Swift `DevRelay` path; photo
   backgrounds in shared chats go through the core (M4 work below), local chats stay local.
4. The relay verifies what it received, not a re-serialization. Today a relay older than a
   wire-type change drops unknown fields and every signature fails. Fixed by S1: signatures
   cover the exact bytes the author produced and nobody re-encodes them.

## M2. End-to-end encryption with OpenMLS

Goal: the relay stores only ciphertext for DMs and groups. It still orders, checks
membership and fans out.

Shape:
- `roda-mls` crate over `openmls` with a SQLite storage provider in the device database
  (`openmls_sqlite_storage` if it fits the store's single connection, otherwise our own
  `StorageProvider` over `roda-store`). Ciphersuite
  `MLS_128_DHKEMX25519_CHACHA20POLY1305_SHA256_Ed25519`.
- Every device is a leaf. The credential binds the leaf to the identity through the
  existing device certificate, so a member list is "identities with their devices".
- Key packages: devices publish a batch plus a last-resort package; the relay hands out one
  per device on claim (`Op::PublishKeyPackages`, `Op::ClaimKeyPackages`), table
  `key_packages` with single-use claims.
- Ordering: commits are relay-sequenced envelopes in the Space's log, so the relay's total
  order is the MLS epoch order. A commit for a stale epoch is rejected with `stale_epoch` and
  the client rebases (re-proposes after processing the winner). Application messages are
  `Payload::Sealed` (MLS PrivateMessage); the relay sees author, device, Space and size.
- Welcomes go to the added devices' mailboxes on the relay and are deleted on fetch.
- Membership stays checkable by the relay: membership changes carry a signed public
  `MemberAdded`/`MemberRemoved` envelope alongside the commit, and the relay rejects a commit
  whose roster disagrees.
- History: new members read from their join onward (forward secrecy). Closed (relay-readable)
  Spaces stay for communities.
- Device secrets: the Ed25519 secrets are wrapped by a Secure Enclave P-256 key
  (`kSecAttrTokenIDSecureEnclave`) on hardware; the simulator keeps the Keychain item.
- Linking a second device: the new device shows a QR with its key; an existing device
  certifies it and adds it to every group (commit per group, batched).

Proof: CLI journeys where the relay's Postgres has no plaintext anywhere, a removed member
can't read anything after removal, a second device reads new messages, and the simulator
journey passes unchanged on top.

## M3. Real agents

Goal: your Zoen agent and community agents are real members of chats, running on a server,
calling real models and tools under Grants.

Shape:
- `zoen-agentd`: a runtime process that holds agent identities (each owned by a person),
  joins Spaces as an MLS member, and reacts to messages.
- Model gateway: one interface over OpenAI-compatible chat completions with tool calls, with
  per-owner budgets enforced from `UsageRecorded` events. Providers are configuration.
- Tools run only under a signed `Grant`; requests above the grant become `AgentRequest`
  events the owner approves in the app (the existing review UI).
- The app's "Ask Zoen" row hands the query to your agent's DM.

Proof: a journey where a person asks the agent in a group, the agent answers through the
relay, a tool call outside its grant waits for approval, and the usage is debited. Tests run
against a scripted OpenAI-compatible server under `tests/`; a live smoke test runs when a
model key is present.

## M4. Client-side encrypted media

Goal: photos (then voice notes and files) reach everyone in a chat while the server holds
only ciphertext. Replaces `tools/dev-relay`.

Done: per-attachment random key, XChaCha20-Poly1305, ciphertext addressed by its sha256,
signed uploads (`x-zoen-device`, `x-zoen-ts`, `x-zoen-sig`), `object_store` backend
(directory in dev, any S3-compatible bucket in production), a background upload queue in
the device database, downloads with backoff, journey proving the relay never sees the
plaintext. ADR 0007.

Remaining: the app path (above, M1.3), an S3 integration test against a local S3 server,
voice notes and files on the same pipeline, and after M2 the key rides inside MLS.

## M5. Push

Goal: a message wakes the recipient's phone with a notification that shows the decrypted
text, without the relay knowing it.

Shape: device token registration (`Op::RegisterPush`), a relay APNs client (token auth with
a .p8 key over HTTP/2), a content-free push with `mutable-content`, and a Notification
Service Extension that opens the MLS state from the shared app group and decrypts.

There is no Apple developer account yet, so M5 is proven without one. The client path is
proven in the simulator with `xcrun simctl push` delivering the exact payload the push
gateway emits, through the real Notification Service Extension. The server path runs the
real APNs HTTP/2 client against a local APNs-compatible sink (a real HTTP/2 server that
speaks APNs's request and response shapes and records them), plus a documented `dry-run`
mode that signs the provider JWT and builds every request without sending. Switching to
api.push.apple.com is configuration: the .p8 key, Key ID and Team ID.

## M6. Store and catalog

Goal: the Store tab lists real agents and mini-apps, published as signed manifests, with
search.

Shape: catalog tables on the relay (publisher identity, signed manifest, versions), Postgres
full-text search, `GET /v1/catalog` and `/v1/catalog/search`, install as a signed event in
your personal Space.

## M7. Deploy-ready

Goal: one command runs the whole stack locally, and one command deploys it.

Shape: multi-stage Dockerfile (static binary on distroless), `fly.toml`, migrations at boot,
`/healthz` and `/readyz`, JSON logs, Prometheus metrics, a `scripts/dev-stack.sh` that
brings up Postgres, the relay, a local S3 and the agent runtime.

## M2.5. Hooks

After MLS: the hooks primitive in [hooks.md](hooks.md) (ADR 0013), with the safety gate,
push dispatch, search indexing and analytics re-expressed as hooks. Proof: a WASM `before`
hook redacting an outgoing message on device, an `after` webhook receiving metadata only, a
test that server hooks never see plaintext, and a loop-guard test.

## M2.6. Files and memory

After hooks: memory, files, skills and knowledge as Items (ADR 0014). Item kinds and paths,
Personal Space memory, per-Item sealed keys, the index Item, `memory_search`,
`memory_explain` and `forget`, search v2 (FTS5 plus sqlite-vec with RRF), the agent cache
keyed to the MLS epoch, and the signed export bundle. Proof: golden retrieval evals, a forget
test that finds nothing derived afterwards, a revoke test that drops vectors, and a sandbox
test that the agent can't read its own runtime or any secret.

## First-class concerns (after M1 to M3, in this order)

These shape the architecture now; each gets built in full after milestones 1 to 3.

1. **Multi-device and recovery.** Every device is its own MLS leaf. Linking: the new device
   shows a QR with its key, an existing device certifies it and adds it to every group.
   History reaches a new device encrypted device to device (a one-time MLS-protected
   transfer) or from the encrypted backup. A lost device is revoked from any other device
   (removal commits in every group plus a revocation entry in key transparency). Account
   recovery with a passkey: the identity root is encrypted under the passkey PRF.
2. **Version compatibility.** The protobuf protocol carries a version and capability list in
   `Hello`; the edge answers with what it supports and a minimum client version, below which
   the app shows an update screen. FDB and Postgres migrations are expand then contract,
   never a breaking change in one deploy. Server-driven feature flags and kill switches live
   in NATS KV, signed, cached on the client with a TTL.
3. **Offline and bad networks.** The durable outbox exists. Media uploads and downloads
   become chunked and resumable (4 MB chunks, tus-style offsets, each chunk content-hashed).
   Sync adapts to Low Data Mode and Low Power Mode (no prefetch of media, longer ticks), and
   catch-up runs in BGTaskScheduler. Tests run the journeys under Network Link Conditioner
   profiles (3G, Edge, 100% loss bursts) on the simulator.
4. **Agent economics.** The agent runtime meters every model call (tokens in and out, cost)
   and debits the owner's budget through a double-entry ledger with idempotent entries keyed
   by `(job, call)`. Budgets and quotas per user; when a budget runs out the agent says so in
   the chat and stops calling models. Creator payouts and credits post to the same ledger;
   the amounts ($5 budget, $0.50 per install) stay undecided and no payments move.
5. **Agent security.** Untrusted content goes through a quarantined model with spotlighting;
   tool calls pass Cedar grants and the safety gate; agent actions are signed events visible
   to members; nothing leaves the Space without confirmation. Details in security.md.
6. **Cost per user.** [cost-model.md](cost-model.md), fed by `zoen-load` measurements.

Designed and built after those:

7. **Moderation.** Franking-based reports, limits for new accounts, a legal-request runbook.
8. **Ops.** SLO alerts from the OpenTelemetry metrics, runbooks per component, chaos tests in
   the kind cluster (kill a pod, partition NATS, slow FDB).
9. **App Store compliance.** A note on mini-apps that run code (Guideline 4.7: HTML5
   mini-apps in a WKWebView, no native code download) and on in-app purchase for credits.
10. **Private analytics.** Aggregate counters only, no content and no identifiers.
11. **Accessibility, i18n and RTL.** Extend what exists: VoiceOver labels on every control,
    Dynamic Type, pt-BR and en strings, RTL layout checks.
12. **Legal drafts.** Terms of service, privacy policy and data-processing inventory as drafts
    for a lawyer to review.

## Needs Enzo

### Activates when the Apple developer account exists
Enzo has no Apple developer account yet. Nothing waits on it: every milestone is proven in
the simulator, and these pieces are built and gated off until the account exists.
- Team ID: fills the `apple-app-site-association` file the relay already serves at
  `id.tryzoen.com` (404 until then, by design) and turns on passkey login and universal
  links. Until then, login is the device-held identity and device keys (ADR 0003).
- APNs .p8 key with its Key ID (M5): the push gateway switches from the local sink or
  dry-run to api.push.apple.com.
- App Group and Keychain access group on the bundle ids (M2, M5): the Notification Service
  Extension shares MLS state with the app on real devices; the simulator build uses the
  same code path.
- App Attest and DeviceCheck, TestFlight and App Store submission.

### Other accounts
- The domain tryzoen.com is in place (Cloudflare DNS, ADR 0015).
- Hosting with a card on file if the free tiers we try can't host Postgres plus the relay.
- A model provider key for live agent runs (M3).
- GitHub Actions billing: the repository is private and GitHub refuses to start jobs
  ("recent account payments have failed or your spending limit needs to be increased").
  `.github/workflows/ci.yml` is in place and runs the moment billing is fixed in
  Settings → Billing & plans (the free 2,000 Linux minutes a month are enough) or the
  repository goes public. Until then the same steps run on the box before every push.
