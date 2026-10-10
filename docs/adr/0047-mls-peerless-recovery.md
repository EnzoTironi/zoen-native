# ADR 0047: MLS recovery without a surviving device

Status: development implementation; production gates below remain open.

## Problem

Backup restores identity and agreement keys, confirmed history and membership. It makes
a fresh enrolled device and deliberately excludes the old device's MLS secrets. A
Welcome from an online administrator cannot help when every device in the group is gone.

MLS external commits allow a new device to join using authenticated public GroupInfo.
The mechanism is specified in [RFC 9420 §12.4.3](https://www.rfc-editor.org/rfc/rfc9420.html#section-12.4.3).
The pinned OpenMLS 0.9.1 implementation provides the external commit builder; its
`finalize` installs operational group state before the relay confirms anything. The
device therefore needs a durable pending marker outside the library's own pending
commit state.

## Publication and privacy

Every commit made by an upgraded native client publishes the next epoch's complete
GroupInfo, including its public ratchet tree, as an encrypted content-addressed blob.
No epoch secrets or old device private keys are exported. A fresh random content key
encrypts GroupInfo with XChaCha20-Poly1305. A separate ephemeral X25519 exchange and
HKDF-SHA256 key box wraps that key to each current identity's verified agreement key.
Space, epoch, publishing device and recipient identity bind the authenticated encryption.
The backed-up agreement secret opens the box after authenticated device enrollment.
The relay sees membership identities it already knows; it cannot read the device tree.

The signed v4 sealed header carries `Sealed.recovery` (protobuf tag 5): version 1, next
epoch and lowercase SHA-256 blob address. This small reference survives authenticated
pruning. The relay checks that only a Commit carries it and that its epoch follows the
commit framing. Clients verify the full blob hash, authenticated encryption, GroupInfo
signature under the publishing device, tree, device certificates, group ID, ciphersuite,
epoch and current identity roster before installing a group.
An external commit's UpdatePath leaf is also checked against the current roster and
the admitted outer envelope's identity and device before merging. A listed account
cannot wrap an unlisted leaf or borrow another device's enrollment. Recovery references
are read only from authenticated v4 headers. Legacy tag-5 pruning stubs remain readable
for their historical signature chain but cannot install or invalidate a recovery context.

Public context is limited to 8 MiB and 5,000 identity key boxes. Fixed-size packed boxes
bound decoder allocations; the complete blob is limited to 8 MiB + 680,128 bytes. Downloads
check Content-Length when supplied and enforce the bound while reading every chunk.
These are wire and allocation input bounds, not measured process RSS or throughput.

## Atomicity and ordered recovery

1. A SQLite transaction stages MLS state, the encrypted blob upload, its unconfirmed
   upload receipt and the exact signed Commit/Welcome outbox entries. A failed write
   rolls them all back. Nothing is published from partially staged state.
2. HTTP success atomically confirms the receipt and removes the upload queue row.
   A 400/413 cannot confirm a recovery upload. A timeout or lost response retries the
   same content address. The outbox sends the Commit only with a confirmed receipt;
   its Welcome waits for the Commit's ordered confirmation.
3. After catch-up, a current non-reader with no group fetches the latest signed context
   and stages an external commit in the same transaction. A persisted SHA-256 marker
   identifies that exact commit. Applications stay held while that marker exists,
   including after a restart, even though OpenMLS has installed operational state.
4. Only confirmation of those exact MLS bytes releases the marker, atomically with
   durable log append, outbox removal and upload-receipt cleanup. Storage failure leaves
   the confirmation retryable. A competing
   sequenced commit discards tentative state; a permanent stale refusal atomically drops
   the abandoned handshake, its upload receipt and tentative state. Recovery waits for any older queued handshake to settle before
   creating another one. Retryable failures preserve the original idempotency key.
   If a clear membership removal lands while an external commit's upload is held,
   confirmation moves upgraded peers to a durable reconciliation hold at the same
   epoch. Only previously admitted leaves may remain temporarily; no unlisted new
   UpdatePath or Add is accepted. Applications stay held through restart until an
   ordinary sequenced commit removes those leaves. Current members can reconcile
   these already-authorized removals without adding identities. This requires upgraded
   peers; mixed-version rollout remains a production gate.
5. A subsequent commit replaces the reference. A legacy commit without a reference
   invalidates the old one instead of recovering from an obsolete epoch. The local
   ciphertext cache keeps at most one downloaded context per Space and is excluded
   from backups; authenticated references in confirmed metadata are retained.
6. Erasing a device clears pending and reconciliation markers in the same transaction
   as its outbox. Failed erasure preserves the account and vault keys; a fresh restored
   device can recover the same Space without inheriting an orphan hold.

New or upgraded groups with no published context make an empty update once their group
is ready. Unknown member agreement keys are queried before claiming one-shot key packages
or staging a recoverable commit. Context download backoff is separate from media retries:
one current reference per missing, eligible Space, pruned when the reference changes,
the ciphertext arrives or the Space no longer needs recovery.
Old relays and members explicitly lacking agreement keys retain the existing Welcome
path. They do not gain peerless recovery from this change.

## Boundaries and validation

The first recovered writer needs no original device, online administrator or surviving
MLS secret. Current relay membership and active enrolled device authorization still
apply. Readers keep their existing permission: they cannot submit sealed commits and
wait for an administrator's recovery and Welcome. Removed identities are refused; a
context containing an identity removed since its publication cannot install a group.
Unlink and roster removal continue through normal MLS reconciliation.

Recovery retains backed-up history and enables future messages. GroupInfo cannot
decrypt encrypted history absent from that snapshot and predating the new join. It
does not recover passwords, missing backups, deleted blobs or lost backup authority.
The existing production password-recovery authorization/HSM gates in ADR 0046 remain.

The real-store journey deletes both original databases and vaults, checks no original
session survives, recovers and sends from the first fresh enrolled device while the
second remains absent, then restores the second and verifies new messages both ways.
Layer tests cover unauthorized roots, wrong groups/epochs/devices, corrupt contexts,
removed members, database write failure rollback, upload refusal, exact-byte resend and
marker gating across reopen, and reference preservation/tampering after pruning.
Regressions also graft a valid-format reference and Commit kind onto a frozen legacy
stub, borrow an admitted sender for a different external leaf, inject confirmation and
erasure write failures, remove a member during a held recovery upload and reopen, and
cycle 10,000 failed download references without retaining their history. The photo
journey identifies its one authenticated encrypted photo object separately from signed
recovery blobs while retaining its ciphertext and plaintext-leakage checks. The delayed
Welcome proxy forwards concurrent HTTP traffic while preserving its ordering fault.

Before public activation or a billion-user claim, prove mixed-version recovery policy,
reader recovery, simultaneous recoveries and asymmetric faults on authenticated TLS;
exercise region failover and backup/blob restore; and add operations-tested retention
and garbage collection for historical recovery blobs. The current shared blob store
retains content-addressed objects and has no recovery-specific collector. Measure
per-commit tree size, key-box CPU, upload/download latency, RSS, churn and complete
storage/egress costs under real workloads. No capacity estimate closes those gates.
