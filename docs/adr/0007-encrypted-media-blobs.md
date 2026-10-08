# ADR 0007: Media travels as encrypted, content-addressed blobs

Status: accepted (milestone 4, started during milestone 1)

## Context
Chat backgrounds already used content-addressed photos stored in the device database, and a
throwaway Python server (`tools/dev-relay`) mirrored them in plaintext. Shared chats need
everyone to get the photo, and the server must not be able to look at it.

## Decision
- Each attachment gets a fresh 32-byte key. The device encrypts it with
  XChaCha20-Poly1305 (random 24-byte nonce, prepended) and addresses the ciphertext by its
  sha256. `MediaRef` gains optional `key` and `blob`; local chats keep plain references.
- The key rides in the chat's signed log. Until M2 the relay can read that log, so the
  protection is complete only once MLS encrypts the log. The relay's blob store and any S3
  bucket behind it never see a key.
- Upload: `PUT /v1/blobs/{sha256}` with `x-zoen-device`, `x-zoen-ts` and `x-zoen-sig`, a
  device signature over `zoen-sync/1:blob-put:{relay}:{sha256}:{ts}`. The relay checks the device
  logged in before, the timestamp is within five minutes and the body hashes to the address.
  Writes use create-if-absent, so a retry is a no-op.
- Download: `GET /v1/blobs/{sha256}` with no auth. The address is the hash of random-keyed
  ciphertext, so it can't be guessed and reveals nothing.
- Storage is `object_store`: a directory in dev, any S3-compatible bucket in production
  (`ZOEN_S3_BUCKET`, the usual `AWS_*` variables, `AWS_ENDPOINT_URL` for R2, Tigris or MinIO).
- On the device, `media_keys` remembers each attachment's key and address (re-sharing reuses
  the copy), and `blob_uploads` queues ciphertext until the relay has it. The network task
  runs a media worker beside the socket: uploads first, then downloads for any photo a
  shared chat shows but the device lacks, with exponential backoff because a peer's event can
  arrive before their upload finishes. A download is stored only if the ciphertext hashes to
  the address, opens with the key and the plaintext hashes to the promised sha256.

## Consequences
- `tools/dev-relay` and the app's `DevRelay` path go away.
- Voice notes and files reuse the same pipeline.
- Large files will want chunking and resumable uploads; the address scheme stays.

## At 1B users
200 TB of new media a day, 73 PB a year. Content addressing makes every blob immutable, so
a CDN caches it forever (`Cache-Control: immutable`) and the relay serves only misses.
Writes are create-if-absent, so retries cost a conditional put. At object-storage prices
around $0.01/GB-month the first year's media costs about $0.7M/month at the end of the year,
the largest line in the budget, which is why retention tiers (cold after 90 days) and
resized uploads matter more than relay tuning. Large files move to chunked, resumable
uploads with the same addressing.
