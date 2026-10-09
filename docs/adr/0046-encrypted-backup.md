# ADR 0046: Encrypted server backup

Status: accepted as a development implementation; password recovery is not ready for public activation

## Context
A phone that's lost, stolen or wiped takes the account with it. The identity key
(`zoen.identity.v1`) and the history only live on the person's devices. Multi-device linking
(M2) covers "I still have another device". It doesn't cover "I have nothing left". WhatsApp
answers this with an optional end-to-end encrypted backup. You protect it with a password,
which an HSM-backed key vault guards with a guess limit, or with a 64-digit key that only
you keep. Signal's SVR follows the same idea.

Requirements, from Enzo's mandates:
- The relay never sees plaintext. It can't read the backup, and it can't brute-force the
  password offline even if its whole database leaks.
- It's optional and off by default. Turning it on takes one choice.
- It scales to 1B users. A backup costs one small object and one row. The vault does one
  group operation per guess.

## Decision

### What's in a backup
- **Payload.** The format is `ZOENBK1\0 ‖ u32 header_len ‖ header JSON ‖ SQLite bytes`.
  - The header holds the identity id, the relay, the identity root secret, the agreement
    secret, the creation time and the payload version.
  - The SQLite part is a `VACUUM INTO` copy of the device database. It keeps only the
    tables a new device can use: `identities`, `events`, `meta`, `profile_keys`,
    `peer_agreement_keys`, `blocked`, `synced_spaces` and `media_keys`.
  - It drops everything tied to the old device: the MLS group state (sealed under the old
    device key), the outbox, blob-upload bookkeeping, the search index, and `mls.*`,
    `invite:*` and `account` meta.
  - Media bytes stay out. Their encrypted copies are already on the relay
    (content-addressed, ADR 0007), and `media_keys` brings back the keys to open them.
- **Sealing.** A random 32-byte backup key `K` encrypts the payload with
  XChaCha20-Poly1305. The stored form is `nonce ‖ ciphertext`, with AAD
  `zoen-backup-v1\0 ‖ identity`. The relay keeps one active object pointer per identity.
  Each upload uses a fresh immutable object path; activation switches the pointer before
  bounded cleanup of the old object. Legacy `backups/{identity}` objects remain readable.

### Two ways to protect `K`
1. **Password (HSM-guarded).** The password needs 8 or more characters.
   - **OPRF.** The relay holds a per-person OPRF key `k` and runs 2HashDH over
     ristretto255, the same construction as RFC 9497 OPRF mode 0. The device sends
     `B = r·H(pw)`. The relay answers with `k·B` and never sees `pw`. The device computes
     `rwd = SHA-512(pw ‖ r⁻¹·k·B)`.
   - **Stretching.** Argon2id(rwd, salt = identity, m = 64 MiB, t = 3, p = 1) gives
     32 bytes. HKDF-SHA256 splits them into `wrap_key` (which seals `K` as `wrapped_key`)
     and `auth_key`.
   - **What the relay stores.** It keeps `sha256(auth_key)` (the verifier), `wrapped_key`,
     `k` sealed under the vault master key, the KDF parameters, and a guess counter.
   - **Development guess limit.** Each OPRF evaluation for a restore counts as one guess, and so does
     each failed open. After **10** guesses without a success, the relay **destroys `k`**,
     and the current restore path cannot open the backup. A correct password resets the
     counter. This unauthenticated ceremony is unsafe for public activation: an outsider
     who knows a handle can exhaust all ten attempts without trying a password.
   - **Why a leaked database isn't enough.** Without `k` an attacker can't run an offline
     dictionary attack against the verifier or `wrapped_key`. `k` exists only sealed under
     the vault master key, so they would need both the database and the master key.
2. **Recovery key (no server trust).** The device makes 64 random decimal digits, shown in
   groups of four. HKDF-SHA256 turns the digits into `wrap_key` and `auth_key`. The relay
   stores only the verifier and `wrapped_key`. With about 212 bits of entropy, the vault
   adds nothing, and this mode survives even the loss of the vault master key.
   Failed authentication never spends a permanent guess counter or locks this mode.
   Per-IP and per-handle network limits still apply. Correct key authentication also
   clears a recovery-key lockout left by an older relay.

### The HSM boundary
`zoen_relay::backup::Vault` is the only code that touches `k`. It creates sealed keys,
evaluates blinded points, and provides independent decoy evaluations and metadata.
- **Staging.** The `EnvVault` implementation seals `k` with XChaCha20-Poly1305 under
  `ZOEN_BACKUP_VAULT_KEY` (a Fly secret), with the identity as AAD. The plaintext `k` lives
  only inside one `evaluate` call.
- **Unfinished production gates.** Password restore needs independent, identity-bound
  authorization before any real OPRF evaluation or destructive counter change. A surviving
  device alone cannot provide lost-all-devices recovery. An enrolled, externally recoverable
  factor and its recovery journeys remain to be designed and built. The HSM or enclave
  implementation must enforce the key and counters across nodes and prove its destruction
  semantics. Clearing a sealed key from a staging database does not erase historical copies.
- **Default off.** A vault key alone never enables password setup, password uploads or
  password restore. The current ceremony requires both `ZOEN_BACKUP_VAULT_KEY` and
  `ZOEN_DEV_ALLOW_UNAUTHENTICATED_PASSWORD_BACKUP=1`. The latter is exclusively an explicit
  opt-in for development and tests, not a production readiness flag. With it absent,
  password requests answer `503` without evaluating or spending guesses. Existing snapshots
  and keys are preserved, and recovery-key backups still work. Production activation
  remains blocked by the independent recovery authorization and HSM gates above.

### Wire (HTTP on the relay, JSON unless stated)
- **Writes from a certified device.** These carry `x-zoen-device`, `x-zoen-ts` and
  `x-zoen-sig`, signed over
  `zoen-sync/2:backup:{relay}:{identity}:{op}:{sha256(body)}:{ts}`. The relay checks the
  active device against the directory. A database authorization lock prevents a write
  from committing after unlink has completed.
  - `POST /v1/backup/oprf {blinded, generation}` → `{evaluated}`. Starts a password setup.
    The pending `k` belongs to that device and configuration generation.
  - `PUT /v1/backup/vault {mode, verifier, wrapped_key, kdf, generation}` stages the
    replacement configuration. The existing vault and object remain restorable.
  - `PUT /v1/backup/blob` has binary body `ZOENBG1\0 || generation[32] || sealed payload`.
    The payload is at most 64 MiB. The signature covers its generation too. Upload writes
    an immutable object with a fresh server-generated key on every attempt, then
    atomically activates its matching staged configuration or
    updates the matching active one. A stale device gets `409` and must configure backup
    again. Per-identity database locks serialize writes across relay nodes.
  - `DELETE /v1/backup` turns the backup off. It forgets the vault and the object.
  - **Bounded waits.** `ZOEN_BACKUP_STORAGE_TIMEOUT_MS` defaults to 15000 for each PUT,
    complete GET including its body, and DELETE. `ZOEN_BACKUP_REQUEST_TIMEOUT_MS` defaults
    to 30000 for the full HTTP operation. `ZOEN_BACKUP_LOCK_TIMEOUT_MS` defaults to 5000
    for database lock acquisition. Values must be 1–120000 ms, storage must be below the
    request budget, and the lock budget must not exceed it. Each authorization transaction
    also sets PostgreSQL statement and idle-in-transaction timeouts to the request budget,
    so cancellation cannot leave device or restore locks held indefinitely. A timed-out
    PUT never publishes its new object pointer, even if the cloud write finishes later.
    Cleanup runs after commit and is bounded; failed or uncertain cleanup and orphaned PUTs
    still require an operations-tested garbage collector before full-scale completion.
- **Restore (no device yet).**
  - `POST /v1/backup/restore/start {handle, blinded?}` → `{identity, mode, kdf,
    evaluated?, generation}`. The development password mode counts a guess.
  - `POST /v1/backup/restore/open {handle, auth_key, generation}` → `{wrapped_key, size,
    generation}`. A failed development password open counts a guess. Row locks keep checks and counters on
    the same configuration; a rotation requires restarting restore.
  - `GET /v1/backup/restore/blob?handle=…&generation=…` with header
    `x-zoen-backup-auth: auth_key`.
  - `POST /v1/backup/restore/enroll {handle, auth_key, generation, device, cert, sig}`
    enrolls the fresh root-certified device only after recovery authentication. Its
    device signature covers the identity, device, certificate and current generation,
    under the `backup-device-enroll` domain. The authority check and enrollment share
    one bounded database transaction. Safe retries accept the same active key;
    revoked or conflicting keys are never reactivated. A root certificate alone cannot
    enroll a device into an existing account.
  - **No existence oracle.** For a handle without a backup (or an unknown one), `start`
    answers like a password vault, with an evaluation under a key derived from the master
    key and the handle. Every `open` for it then fails the same way. The relay limits these
    by IP and by handle. Decoy identity/generation fields use a separate secret PRF,
    independent of OPRF evaluations. A known account without backup keeps its public
    identity. Failed authentication returns the same refusal regardless of generation.

### After a restore
The device decrypts the snapshot, validates the identity root, makes a **new device key**,
and certifies it with the restored identity. It proves control of that key and current
recovery authority at `restore/enroll` before installing the tables and secrets. It then
connects using the same enrolled key. A refused enrollment leaves the local account empty.
- **What works right away.** History, contacts, profiles and readable Spaces.
- **End-to-end Spaces.** Restore persists a signed `DeviceJoining` request in the durable
  outbox for each encrypted Space in the snapshot where the identity is a member. A
  surviving group admin adds the new device's leaf and sends its Welcome. The new device
  then receives and sends encrypted messages while keeping its backed-up history. The
  requests survive a relaunch; the relay still checks current membership, so a stale
  backup cannot restore access after removal.
- **Lost-device revocation.** The recovered account can use `unlink` to revoke the old
  device and remove its MLS leaves. Its key is refused at every backup write endpoint.
  Knowing the identity root alone cannot mint another relay device after revocation.
  Valid recovery authority can deliberately enroll a different fresh device; keys
  enrolled before revocation and recovery-authority rotation need their own lifecycle.
- **Upgrade.** Existing vaults and their original object paths remain readable.
  Migrations `0021` and `0023` preserve `0020` and assign legacy wrappers an opaque
  generation. A successful restore binds its verified key to that generation. Existing
  devices without a stored generation must configure backup again before upload.
- **Remaining recovery gate.** Joining waits for an authorized device that still has the
  group's MLS state. Recovery when no such device survives needs a protocol design and
  its own failure journey. The snapshot deliberately excludes old MLS state.

## Consequences
- **Privacy.** Plaintext and `K` never leave the device. The relay learns that a backup
  exists, its size and when it changed. A restore attempt tells it someone typed a handle
  and nothing about the password.
- **Cost.** One object per person: a text history is a few MB, and media is excluded.
  Storage is about US$0.0001 per user per month on R2 or Tigris. The vault does one
  ristretto255 scalar multiplication per guess (about 50 µs).
- **Loss.** The development password ceremony can disable a backup after ten public
  attempts and must remain gated. A recovery-key backup remains usable after bad attempts;
  losing the 64-digit secret itself prevents recovery.
- **Later.** The automatic schedule (daily on Wi-Fi), chunked uploads past 64 MiB, a media
  backup option, the HSM implementation, and a Signal-style SVR with distributed counters.

## Tests
`crates/zoen-cli/tests/journey_backup.rs` covers these journeys:
- turn on a password backup, lose the phone, restore on a new device, read the encrypted
  history and continue in a readable group;
- a wrong password is refused, and 10 wrong ones lock the backup forever;
- restore with a recovery key;
- restore by password or recovery key, rejoin an encrypted group through a surviving
  admin, exchange new messages, revoke the lost phone and verify signed history;
- a stale backup retains old history but cannot regain removed group membership;
- the relay database holds no plaintext.
- linked devices with different backup keys cannot overwrite the newer backup;
- interrupted configuration keeps the previous copy restorable;
- every backup write refuses a revoked device;
- a generation-less backup restores and can be upgraded without losing its old copy.
- more than ten bad recovery-key attempts cannot lock a backup, and a correct key heals
  lockout from an older relay;
- a vault key without the development opt-in rejects password setup, upload and restore
  while preserving an existing snapshot and counter.
- contested database locks and incomplete request bodies terminate within configured
  budgets without mutating the existing snapshot;
- a local S3-compatible endpoint proves stalled PUT acknowledgements, stalled GET bodies,
  and stalled cleanup are bounded, Unlink completes, and a late PUT remains an orphan.

The backup decision was originally numbered 0045, also used by device linking. It is now 0046. The existing `0020_backups.sql` migration retains its historical comment so its SQLx checksum remains unchanged.
