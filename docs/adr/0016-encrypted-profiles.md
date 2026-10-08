# ADR 0016: Encrypted profiles

Status: accepted (built between S3 and S2)

## Context
Every person needs a profile page: display name, bio or status, and a photo. Until now the
relay directory held each person's name and bio in plaintext next to their handle, so the
operator (and anyone who could query the directory) could read them. Signal's model is the
reference: the server keeps a profile it can't read, and contacts get the key.

## Decision
- **Profile key.** Each person has 32 random bytes. HKDF-SHA256 (info `zoen-profile-v1`)
  derives an XChaCha20-Poly1305 key. The protobuf `ProfileFields {name, bio, photo}` is sealed
  under it, `nonce ‖ ciphertext`, with AAD `zoen-profile-v1\0 ‖ identity ‖ version_be`. The
  relay can't swap one person's profile for another's, or replay an old version under a new
  number.
- **Upload.** `PutProfile {identity, version, ciphertext, signed}`. The signature comes from a
  device certified by the identity, over `zoen-profile-upload-v1\0 ‖ identity ‖ 0 ‖ version_be ‖
  sha256(ciphertext)`. The relay keeps only the newest version. Resending the exact stored
  bytes counts as success, so a retry after a lost reply never looks stale. Ciphertexts are
  capped at 8 KB. The relay stores `(identity, version, ciphertext, device, sig, cert)` and
  nothing else.
- **Photo.** It uses the encrypted media path from ADR 0007: a random per-file key, an
  XChaCha20 blob addressed by the ciphertext hash, `GET` without auth because the hash is the
  capability. The blob's address and key sit inside the sealed fields.
- **Agreement keys.** Each identity publishes an X25519 public key, signed by a certified
  device over `zoen-agreement-key-v1\0 ‖ identity ‖ 0 ‖ public`. The secret lives in the platform
  vault (`zoen.agreement.v1`) and never in SQLite. Clients check the signature against the
  identity, so a relay can't substitute its own key without forging the identity's
  certificate.
- **Shares.** For each recipient, the sender makes an ephemeral X25519 key and runs
  HKDF-SHA256 with salt `ephemeral ‖ recipient` and info
  `zoen-profile-key-share-v1\0 ‖ space\0sender\0recipient`. That derives the XChaCha20 key that
  seals `key ‖ version_be`. Shares travel as the signed Space event
  `ProfileKeyShared {version, shares: [{to, ephemeral, sealed}]}`, up to 200 per event, and
  `version` is the first profile version the key opens. From M2 on the event is an MLS
  application message, so the relay no longer sees who got a share.
- **Who gets the key.** Contacts are the people you share a relay Space with (DMs, groups,
  communities). After every membership change, catch-up, agreement-key arrival, block or
  unblock, a share pass runs. It shares the current key with each member who doesn't yet
  hold it, skipping blocked people and agents. Every recipient gets one share per key, and
  the set of who already holds it is rebuilt from the sender's own events whenever the key
  version changes.
- **Strangers see the handle.** Relay migration 0004 strips `name` and `bio` from every
  person's directory entry, and `Register` clears them from then on. The directory keeps
  id, handle, kind and tint, which lookup needs; agents stay public. A client shows `@handle`
  as the name for anyone whose profile it can't open.
- **Change events.** After an upload, the relay sends
  `ServerFrame::ProfileChanged {identity, version}` to the owner's co-members and other
  devices. Clients holding the key fetch the new version (`GetProfiles`, 500 ids per request),
  verify the device signature, open it and call `CoreListener.on_profile_changed`. A session
  also refetches every profile it holds a key for when it starts.
- **Block.** A blocked person goes on a local list and the profile key rotates: a fresh key
  from version n+1, a re-upload, and new shares for every contact except them. They keep
  what they already saw and nothing after it, the same caveat as Signal. Unblocking shares
  the current key again.
- **Device storage.** `profile_keys` holds my key and the keys contacts sent me, along with
  the newest fields each one opened. `peer_agreement_keys` holds verified public keys and
  `blocked` holds the block list. All of it is in the same SQLite file, with the same
  protection class as message content (SQLCipher seam, ADR 0011).

## Consequences
- The name in a chat comes from the profile, so a brand-new contact appears as `@handle` for
  the round trip it takes to get their share and fetch their profile. The network task holds
  UI updates for up to 1.5 s while profiles are opening, so names usually arrive before the
  first render.
- A device that loses its agreement secret can't open shares sent to the old key. Senders
  share again when they rotate. Multi-device and new-device recovery reuse the MLS key
  packages in M2, and the agreement key folds into the identity's device set.
- Until M2, the relay can see who shared a key with whom (it already knows who shares
  Spaces), but never the key or the fields.
- The CLI (`zoen profile set|show`, `block`, `unblock`) and the UniFFI API
  (docs/api-profile.md) are the same core, so the journey proves exactly what the app ships.

## At 1B users
- **Storage.** About 1 KB of ciphertext per person, or 1 TB for 1B people: a single Postgres
  table sharded by identity hash with Citus or a directory split, or FDB keys under
  `("profile", identity)` if the directory moves there. Agreement keys add 100 B per person.
  Photos are CDN-cached immutable blobs (ADR 0007).
- **Fan-out.** A profile edit sends one small `ProfileChanged` frame per co-member who is
  online. This is the same per-edge fan-out as presence (ADR 0009). At a few edits per
  person per month it's noise next to messages. Offline members catch up through the
  session-start refetch, which is batched 500 ids per request and only covers key holders.
- **Shares.** One share per (person, contact, key). A median of ~100 contacts gives about
  100 shares per person over the life of a key, or 10^11 tiny events across the fleet, spread
  over Spaces that already exist. Rotation happens only on block. Shares ride Space logs, so
  they shard with Space ownership (S4) and need no new service.
- **Hot spots.** Someone with 10^6 followers in a community gets their key shared in that
  community's log as 5k events of 200 shares each, written once. The relay sees ciphertext
  and the frame budget is unchanged. Above a threshold, M3 communities can switch to a
  community-scoped profile key shared once per epoch through MLS.
- **Abuse.** Profile uploads and agreement lookups go through the per-device token buckets
  (S5). Lookups return keys only for identities that published one, and an agreement key
  reveals nothing a handle lookup doesn't already.
