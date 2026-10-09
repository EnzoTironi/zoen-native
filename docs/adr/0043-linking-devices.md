# ADR 0043: Linking a second device, with its history

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
   the device certificate, profile, profile key and relay. It records the device as active
   and posts `DeviceJoining { device }` in every end-to-end Space it is in.
3. The new device polls the relay for its box (`FetchLink`, allowed before login), opens it,
   becomes the account and logs in with its own certified device key, publishing key
   packages.
4. The existing device (and any other device of the account, or an admin) adds the new leaf
   to every group from a fresh key package (the same path as ADR 0026's rejoin).
   `link_progress` reports groups done / total.
5. Once the new device is in every group, the existing one sends the **first history
   bundle** (below).

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
`zoen unlink DEVICE` / `unlink_device`: the relay marks the device revoked (`Unlink`; Hello
then refuses it) and the account's other devices remove its leaf from every group, so it
reads nothing new. Removing a member from a Space removes all of their leaves.

## Consequences and limits
- **The identity key moves** to the new device. A compromised linked device holds the whole
  account until the identity key is rotated (not built).
- **Future option**: per-device keys cross-signed by a long-term key kept on one device (or
  split), so linking never moves the account key; unlinking would then revoke a signature
  instead of trusting a device to forget. The protocol keeps device keys separate already, so
  this is a client change plus a cross-signing record.
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
