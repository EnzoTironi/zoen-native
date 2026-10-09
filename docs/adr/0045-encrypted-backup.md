# ADR 0045: Encrypted server backup

Status: accepted (built in `feat/encrypted-backup`)

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
  `zoen-backup-v1\0 ‖ identity`. The relay keeps one object per identity
  (`backups/{identity}`), and the newest upload replaces the old one.

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
   - **Guess limit.** Each OPRF evaluation for a restore counts as one guess, and so does
     each failed open. After **10** guesses without a success, the relay **destroys `k`**,
     and the backup can never be opened, not even by Zoen. This is WhatsApp's HSM rule. A
     correct password resets the counter.
   - **Why a leaked database isn't enough.** Without `k` an attacker can't run an offline
     dictionary attack against the verifier or `wrapped_key`. `k` exists only sealed under
     the vault master key, so they would need both the database and the master key.
2. **Recovery key (no server trust).** The device makes 64 random decimal digits, shown in
   groups of four. HKDF-SHA256 turns the digits into `wrap_key` and `auth_key`. The relay
   stores only the verifier and `wrapped_key`. With about 212 bits of entropy, the vault
   adds nothing, and this mode survives even the loss of the vault master key.

### The HSM boundary
`zoen_relay::backup::Vault` is the only code that touches `k`. It has three operations:
`new_key(identity) → sealed`, `evaluate(sealed, identity, blinded) → element` and
`destroy`.
- **Staging.** The `EnvVault` implementation seals `k` with XChaCha20-Poly1305 under
  `ZOEN_BACKUP_VAULT_KEY` (a Fly secret), with the identity as AAD. The plaintext `k` lives
  only inside one `evaluate` call.
- **Production.** The same trait gets an HSM or enclave implementation: AWS Nitro Enclaves
  or CloudHSM, or a Signal-style SVR cluster with several enclaves for the counter and the
  key. Then `k` never exists outside the hardware. Nothing else changes.
- **Not configured.** Without a vault key, the password endpoints answer `503` and
  recovery-key backups still work.

### Wire (HTTP on the relay, JSON unless stated)
- **Writes from a certified device.** These carry `x-zoen-device`, `x-zoen-ts` and
  `x-zoen-sig`, signed over
  `zoen-sync/2:backup:{relay}:{identity}:{op}:{sha256(body)}:{ts}`. The relay checks the
  device certificate against the identity.
  - `POST /v1/backup/oprf {blinded}` → `{evaluated}`. Starts a password setup: makes a new
    pending `k`.
  - `PUT /v1/backup/vault {mode, verifier, wrapped_key, kdf}` activates the pending `k`
    (password mode) or stores a recovery-key vault, and resets the counter. Changing the
    password or switching modes is another `PUT`.
  - `PUT /v1/backup/blob` (body: the sealed payload, ≤ 64 MiB).
  - `DELETE /v1/backup` turns the backup off. It forgets the vault and the object.
- **Restore (no device yet).**
  - `POST /v1/backup/restore/start {handle, blinded?}` → `{identity, mode, kdf,
    evaluated?}`. The password mode counts a guess.
  - `POST /v1/backup/restore/open {handle, auth_key}` → `{wrapped_key, size}`. A failed
    open counts a guess.
  - `GET /v1/backup/restore/blob?handle=…` with header `x-zoen-backup-auth: auth_key`.
  - **No existence oracle.** For a handle without a backup (or an unknown one), `start`
    answers like a password vault, with an evaluation under a key derived from the master
    key and the handle. Every `open` for it then fails the same way. The relay limits these
    by IP and by handle.

### After a restore
The device installs the restored tables, writes the root and agreement secrets to its vault,
makes a **new device key**, and certifies it with the restored identity. It then connects
like any new device of that person.
- **What works right away.** History, contacts, profiles and readable Spaces.
- **End-to-end Spaces.** New messages there need the group to add the new device. That's
  M2's device linking: the old device's leaf is dead, and the 30-day ceiling removes it.
  Until then, new sealed entries stay sealed on this device.

## Consequences
- **Privacy.** Plaintext and `K` never leave the device. The relay learns that a backup
  exists, its size and when it changed. A restore attempt tells it someone typed a handle
  and nothing about the password.
- **Cost.** One object per person: a text history is a few MB, and media is excluded.
  Storage is about US$0.0001 per user per month on R2 or Tigris. The vault does one
  ristretto255 scalar multiplication per guess (about 50 µs).
- **Loss.** Forget the password and use up 10 guesses, or lose the recovery key, and the
  backup is gone. This is by design, the same as WhatsApp. The app has to say so plainly,
  without jargon.
- **Later.** The automatic schedule (daily on Wi-Fi), chunked uploads past 64 MiB, a media
  backup option, the HSM implementation, and a Signal-style SVR with distributed counters.

## Tests
`crates/zoen-cli/tests/journey_backup.rs` covers these journeys:
- turn on a password backup, lose the phone, restore on a new device, read the history,
  keep chatting;
- a wrong password is refused, and 10 wrong ones lock the backup forever;
- restore with a recovery key;
- the relay database holds no plaintext.
