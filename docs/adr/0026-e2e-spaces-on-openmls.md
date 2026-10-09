# ADR 0026: End-to-end Spaces on OpenMLS, with member-signed checkpoints

Status: accepted (M2)

## Context
`Privacy::EndToEnd` exists in the model and the wire has reserved `Sealed { kind, suite,
data }` since protocol v2, but every Space today is relay-readable. M2 makes end-to-end Spaces
real. The relay still orders, checks membership and fans out, and stores only ciphertext for
everything people say. Two questions shape it:

1. **Where MLS state lives and how members are identified**, without a second key system next
   to ADR 0003's identity and device keys.
2. **What stops the relay from lying about history.** It can't read an E2E Space, but it
   orders it, so it could show different members different logs (a fork), drop entries, or keep
   ciphertext forever. MLS protects content, not the shape of the log.

## Decision

### Library and suite
- `roda-mls` over **OpenMLS 0.9** (MIT) with the RustCrypto provider (pure Rust, builds for
  iOS and the simulator), ciphersuite **`MLS_128_DHKEMX25519_CHACHA20POLY1305_SHA256_Ed25519`
  (0x0003)**. The suite id travels in `Sealed.suite`, so a future suite (post-quantum KEM) is a
  new value, not a new format.

### The leaf is the device
- Every device is one MLS leaf. Its **signature key is the device's existing Ed25519 key**,
  and its `BasicCredential` is `zoen-leaf/1` + identity + device + the identity's certificate
  over the device. A member accepts a leaf only if the certificate verifies under the identity
  and the leaf's signature key is that certified device key. "Members" stays "identities with
  their devices", and revoking a device (ADR 0003) is removing its leaf. The same key now signs
  envelopes, logins and MLS messages under different domain labels (`zoen` content tag,
  `<protocol>:auth:`, MLS `SignWithLabel`), so no signature can be replayed across them.

### State on the device, sealed
- MLS state uses `openmls_sqlite_storage` tables in the device database next to `roda-store`.
  Their migrations are applied at open, so state changes commit in the same database as the
  log. Group state is full of secrets, and ADR 0003 keeps secrets out of SQLite. So every value
  is **sealed with XChaCha20-Poly1305 under a 32-byte state key**, derived from the device's
  Ed25519 secret by HKDF (`zoen-mls-state-key/1`). It lives exactly where that secret lives
  (the Keychain), needs no vault entry or app change of its own, and dies with the device key.
  The storage codec has no instance to hold a
  key, so `roda-mls` puts the key in a thread-local scope for exactly the span of each
  (synchronous) OpenMLS call, and the codec fails closed outside one. The same codec encodes
  row keys (group ids, key package refs), and lookups need them stable, so the nonce is
  synthetic: HMAC-SHA256 of the plaintext under a second HKDF subkey (SIV). Keys and values
  are both sealed; the database shows only which stored items are equal.

### Key packages
- A device publishes 32 single-use key packages plus one last-resort package once it is
  online. The relay keeps them in Postgres (`key_packages`): at most 200 single-use packages
  per device, and one last-resort package that a new one replaces. At most 100 packages per
  op, 4 KB each. A claim is one statement per device: it deletes the oldest single-use
  package (`FOR UPDATE SKIP LOCKED`) and otherwise returns the last-resort one, kept. Claims
  are rate-limited like account lookups. Ops: `PublishKeyPackages`, `ClaimKeyPackages`.
- The relay verifies each package (signature, suite) and that its credential names the
  publishing identity and device, so nobody can publish keys for someone else. The claimer
  checks the leaf again and drops any package for a device it didn't ask for.
- Every leaf advertises the `last_resort` extension in its capabilities. Without it the
  last-resort package fails RFC 9420 validation (the first journey caught exactly that).

### What goes in the log
An E2E Space is an ordinary relay-ordered log, so the relay's total order is the MLS epoch
order.
- **Public control events stay plaintext, as today:** `SpaceCreated` (privacy `EndToEnd`),
  `MemberAdded`, `MemberRemoved`, `ProfileKeyShared` (already sealed to its recipients, ADR
  0016) and `Checkpoint` (below). The relay keeps checking roles and membership on these, as
  in any Space.
- **Everything people say is `Sealed`.** `Application` carries an MLS PrivateMessage whose
  plaintext is a normal signed inner event (same bytes as a relay-readable Space). A
  decrypted message is therefore an ordinary `Event`, and the store, the UI and agents need no
  second path. Authenticity is end to end twice: the MLS sender and the device signature on
  the inner event.
- **Membership is two steps, member-checked.** The admin publishes `MemberAdded` (relay
  checks the role), then a `Sealed::Commit` with the Add, and a `Sealed::Welcome` for the new
  devices right after it in the same log. Members apply a commit only if every identity in the
  group it leaves is listed in the log at that point (a **subset**, `MlsError::Unlisted`
  otherwise), so a relay can't slip in a reader. People listed but not yet in the group don't
  block it: an add without a commit fails closed (listed, but can't read), and a member whose
  own commit is refused clears it. A Welcome whose roster disagrees
  with the log is refused, and its key package is spent with it. The adder re-adds from a
  fresh package; no one can retry the bad Welcome. Removal is the mirror image.
- **Who commits:** the log is the intent and the group follows it. Any Owner or Admin device
  that sees a difference (listed but not in the group, or in the group but no longer listed)
  claims packages for the missing devices and puts all of it in one Commit plus Welcome. The
  first commit for the epoch wins (below), so several admins reconciling at once converge.
  A device's own commit comes back from the relay as its own message: at the current epoch
  with a commit pending it merges it, and anything else is stale.
- **Batching:** a device doesn't reconcile while membership events of its own for that Space
  are still on their way to the relay. Creating a group with five people is one commit and
  one Welcome, never five epochs, and a commit never gets ahead of the log entries it
  depends on.
- **Removal:** after a `MemberRemoved` lands, **no device seals** for that Space until the
  commit taking the person out has landed too. This applies to every member, not only the
  admin who will commit, because a message sealed one epoch early is readable by the person
  just removed. The relay stops sending the Space to them, but serves their catch-up
  *through* their own removal (`("s", space, "gone", identity) -> seq`, cleared if they are
  added back). Their device sees the removal and deletes the group (`Device::forget`). It
  never receives the commit, it keeps no secrets, and a later Welcome starts it fresh. A
  member leaving on their own is removed the same way: the group holds sends until an owner
  or admin device commits. That is the price of the guarantee, and it is visible as
  *Sending*.
- **Concurrent commits:** every member applies the first commit for the current epoch in log
  order and ignores later ones for an epoch already gone. The losing author sees its commit
  skipped, processes the winner, and re-proposes. The relay rejecting stale epochs early
  (`stale_epoch`, from the epoch in the clear MLS framing) is an optimization on top. It
  saves log space, and correctness doesn't depend on it.
- The relay **rejects any plaintext body outside the control set** in an E2E Space, so a
  buggy or old client can't leak into one.

### Member-signed checkpoints
A `Checkpoint { upto: Seen, epoch, epoch_digest }` is a plaintext control event signed by a
member device. `upto` is the chain position (seq, hash) as that member applied it. And
`epoch_digest = SHA-256("zoen-checkpoint/1" ‖ group id ‖ epoch ‖ epoch_authenticator)`: only a
member who holds the epoch's key schedule can compute it, and it reveals nothing about the
secrets.
- **When:** a device checkpoints after its own commit lands, after it joins, and after 256
  sealed entries since its last checkpoint. Members who only apply someone else's commit don't
  post one: the committer's and the joiners' checkpoints already pin that epoch, and the
  cadence keeps checkpoints at O(commits + messages/256), not O(members × commits).
- Each device records its own digest per epoch. A checkpoint whose digest differs marks the
  Space forked on that device.
- **What members check:** for every checkpoint they apply, `upto.hash` must equal their own
  chain at `upto.seq`, and `epoch_digest` their own digest for that epoch. A mismatch is
  signed evidence that two members saw different histories or different groups: the relay
  forked the Space, or a member was fed a different commit. The client marks the Space broken
  and says so (`verify`), never silently "heals".
- **What the relay checks:** `upto` must match its own chain, so a member can't post a
  checkpoint on a history the relay doesn't have.
- **What it buys the relay:** a checkpoint is a signed receipt. Ciphertext below the lowest
  `upto` across the current members' devices is held by every one of them and can be pruned.
  Catch-up for those devices starts there. Storage is the largest line of ADR 0022's 1B
  estimate, and the 30-day hot window becomes "until every member has it". New members don't
  need it (forward secrecy: they read from their Welcome on).

## First journey (the M2 milestone)
`journey_m2` (passing): Ana, Bruno and Carla sign up through the real CLI, relay, Postgres and
FoundationDB, and each device publishes 33 key packages. Ana creates an E2E group with Bruno:
her device claims one of his packages and commits the Add and the Welcome. Both send messages
and read each other's. Every CLI call is a new process, so each read reloads MLS state from
the sealed device database. The relay's log holds only the control events, the checkpoints and
four sealed entries (commit, welcome, two messages). Neither FoundationDB nor any Postgres
table holds the messages, as text or as hex. A raw client's plaintext message into the Space
is refused. Both devices are at epoch 1 with the same digest, the two checkpoints (Ana after
her commit, Bruno after joining) agree, and `verify` re-checks every signature and the chain
over ciphertext. Then Carla joins by invite; Ana's device commits her in (epoch 2), and Carla
reads what is said after her Welcome and nothing before it.

Later M2 steps, in order: removal (a removed member can't read what follows), concurrent
commits and the relay's `stale_epoch`, pruning below checkpoints, linking a second device, the
app on the simulator with the Notification Service Extension sharing state.

### Not yet (tracked in the plan)
- Topping up key packages: today a device publishes once, and claims fall back to the
  last-resort package when the 32 run out. The relay will say "low" and the device refills.
- Several devices per identity: reconcile adds every device with packages, but linking a
  second device to existing groups is the multi-device step.
- Done since: DMs and new groups are end-to-end by default, M1 Spaces upgrade one way with
  `SpaceEncrypted`, and messages are sealed at send time at the current epoch, which also
  closes the "outbox older than 4 epochs" gap (ADR 0027).

## Consequences
- The relay keeps seeing who talks in which Space, when and how much: author, device, Space,
  size, membership. MLS hides content, not metadata. Closed Spaces (relay-readable) stay for
  communities, moderation and server-side agents.
- New members read from their join onward. History from before the join needs a separate
  device-to-device transfer (the multi-device step), not relay storage.
- Every member device does MLS work per commit (O(log n) with the ratchet tree). Groups are
  capped at 1,000 members (plan D10); above that, a Space is Closed or a channel.

## At 1B users
- **Key packages:** 1.6B devices × (100 + 1) packages × ~400 bytes ≈ 65 TB in Postgres,
  partitioned by identity. Claims are single-row updates. Devices top up when the relay says
  they're under 20.
- **Relay CPU:** for application messages it doesn't change. A sealed envelope is routed like
  any other, and the relay never touches MLS crypto. The relay's only MLS parsing is the
  epoch in the framing, a few bytes.
- **Storage:** ciphertext is about 1 KB per message (the plan's model already assumed MLS
  overhead). Checkpoint pruning cuts hot storage from 30 days to the slowest member's lag,
  hours for most Spaces, a large share of ADR 0022's $270k/month storage line.
- **Device CPU:** a commit in a 1,000-member group costs a member about 10 path-secret
  derivations and HPKE operations, milliseconds on a phone.
