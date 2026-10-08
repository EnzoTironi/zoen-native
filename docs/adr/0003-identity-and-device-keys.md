# ADR 0003: No passwords: an identity key and a key per device, in the Keychain

Status: accepted (milestone 1)

## Decision
An account is an Ed25519 identity key made on the device. Each device has its own Ed25519
key; the identity signs a certificate for it (`device_cert_message`). Events are signed by
the device key and carry the certificate, so a lost phone is revoked by revoking its device
key, not by changing who you are.

Login is challenge-response: the relay sends a nonce, the device signs
`<protocol>:auth:<relay name>:<nonce>`. The relay name in the signed message stops replays to another
relay. The @handle is a directory entry on the relay, unique and changeable; the identity id
(the public key) is what everything references.

Secrets never go in SQLite. The core asks the app through the `SecretVault` UniFFI callback:
Keychain on iPhone (`AfterFirstUnlockThisDeviceOnly`, not synced, not restored elsewhere),
data-protection keychain on a signed Mac build, a 0600 file for the CLI and unsigned dev Mac
builds.

## Not yet
- Adding a second device (QR pairing that transfers a certificate) and account recovery.
  Until then, losing the device loses the identity; this is said in onboarding copy later.
- Key transparency for handles.

## At 1B users
1B identities and about 1.6B devices. Ids are public keys: globally unique with no
allocator. The directory is 1.6B rows of about 300 bytes (500 GB), partitioned by identity.
Handle lookups are point reads on a unique index; prefix search goes to a dedicated search
index, not the primary store. Login is one signature verification per connection; at a
reconnect storm of 10% of 160M sockets within a minute that is 270k verifications/s, about
14 cores fleet-wide.
