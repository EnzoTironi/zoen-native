# Private encrypted history transfer

The fixed Directory now composes the native device-history HTTP contract with its
current device rows and private R2 binding. Shared Rust owns admission and object
keys; JavaScript handles bounded I/O and R2 operations. The outer Worker still
returns404 and has no public route. This is backend adapter acceptance, not a
public history-transfer, recovery or complete application acceptance.

## Contract

- `PUT /v1/transfer/{id}/{n}` accepts1 byte–4MiB of ciphertext, signed over the actual
  body hash, transfer ID, canonical chunk number and timestamp. IDs are64 lowercase
  hex characters; chunks are0–4095. The five-minute timestamp bound is inclusive
  and cannot overflow on extreme signed integers.
- A currently active device is required for every mutation. Uploads spend the
  same weighted device budget as media, including valid retries. Native refusal
  ordering is preserved: path, body bound, signature, active device, then quota.
- R2 conditional creation returns201 for a new chunk and200 for a valid retry.
  An existing chunk is retained even if a retry signs different bytes, matching
  native resume semantics. A signature over different actual bytes is refused.
- `GET`/`HEAD` are ciphertext-capability reads and always use `no-store`.
  The unguessable transfer ID is the capability; no ambient browser credentials
  or plaintext keys reach the relay. Existing GET capability behavior after
  device revocation is preserved.
- Signed `DELETE /v1/transfer/{id}` lists the exact slash-delimited transfer prefix
  before removing chunks in batches. It returns204 only after all batches finish.
  A failed or partial delete returns503; repeating it completes the deletion.
  Unrelated transfers and media objects are retained.

Authorization is the native active-device-plus-capability contract; this adapter
does not invent an account ownership table. Admission is a snapshot: an operation
already admitted before Unlink can finish afterward, and concurrent admitted PUT
and DELETE do not acquire a transfer tombstone. Broader recovery/erasure guarantees
must account for these semantics before public acceptance.

## Explicit private capacity bounds

Media and transfer hosts share one isolate-wide body lease, at most eight unread
waiters, a ten-second queue/stream-progress deadline and the persisted native
default GCRA budget. Incoming fragments are coalesced into owned64KiB buffers;
empty fragments do not allocate or extend the progress deadline. Outgoing bodies
retain the lease until consumed, cancelled or idle.

R2 operations have a15-second response deadline. R2 calls cannot be cancelled:
after timeout, new body readers are refused until the original operation actually
settles. This bounds retained upload memory even when storage stalls. A timed-out
write has an unknown storage outcome and must be retried with the same chunk.
Reads buffer the bounded ciphertext before returning200, so a late storage read
failure becomes503. Transfer deletion is bounded to4096 listed keys, with at most
1000 keys per R2 batch. These are private adapter limits, not production scale
claims or support for native custom quota configuration.

## Verification

```sh
node scripts/verify-cloudflare-directory.mjs --transfer --out-dir /fresh/evidence
```

Use a disposable PostgreSQL cluster through `ZOEN_TEST_PG`; the existing native
fixture creates and drops only its own database. The verifier builds shared WASM
unless `--skip-build` is supplied, records actual native HTTP replies and object
bytes, then compares every vector in workerd with SQLite and R2. Source and exact
WASM/glue hashes must remain unchanged during the run.

The initial local proof passes31 native HTTP vectors and seven workerd cases,
including4MiB boundaries, malformed paths, extreme timestamps, immutable retries,
revocation, shared media quota/body leases, storage read/write failures, deletion
of1001 chunks across R2 pages and partial-delete recovery. Receipt:
`/private/tmp/zoen-transfer-v102/differential-03/summary.json`. No Cloudflare account,
real bucket, DNS, protected relay, user data or public endpoint was modified.

The existing `--media` lane also runs the shared I/O regression tests. CI keeps
transfer evidence before the unchanged Directory socket closure gate. A green
private lane does not replace that gate or prove the deployed backend current.
