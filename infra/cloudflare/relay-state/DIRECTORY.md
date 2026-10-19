This private adapter keeps one directory transaction domain per relay cell. Identities,
unique handles, globally unique devices, sealed one-time link boxes, handshake challenges
host sessions, public MLS key packages, publication tombstones and durable claim receipts
belong to the same SQLite Durable Object. A caller cannot choose an
account shard or supply `registered`, an enrollment row, or an authenticated session.

`hello({frame})` accepts the canonical native binary Hello, verifies the identity's device
certificate in shared Rust, and returns a native protocol-4 Challenge plus an opaque
challenge ID. Its nonce comes from the host's secure random generator. `authenticate`
accepts that ID and the native Auth bytes, consumes the challenge once (including failed
attempts), verifies the device signature over the unchanged `zoen-sync/2:auth` domain,
and derives the session from current directory rows. Challenges expire after ten seconds;
the private adapter caps pending challenges and sessions at 256 each. Its sessions expire
after fifteen minutes; this bound is not a proposed public WebSocket lifetime.

`request({sessionId,frame})` supports Person Register, DeliverLink, FetchLink, Unlink,
PublishKeyPackages and ClaimKeyPackages.
Each request re-reads its active device in the same synchronous SQL transaction as its
writes. Root certification alone cannot link a new device to an existing account. A
registered active sponsor is required; exact active enrollment retries are read-only,
while revoked/conflicting devices cannot be revived or reassigned. Enrollment and its
sealed link box commit together. Boxes preserve the native fifteen-minute expiry,
bounded cleanup and one-time consumption. Successful Register transitions only that
session; other preopened provisional sessions cannot overwrite the new identity.

The Rust policy reuses the native protocol codecs, certificate/auth signature domains,
Person profile normalization/ownership rules and request authorization predicate. The
native PostgreSQL relay calls the same extracted pure functions. The pending self-key
FetchLink exception remains valid after sponsor enrollment assigns the key to an account.
First registration still cannot claim that globally enrolled device for its self identity.

`authorize({sessionId})` returns a request-admission snapshot. It is not an outbound
delivery lease. Returned binary reply bytes are private operation results, not permission
to send them to an external socket. Native delivery holds a durable device fence through
the actual send; this adapter has no sockets and does not implement that fence. An already
admitted native Space append may finish after revocation, while fresh admission and every
future public delivery must reject the revoked device.

MLS publication validates the entire batch and last-resort package in shared Rust before
writing anything: at most 100 packages, each at most 4096 bytes, using the native TLS,
ciphersuite, signature, lifetime and identity/device checks. The original package bytes
are stored unchanged. Single-use SHA256 publication tombstones survive consumption until
the signed expiry; retrying publication cannot resurrect consumed stock. Capacity is 200
single-use packages per device; the independent last-resort package is replaced in place.
Expiry maintenance removes at most 256 scoped rows per operation, plus an exact expired
publication hash when needed. Unlike native bulk cleanup, unused expired physical metadata
can linger; expired rows never affect deduplication or live receipt quota accounting.

Claims select one oldest single-use package per currently active target device, or retain
its last-resort fallback. Native SQL does not promise inter-device result order; fresh
cross-backend comparisons use record sets, while each backend preserves its exact original
receipt bytes and order for retries. Receipt-mode operations retain the native canonical
ULID/time rules, sorted/deduplicated one-to-fifty canonical targets, and source-device scope.
The original canonical id-zero binary response is durable for 24 hours, and a replay uses
the current request ID without consuming stock again. Fresh source authorization precedes
every lookup, even replay: revoking a target does not erase an existing caller's receipt,
while revoking the caller prevents its retry. Unlink deletes that device's inventory in
the same transaction and retains publication tombstones and claim receipts. A late already
admitted publication cannot deliver from a revoked target. Legacy claims without an
operation retain the native first-50-before-dedup behavior and create no receipt.

The native per-source quotas of 4096 receipts and 8 MiB aggregate canonical binary response
bytes are preserved, including replay before quota and rollback if a new encoded response
would exceed quota. This private adapter additionally caps a fresh claim at 128 records
and 768 KiB canonical response bytes. SQL reads a 129th sentinel and refuses with retryable
`private_key_package_capacity` before deleting stock or storing a receipt; it never silently
truncates devices. This is an explicit private capacity bound, not full native capacity
parity. Selected data and receipt facts use base64; the existing shared context limit is
1 MiB of actual UTF-8 bytes. SQL bindings stay bounded and each stored response stays below
Cloudflare's 2 MiB whole-row limit. Local workerd stress fixtures prove 128 records of 4096
bytes and pre-consumption refusal at 129, independently of genuine signed MLS fixtures.

The Worker has no HTTP routes, workers.dev or preview URLs. It is not connected to the
closed-Space service. Provider proofs/JWTs, backup recovery, agent registration, profile
fanout, global rate limits, KeyPackagesLow/refill notifications, Space MLS epochs, encrypted
commit/checkpoint/hold handling and the complete public session protocol remain separate
gates. Encrypted blob transport and R2 activation are also unfinished. No caller may
use this slice as a public authentication facade.

With an existing PostgreSQL test environment, run
`node scripts/verify-cloudflare-directory.mjs --out-dir .dev/cloudflare-directory` from
the repository root. `--skip-build` skips only shared WASM generation; the native oracle
still creates its own fresh, validated PostgreSQL schema, runs real native directory
transactions and removes only that schema on success or ordinary errors. The test bridge
and SQL failure triggers are local fixtures and are absent from the deployment config.
The shared WASM build command is reused from `scripts/build-cloudflare-closed.mjs`; it
does not require PostgreSQL or deploy either Worker.

The current key-package extension passed 71 fresh native PostgreSQL scenarios and 18
local workerd cases, including real signed MLS publications, exact receipt retries,
revocation, durable restart, concurrency and late SQL-failure rollback. The shared Rust
crate passed 47 tests, and the existing native receipt transaction suite passed all three
cases. Capacity fixtures populate trusted synthetic storage only; they do not claim real
MLS enrollment of 128 devices. These checks validate private request operations, not
remote public sessions or delivery.

The key-package extension was privately deployed on 2026-10-19 as
`1fd4bab4-7013-4749-bb1e-3c59efa146c3`, using reviewed runtime `8bb0ae1e`.
The deployment checkout was descendant `819e58cb`; its only changes from the reviewed
commit were outside the shared runtime/build inputs. All 19 reviewed source hashes
and the rebuilt 1,636,277-byte WASM matched before upload and afterward. Its WASM
SHA256 is `ae04d9d2969eae654fcfbdcc2924137d018105d5def6523ba6df3a0ee64d5bb0`.
The existing namespace `66f8e984ecda4663944a760b49d31254`, all three bindings and
the migration tag were preserved. Read-only API checks confirmed active version 2,
`has_preview=false`, no routes/custom domains and disabled workers.dev/previews.
Receipt: `/private/tmp/zoen-cloudflare-admission-v96/validation/directory-keypackages-deployment-acceptance.json`
(SHA256 `4b3cb67beee155ae69e724520805f04f170849aae83f2b2bc312c8633f92067a`).
This is private provisioning evidence; no remote Directory request journey was run.
The previous `759ffee3-0ef9-4ff5-a53b-1c971a3e2197` provisioning proof remains retained.

The earlier [CI run 39204333306](https://github.com/EnzoTironi/zoen-native/actions/runs/39204333306)
validated the original 33 PostgreSQL scenarios and nine workerd cases on merge
`59998b677ce581bd49a045538ee62f3d17b55347` for head `52da9729`. Artifact
`12164869200` predates this key-package extension. The later 47 Rust tests, three
native receipt regressions and 71/18 differential cases above are separate local
evidence. Machine-local receipt paths do not replace that portable CI attribution.

The later [CI run 39207994304](https://github.com/EnzoTironi/zoen-native/actions/runs/39207994304)
passes all 71 PostgreSQL scenarios and 18 workerd cases for head `c727b1f9`, merge
`85c7f2d58aef38fc5853fb7b8efab3641aee0d91`. Artifact `12166444089` retains the
fixture and WASM;19 source hashes match that head and all four recorded artifact
hashes were verified. The overall workflow fails an independent admission-format
check, corrected by `8d64d762`; its tests were not executed in that job. Receipt:
`/private/tmp/zoen-cloudflare-web-v96/ci-c727-evidence/acceptance.json`
(SHA256 `47ba634a497895c0479035eb9b3241b18cc7ca11edb722a479eba66570c8b1d0`).

## Private socket work in progress

Commit `8d64d762` adds a private native-codec Hello/Auth transport with the
Directory's current device/certificate checked immediately before every outbound
frame. Revocation, self-unlink, authenticated expiry, normal client close and SQL
cleanup pass. Its 71 native scenarios and 32 of 34 local workerd cases pass, with
zero skips. Two untouched pre-Hello timeout cases fail to complete the client close
within five seconds on the pinned local runtime. This source is not deployed or
accepted as a full transport. The blocked receipt remains immutable at
`/private/tmp/zoen-cloudflare-admission-v96/validation/directory-socket-blocked-acceptance/acceptance.json`.

A separate minimal probe with no Zoen authentication, SQL or Rust code passes all
four lifecycle cases directly against authenticated Cloudflare edge preview,
including untouched sockets and a real alarm. Clients reached CLOSED within the
unchanged five-second limit. The temporary Worker and SQLite namespace were removed
and the exact account resource inventory restored. Receipt:
`/private/tmp/zoen-cloudflare-admission-v96/validation/socket-remote-runtime-02/acceptance.json`
(SHA256 `8fdc13155568c4c256d8cc80451fbb0c6db6b134b57d099486d24b64b63be47d`).
This isolates the bounded local-runtime symptom; it is not a remote Directory,
provider authentication or encrypted synchronization acceptance.
