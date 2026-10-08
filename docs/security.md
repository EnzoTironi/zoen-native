# Zoen security and privacy

Status: design, folded into [system-design.md](system-design.md) and the ADRs. Items marked
**later** have their seam designed now and their implementation scheduled; nothing here is a
stub in shipped code.

## Assets

Message and media content; who talks to whom and when (metadata); identity and device keys;
contact graph and handles; agent permissions and budgets; backups; the code we ship.

## Adversaries and what they get

| adversary | capability | what the design leaves them |
|---|---|---|
| malicious server operator | reads and alters everything on our servers | ciphertext of E2EE Spaces and of every person's profile (name, bio, photo; ADR 0016), plus handles; author signatures (they can't forge content). Since S1, every event signs the `(seq, hash)` of the newest event its author had seen, so reordering and split views are caught as soon as an author's later event reaches a device (ADR 0010). Withholding the newest events, or keeping two groups on disjoint histories that never cross, is detected only once members gossip signed head checkpoints inside the encrypted channel (M2). Membership metadata until sealed sender lands. Key substitution is detected only once key transparency ships with witnessed checkpoints; until then, safety-number verification is the guarantee. Server-hosted agents are trusted recipients: a Space that admits one trusts its operator and model provider with what the agent reads, and members see it as a declared reader. Operator-resistant Spaces use device-side agents only |
| compromised node (one edge, sync or worker) | memory and traffic of that process | sessions on that node (no long-term keys: edges hold no device secrets); mTLS identity of that workload only, so lateral movement is bounded by least-privilege service accounts |
| nation-state network adversary | observes and tampers with traffic, harvests for later | TLS 1.3 to the edge, MLS inside with an X-Wing (ML-KEM-768 plus X25519) hybrid suite against harvest-now-decrypt-later; padding hides exact sizes; timing is out of scope beyond batching |
| stolen or unlocked device | the device itself, maybe unlocked | Keychain items with `AfterFirstUnlockThisDeviceOnly`, device key bound to the Secure Enclave, app-switcher redaction, optional app lock; remote revocation from another device removes it from every group |
| malicious agent or mini-app | code or prompts inside a Space | runs under Cedar grants evaluated on device and server; mini-apps in a WKWebView with CSP and no network except the bridge; agents are declared readers shown to every member; anything leaving the Space needs confirmation |
| insider | access to infrastructure and logs | no content and no clear identifiers in logs or traces; break-glass access audited; secrets only in KMS-backed stores; production access through short-lived credentials |

## STRIDE per component

| component | spoofing | tampering | repudiation | information disclosure | denial of service | elevation of privilege |
|---|---|---|---|---|---|---|
| device core | device key in Secure Enclave, identity root under passkey PRF | signed, chained logs verified on every event | every event is author-signed | SQLite holds only what the user can already see; keys in Keychain | outbox survives offline | grants evaluated locally before any agent action |
| zoen-edge | challenge login signed by the device key bound to `zoen-sync/1:auth:{relay}:{nonce}` | TLS; frames verified downstream | session logs carry pseudonymous device hashes | no content handling beyond forwarding ciphertext | per-device and per-IP limits, Cloudflare in front, connection shedding | runs with no FDB or Postgres write rights except through sync |
| zoen-sync | mTLS workload identity; envelopes verified by author signature | FDB writes only through append transactions | logs and chain are the audit trail | stores ciphertext for E2EE Spaces; envelope encryption at rest | per-account counters in FDB; priority shedding | membership rules enforced on every append |
| FoundationDB, Postgres, object storage | mTLS from named service accounts only | checksummed storage; backups verified daily | access logs | envelope encryption with KMS keys; per-region keys | capacity alerts | no human write path outside runbooks |
| NATS | per-service NKeys, account isolation per flow | JetStream message ids, signed envelopes inside | stream retention | carries the same ciphertext as FDB | max-ack-pending, stream limits | subject permissions per service account |
| zoen-push | APNs token auth; device tokens bound to device keys | payload is ciphertext plus `(space, seq)` | push ids logged pseudonymously | encrypted push payload, decrypted by the NSE | batching, collapse keys | can read only the push stream |
| zoen-agentd | agent identities signed by owners | agent actions are signed events | tamper-evident audit log visible to members | sees only Spaces it is a declared member of | per-owner budgets and quotas | Cedar grants plus the safety gate on every tool call |
| zoen-media | blob grants signed per device | content hash addressing | upload records | per-file keys; the store sees only ciphertext | upload size and rate limits | presigned URLs scoped to one object and minutes |

## Cryptography

- **MLS (OpenMLS).** Every device is a leaf. Default suite
  `MLS_128_DHKEMX25519_CHACHA20POLY1305_SHA256_Ed25519`; the post-quantum suite
  `MLS_256_XWING_CHACHA20POLY1305_SHA256_Ed25519` (code point 0x004D) is available in
  `openmls_libcrux_crypto` 0.4 behind `draft-ietf-mls-pq-ciphersuites`, still a draft code
  point. We ship it as the default for new groups once the code point is final, and allow it
  now behind a server feature flag. Crypto agility lives in the wire format: every sealed
  payload carries its ciphersuite, key packages advertise supported suites, and groups
  upgrade with a ReInit when all members support the new suite.
- **Device keys.** Ed25519 for signing because MLS and our logs use it; the Secure Enclave
  only does P-256, so the Ed25519 secret is wrapped by a Secure Enclave P-256 key (ECIES) and
  stored in the Keychain. Unwrapping needs the enclave, so a copied Keychain is useless.
- **Root identity under the passkey PRF.** The identity root key is encrypted with a key
  derived from the passkey's PRF output and stored on our server as an opaque blob. A new
  device with the passkey recovers the root and certifies itself. Needs the domain for
  passkeys (Enzo).
- **Key transparency (AKD).** Identity to device-key bindings are published in an AKD
  directory with signed epochs. Clients verify inclusion of their contacts' keys and monitor
  their own; auditors verify epoch-to-epoch append-only proofs. A KT tampering test proves a
  client rejects a forged binding. **Later** (M2.x), with the seam now: device certificates
  already bind device to identity, and the key service owns the directory.
- **Verification.** Safety numbers and a QR code per pair of identities, derived from the
  identity keys; verified state is shown in chat info and warns on change.
- **Metadata minimization.** The relay needs to know a Space's members to authorize writes.
  Plan: sealed-sender style delivery for DMs (the envelope's author is encrypted to the
  recipient; the relay authorizes with a delivery token derived from the Space), and
  anonymous credentials (zkgroup-style, or Privacy Pass tokens) proving membership without
  revealing which member. **Later**; seam: authorization in sync goes through one
  `authorize(space, proof)` function whose proof is today an identity signature.
- **Message franking.** Each sealed message commits to its plaintext with a franking key;
  a recipient can reveal it to moderation in a report, and the server verifies the commitment
  it stored. **Later** with moderation; the sealed payload reserves the commitment field.
- **Push.** Payloads are encrypted to the device; the Notification Service Extension decrypts.
- **Backups.** Encrypted on the device with a key from the passkey PRF or a 24-word recovery
  key; we store ciphertext only and can't help anyone read it.
- **Disappearing messages.** A Space setting carried in a signed event; devices delete
  expired content locally and the relay drops expired envelopes with a retention job.
- **Attachments.** Per-file random keys, XChaCha20-Poly1305 (ADR 0007).
- **Profiles (ADR 0016).** Name, bio and photo reference are sealed under a per-person
  profile key (HKDF-SHA256 → XChaCha20-Poly1305, AAD binding identity and version). The
  relay stores ciphertext, a version and a device signature, and nothing else; the directory
  keeps only id, handle, kind and tint for people. The key goes to each contact as an
  X25519 share (ephemeral key, HKDF with Space, sender and recipient in the context) inside a
  signed Space event. Agreement keys are signed by a certified device, so the relay can't
  substitute them. Blocking rotates the key: the blocked person keeps what they already saw,
  as in Signal. Until M2 the relay sees who shared a key with whom, never the key.
- **Padding.** Sealed payloads pad to buckets (powers of two up to 1 KB, then 1 KB steps);
  attachments pad to 64 KB buckets.

## Server side

- **Zero trust between services.** mTLS everywhere through Linkerd (smaller and simpler than
  Istio, Rust data plane) with workload identities; SPIFFE/SPIRE if we outgrow it.
- **Least privilege.** One Kubernetes service account per binary, NATS subject permissions
  per service, Postgres credentials per service. FDB has no per-client authorization (tenant
  authorization is experimental), so only the sync and key services reach FDB, over a
  NetworkPolicy-restricted network; every other worker goes through their APIs, which enforce
  operation and prefix permissions.
- **Secrets.** External Secrets Operator reading the cloud KMS-backed secret manager in
  staging and prod; SOPS with age for the `local` environment, with the age key outside git
  (docs/infra.md). Envelope encryption at rest for FDB volumes, Postgres and object storage
  with KMS keys per region.
- **Logs and traces (built, ADR 0021).** No content and no clear identifiers: identities,
  devices and Spaces appear as keyed pseudonyms (`p:` + 12 hex of SHA-256(key ‖ id), key from
  `ZOEN_LOG_PSEUDONYM_KEY`). Only spans and events from the relay's own code are exported over
  OTLP; dependencies log to stdout only. HTTP spans record the route template, never the URL,
  and the client address is never recorded. `journey_telemetry` runs a two-node journey at
  debug level and fails if any of 18 known secrets (ids, handles, names, message text, invite
  code, client address) appears in the exported OTLP bytes or the relays' stdout. The cell's
  collector deletes address, URL and user-agent attributes from anything that reaches it.
- **Data minimization.** Relay logs kept 30 days for catch-up, dedupe keys 7 days, presence
  never persisted, analytics aggregated and sampled with no identifiers.
- **Abuse (built, ADR 0020).** GCRA buckets that outlive sessions, per device (publish,
  typing, requests, blob KiB), per account (publish across devices, handle lookups, invite
  previews) and per client address (handshakes before any signature work, new accounts).
  Refusals say `slow down, retry in N s`; handshakes get `RateLimited`, and the client backs
  off at least that long. The address comes from an edge header clients can't forge there
  (`fly-client-ip`), so per-IP tiers are coarse; the device and account tiers carry the
  weight. Next: lower limits for accounts in their first days, and edge rate rules.
- **Bus (ADR 0019).** NATS subjects carry a keyed pseudonym of the identity, never the id;
  cells use disjoint subject prefixes; frames on the bus are the same signed or sealed bytes
  the WebSocket carries.
- **DDoS.** Cloudflare in front of the edge (WebSocket proxying, rate rules), L4 load
  balancers behind, connection shedding in the edge.

## Supply chain

`cargo-deny` (licenses, bans, advisories) and `cargo-audit` in CI; SBOM with syft per image;
images signed with cosign, SLSA provenance from GitHub Actions; `Cargo.lock` committed and
pinned actions by SHA; Renovate for updates; branch protection with required checks;
reproducible builds as a goal (static musl binaries, `SOURCE_DATE_EPOCH`).

## Client

App Attest at account creation and on risky operations (DeviceCheck fallback); jailbreak
signals raise risk but never block; app-switcher snapshot redaction; Keychain access group
shared with the Notification Service Extension; lock-screen notifications show "New message"
unless the user opts in; mini-apps in WKWebView with a strict CSP and no network except
through the bridge, with per-app grants; agents shown as declared readers with a visible
disclosure in chat info; one Cedar grant evaluator (`roda-grants`) compiled into the core and
the server.

## Agents

Content from messages, the web and mini-apps is untrusted input. Agents follow a quarantined
pattern: a privileged planner never reads untrusted text directly; a quarantined model
summarizes it into structured fields the planner can use, with spotlighting markers around
any quoted content. Every tool call passes Cedar grants and the safety gate (`roda-gate`).
Agent actions are signed events in the Space's log, so members see a tamper-evident audit
trail. Anything that would send data outside the Space needs the owner's confirmation.

## Privacy (LGPD and GDPR)

Data export from the device and the server (profile, devices, memberships, catalog
activity); deletion with cryptographic erasure (per-account keys for server-side encrypted
data are destroyed, which makes backups unreadable for that account); consent records as
signed events; a data-processing inventory in `docs/privacy-inventory.md` (later).

## Proof

| claim | test |
|---|---|
| the server stores only ciphertext | journey asserts no plaintext marker in Postgres, FDB or blobs (photo journey already does this for blobs) |
| profiles are readable only by contacts | `journey_profiles.rs`: a contact opens bio and photo, a stranger sees `@handle (hidden)`, Postgres has no name, bio or MIME type in `profiles` or `identities`, a blocked contact can't read the next version |
| logs carry no plaintext or identifiers | journey runs with logs captured and scanned |
| decoders survive garbage | cargo-fuzz targets on the protobuf decoder and the event verifier |
| KT detects tampering | a forged directory entry makes the client refuse the key |
| CI gates | cargo-deny, cargo-audit, fuzz smoke run, image signing in the pipeline |
