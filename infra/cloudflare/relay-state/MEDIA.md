# Private encrypted media on R2

The media adapter in `86cc9a74f9cdc1082d3ad9dbbdfc641229c62613` reuses the
[Directory](DIRECTORY.md), its current device rows and shared Rust admission policy.
`PrivateDirectory.media(Request)` forwards to the configured fixed Directory cell,
`zoen-private-directory-v1`. The `MEDIA_BUCKET` binding targets the private
`zoen-private-blobs` bucket. There is no additional account database or namespace.
The outer Worker still returns HTTP 404; routes, workers.dev and preview URLs remain
disabled. See the [storage overview](README.md) for the other private adapters.

## Native contract

Clients encrypt media before upload and retain the decryption keys. Object addresses
are lowercase SHA256 hashes of the **ciphertext**; keys never enter R2 metadata.
Shared Rust owns signature verification, hash validation, object-key construction
and GCRA arithmetic. JavaScript owns bounded I/O and current Directory SQL reads.

`PUT /v1/blobs/{sha}` accepts `x-zoen-device`, `x-zoen-ts` and `x-zoen-sig`.
The device signs the unchanged `zoen-sync/2:blob-put:{relay}:{sha}:{ts}` domain;
the relay name comes from the server, and the timestamp must be within five minutes.
After receiving the complete bounded body, admission checks the signed prefix and
current active device, then body size and weighted quota, then the complete digest.
Every duplicate follows these checks. Both valid duplicates and hash mismatches
spend per-device quota; an existing object never shortcuts authorization or hashing.

R2 atomically creates `blobs/{sha[0..2]}/{sha}` only if absent. A new acknowledged
write returns 201; an existing valid object returns 200 without replacement. Native
status, plaintext error body, Content-Type and CORS behavior are compared with actual
HTTP responses. GET and signed PUT allow any origin without ambient credentials.

`GET /v1/blobs/{sha}` deliberately requires no session, device or Space membership:
the ciphertext hash is the download capability. It returns byte-exact
`application/octet-stream`, `Cache-Control: public, max-age=31536000, immutable`
and `Cross-Origin-Resource-Policy: cross-origin`. A missing object returns 404;
storage or body-read failure returns 503 before successful response headers.
Failures receive no immutable cache header. R2 contains no plaintext or decryption
keys; clients verify the returned ciphertext and decrypt it locally.

The native upload authorization is an admission check, not a lock held through
object storage. An already-admitted PUT may finish after Unlink. Correctly signed
subsequent uploads, including duplicates, receive 403; capability GET still works.
This does not imply the stronger socket delivery or backup recovery fence.

## Explicit private bounds

- Media bodies and R2 reads are limited to 25 MiB. The native outer upload extractor
  allows an additional 1024 bytes: MAX+1 reaches authorization before size refusal,
  while a body beyond MAX+1024 is rejected before handler admission.
- One module-scoped body lease covers all Directory instances in an isolate.
  At most eight waiting requests retain unread streams. Overload returns retryable
  503; queue waits, incoming read idleness and outgoing read idleness expire after
  ten seconds. Consumption, cancellation and timeout release the lease.
- Digest updates copy at most 64 KiB into WASM. The complete upload is never encoded
  as a numeric JSON array or copied into WASM in one allocation.
- Only the native default quota is implemented: 100 MiB/minute with a 200 MiB
  burst, charged in rounded-up KiB. `ZOEN_LIMITS` customization and the native
  counter shared with transfers remain outside this slice.
- Quota persists decimal nanoseconds with a nondecreasing wall-clock basis, clamped
  against the previous admitted timestamp. This survives reconstruction and avoids
  granting quota on a backward clock step; native uses process-monotonic `Instant`.

R2 provides [conditional writes and checksum validation](https://developers.cloudflare.com/r2/api/workers/workers-api-reference/)
and [strongly consistent create-if-absent semantics](https://developers.cloudflare.com/r2/reference/consistency/).
The adapter exposes no object listing or deletion RPC and adds no bucket-wide TTL.

## Acceptance

Run from the repository root with an isolated PostgreSQL test environment:

```sh
ZOEN_TEST_PG=postgres://TEST_ROLE@127.0.0.1:TEST_PORT/postgres \
  node scripts/verify-cloudflare-directory.mjs --media --skip-build \
  --out-dir FRESH_EMPTY_EVIDENCE_DIR
```

`DATABASE_URL` is the fallback. `--skip-build` skips shared WASM generation only;
the command still runs all actual native HTTP tests, generates a fresh oracle in
disposable test databases, and runs the separate media workerd lane. Test clocks,
storage faults and inspection exist only in local fixtures. The known local socket
gates are neither skipped into success nor reclassified by this command.

Local acceptance passed 11 shared Rust media tests, four native limiter tests,
six native HTTP tests, 28 HTTP comparison cases, three native storage-fault vectors
and all 13 media workerd cases. Compatibility checks passed the existing 18 Directory
and 35 provider workerd cases. Coverage includes real 25 MiB storage, extractor and
signed-prefix precedence, malformed paths and long headers, immutable duplicate
handling, quota persistence, concurrent requests, cancellation, abandoned downloads
and the admitted-PUT/Unlink interleaving. Strict scoped Rust Clippy, Oxlint with
warnings denied, Oxfmt and diff checks passed. Owned PostgreSQL was stopped and
removed with no leftover test databases or public tables.

The local index is
`/private/tmp/zoen-cloudflare-admission-v96/validation/media-final-acceptance/acceptance.json`,
SHA256 `99a343afe9de6be158275805b8fd407099f538535186245f73ff4be718d0fb63`.
It binds 19 changed server files and the complete runner's 133 source hashes plus
five artifacts. These tests used local R2 storage, not real remote objects.
The shared/native index is
`/private/tmp/zoen-media-v96/shared-native-media-acceptance-02.json`, SHA256
`b2eb601fcf19847760a6c5be86829073edf5dff444d4b45ab3380c46b9d19074`.

## Private deployment and real R2 proof

Version `9882fa56-24bb-4573-a592-a50b4c138972` was deployed from the exact committed
runtime and sealed passed-test WASM. Post-upload API checks preserve namespace
`66f8e984ecda4663944a760b49d31254`, the fixed cell, relay name and migration tag,
and confirm the added private R2 binding with no public routes or preview URLs.
Deployment receipt: `/private/tmp/zoen-cloudflare-r2-v100/deployment-acceptance.json`,
SHA256 `37970a1069e8c4e4a5a3c375f3af7c9c71dcb005317f523fba92e2ef62bb28fe`.
This checks frozen upload inputs and actual version metadata; server module bytes
were not downloaded.

Authenticated edge QA passes seven checkpoints through the actual DO/R2 adapter in
one fresh, fixed synthetic cell: private access refusal, a missing capability with
no negative cache before its later upload, normal native registration, signed PUT
201/retry 200, exact GET and native decryption, Unlink followed by fresh/duplicate
PUT 403, and the native capability GET after Unlink. This remote proof uses direct
DO calls; fixed service-facade forwarding is covered by the separate local test.

The native helper generated two encrypted objects totaling 1,422 bytes. Only the
first 647-byte object was stored. The preview stopped and owned encryption keys
were removed. Owner CLI cleanup removed that exact object and confirmed both
allowlisted keys missing; no prefix deletion or product DELETE API was introduced.
Small revoked metadata may remain in the synthetic cell; QA did not address the
production fixed cell. Existing Worker/namespace/binding metadata stayed unchanged
during the probe. Public `r2.dev` access remains disabled, with no custom domains.

Final companion receipt: `/private/tmp/zoen-cloudflare-r2-v100/remote-acceptance.json`,
SHA256 `648f8479451c33f794aa484d75b1a38f7ccf43b53a261c7d4e9a6fa830e7c743`.
It preserves the original journey receipt and adds the exact-key cleanup proof.
Remote 25 MiB capacity, performance, transfer chunks, backup/recovery, the public
gateway, real-provider integration and complete public relay parity remain open.
Private media acceptance does not enable public encrypted sync.
