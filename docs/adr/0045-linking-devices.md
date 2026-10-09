# ADR 0045: Linking a second device, with its history

Status: accepted (M2)

## Context
An account is one identity key (ADR 0003) with device keys certified by it; each device is
its own MLS leaf (ADR 0026). People want the same Zoen on their phone and their computer,
with the **same message history** on both (Enzo, 2026-10-09). The relay must never see
plaintext, and pruning (ADR 0026) must not take away history a new device still needs.

## Decision

### Flow (Signal-style)
1. **New device** — `zoen link-request` / `RodaEngine::link_request`: makes a device key, an
   HPKE (X25519) key pair and a 32-byte one-time **link secret**, and shows them as a code
   (`zoen-link:1:<device pub>:<hpke pub>:<secret>`, a QR in the app) with six check digits.
2. **Existing device** — `zoen link CODE` / `link_device`: shows the same check digits (the
   person compares them), certifies the new device key with the account identity key, and
   uploads an **identity box** sealed to the new device: identity secret, agreement secret,
   the device certificate, profile, profile key and relay. `DeliverLink` carries that
   certificate alongside the identity box. The relay verifies the certificate and holds
   the sponsor's active device row through one transaction that enrolls the target and
   stores the box; both commit or neither does. The existing device records the target as
   active locally and posts `DeviceJoining { device }` in every end-to-end Space it is in.
3. The new device polls the relay for its box (`FetchLink`, under its temporary
   self-certified identity), opens it, becomes the account and logs in with its enrolled
   device key, publishing key packages. Fetching the box remains allowed after enrollment
   while the device still has its temporary identity; a revoked key is refused.
4. The existing device (and any other device of the account, or an admin) adds the new leaf
   to every group from a fresh key package (the same path as ADR 0026's rejoin).
   `link_progress` reports groups done / total.
5. Once the new device is in every group, the existing one sends the **first history
   bundle** (below).

### Enrollment and upgrades
- A registered account accepts only devices already enrolled and active in the relay's
  directory. A valid root certificate alone cannot create a device row. First account
  registration atomically creates the identity and its first device; later profile
  updates require an enrolled active device in that same transaction.
- Linking enrolls a device through its active sponsor. Backup recovery (ADR 0046) can
  enroll one through `POST /v1/backup/restore/enroll`, with the current backup generation,
  backup recovery authority, root certificate and the new device's signed proof. Neither
  path revives an existing revoked device row.
- Protocol 4 makes this enrollment mandatory. Versions 2 and 3 receive `UpgradeRequired`
  before authentication: version 3 sponsors sent a box without its certificate and could
  report success while leaving the target unable to log in. Later history boxes omit
  the certificate because they do not enroll a device.
- The handshake upgrade preserves the `zoen-sync/2` signing domain, signed event formats,
  stored history and local event metadata. It requires upgrading clients and relays, not
  deleting the database.

### Crypto
- HPKE (RFC 9180) **PSK mode**, DHKEM(X25519, HKDF-SHA256), HKDF-SHA256, ChaCha20-Poly1305;
  `psk` = link secret, `psk_id` = `zoen-link-v1`, `info` = `zoen-link-v1:<context>`. Only a
  holder of both the new device's HPKE secret and the QR's secret opens anything, so a relay
  that swaps keys learns nothing, and a box made for another code doesn't open.
- Boxes on the relay (`link_boxes`, ≤64 KB, single fetch, 15-minute TTL) are addressed by
  `SHA-256("zoen-link-box-v1\0" ‖ secret ‖ label)`: unguessable, and the relay never sees
  the secret.
- Afterwards the two devices keep the link secret and each other's HPKE key as a **peer**
  (in the vault) for device-to-device messages (`SendDevice` → `DeviceMessage`, fanned out to
  the identity's online sessions only), each sealed with the same HPKE setup.

### History, paginated
- **First bundle** — the recent window: the last `ZOEN_HISTORY_RECENT` (default 200)
  messages of every chat, plus the chat list and metadata the log already carries. Each
  message travels as its original signed sealed entry together with its opened content.
  The bundle is serialized, split into chunks (`ZOEN_TRANSFER_CHUNK`), each sealed with
  ChaCha20-Poly1305 under a fresh random key (nonce = chunk index, aad `zoen-history-v1`),
  and uploaded to the relay's object store (`PUT /v1/transfer/{id}/{n}`, signed by the
  device). The manifest (transfer id, key, chunk hashes) goes in a second link box (label
  `history`), sealed like the identity box.
- **Download** with progress; every chunk is checked against its hash and kept locally, so
  an interrupted transfer **resumes** where it stopped. When all arrive the device imports
  the bundle and deletes the transfer from the relay (`DELETE /v1/transfer/{id}`, signed).
- **Older pages on demand** — on scroll or search the new device asks the device that linked
  it for the page before its oldest opened message (`ZOEN_HISTORY_PAGE`, default 50) with a
  device message; the answer comes back the same way, sealed to it. If that device is
  offline the app shows "Abra o Zoen no celular para carregar mensagens antigas." and the
  CLI prints it (`zoen read CHAT --older`).
- **Media** — attachments already live encrypted in the object store under keys carried
  in the messages; the bundle carries the message (with its thumbnail reference) and the new
  device fetches full files lazily, as any member does.
- **Import is verified**: an opened message replaces a local undecryptable entry (or a
  pruned stub) only if its original wire bytes hash to that entry's signed hash and the
  author's signature checks. The primary can't invent history for the new device, and the
  relay can't either.

### Pruning safety
- Pruned stubs keep the original entry's signed hash (ADR 0026), so the new device can
  restore any pruned entry from the bundle or a page and verify it; pruning never removes
  anything the transfer needs, because the transfer comes from the primary's local copy.
- `DeviceJoining` makes the relay hold pruning for the new device from that seq until it
  checkpoints, so the ciphertext it gets after joining stays on the relay until it has it.

### Unlinking
`zoen unlink DEVICE` / `unlink_device`: the relay commits revocation for that device row,
closes its sessions and refuses subsequent authentication, request admission and outgoing
deliveries for that key. A fresh root-certified key also needs authorized enrollment; the
revoked device cannot enroll it with its root secret alone. The account's other devices
remove the revoked leaf from every group. Removing a member from a Space removes all of
their leaves.

Work admitted before the revocation commit may finish, including an already queued log
append. Postgres revocation and FoundationDB append use separate transaction domains;
this is a request-admission boundary, not atomic ordering of revocation and log commits.

## Consequences and limits
- **The identity key moves** to the new device. Explicit enrollment closes reentry with
  that root secret alone after unlinking at this relay. It does not erase copied secrets
  or rotate the root key; cryptographic root compromise remains a separate limitation.
- A compromised device can enroll extra keys while it is still active. Keys enrolled
  before unlinking remain active until each is revoked. Valid backup recovery authority
  also remains an account recovery path after an individual device is unlinked; device
  revocation does not revoke that authority.
- **Future option**: keep the root key on one device (or split it) and authorize linking
  without copying it. Root rotation is not built. Other relays must enforce the same
  enrollment directory to provide the same revocation boundary.
- Older pages need the primary device online; if it is lost, only the first bundle remains.
- A transfer abandoned mid-way stays in the object store (no sweep/expiry yet).
- The same account on two devices means two leaves in every group; the MLS cap counts
  devices, not people.

## Verified by
`journey_m2::a_linked_device_gets_the_history_and_loses_access_when_unlinked`: pruned
pre-link history, link with matching check digits, an interrupted and resumed transfer,
chunks deleted afterwards, the recent window first, older pages while the phone is online,
the offline state, new messages on both devices, no plaintext in FoundationDB, Postgres or
the object store, and an unlinked device refused at login and blind to new messages.

`session_revocation` covers fresh certificates minted by an unlinked root holder,
pre-opened registration races, atomic enrollment with box delivery, enrollment retries
alongside backup writes, and protocol 3 refusal before authentication or linking.
