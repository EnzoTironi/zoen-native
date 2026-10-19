# Private closed-group relay

The adapter committed as `c7de156f` supports closed Groups through trusted service
bindings. Its caller supplies authenticated identity/device context and scoped
directory facts. It does not establish public authentication or encrypted sync.

Shared Rust verifies Publish frames, certificates and signatures, applies the
native admission rules, normalizes invite capabilities and produces canonical
Sequenced protobuf bytes, chain hashes, membership and delivery audience. JavaScript
provides bounded persistence and invokes those Rust exports through precompiled
WASM with `initSync`; it contains no second crypto, protobuf or admission policy.

After verification, one synchronous SQL transaction looks up the original receipt,
loads projection/history/invite facts, admits a fresh event and commits its bytes,
projection and invite consumption together. Valid retries return their original
receipt with the current audience, including after removal or changed signed retry
bytes. Rejection and later SQL failure leave every table unchanged.

`SpaceStorage` shares persistence helpers with the original storage-only Worker;
neither helper owns a nested transaction. `ClosedSpace` uses a separate Durable
Object namespace and exposes no raw `appendAdmitted`. Its private bounds retain
Space IDs up to 128 characters, client IDs up to 256, and lowercase author/hash
keys; this is not complete native LogStore equivalence.

## Reproduce locally

Use the [admission toolchain](admission.md#reproduce), pinned Cloudflare dependencies
and an existing development FoundationDB cluster with `FDB_CLUSTER_FILE` and its
client-library environment configured. From the repository root:

```sh
npm ci --prefix infra/cloudflare
cargo test --locked -p roda-relay-admission
cargo clippy --locked -p roda-relay-admission --all-targets -- -D warnings
cargo clippy --locked -p zoen-relay --example cloudflare-closed-fixture -- -D warnings
CARGO_BUILD_JOBS=2 node scripts/verify-cloudflare-closed.mjs \
  --out-dir .dev/cloudflare-closed
```

The runner builds current WASM, creates and cleans a fresh isolated native FDB
cell, and compares exact receipts, rejection reasons, audiences and membership
against SQLite Durable Objects in local workerd. `summary.json` records hashes and
source provenance. `--skip-build` reuses WASM; the fresh native FDB oracle still runs.

Recorded acceptance passed 29 Rust tests, strict shared/oracle Clippy, 17 native
differential steps, four negative no-write cases, five closed-adapter tests and
four shared SQL contracts. Coverage includes invalid signatures before dedupe,
unsupported encryption, same-frame races, restart and rollback after a failed
invite write.

## Private deployment and remote proof

`wrangler.closed.jsonc` disables public routes, Workers.dev and preview URLs. Its
build hook resolves the repository-root `build-cloudflare-closed.mjs` wrapper,
which fixes the working directory and generates current WASM without FoundationDB.
Both repository-root and npm-workspace deployment dry-runs passed.

Private Worker `zoen-relay-closed`, version
`b5a34a7e-4305-46de-b595-06a909864e8a`, passed authenticated service-binding QA:
11 pre-invite native steps, six events totaling 4,608 canonical bytes, invalid
signature rejection, payload refusal, exact replay across two preview **client**
sessions, and seven private HTTP paths returning 404. Both clients stopped.
This did not force a remote Durable Object restart or exercise remote invite clocks.

The opt-in helper is `relay-state/tests/closed-remote.mjs`; it requires
`ZOEN_CF_REMOTE_CLOSED_QA=1` and `ZOEN_CLOSED_FIXTURE` pointing to a fresh paired
native fixture. Its static allowlist permits only that synthetic Space.

Machine-local receipt
`/private/tmp/zoen-cloudflare-admission-v96/validation/closed-acceptance.json`
has SHA256 `524a38f64b3572e51a6d94b77f022bc5f203668958a4032ce03d28a2157b3d82`.
All 32 recorded source hashes match `c7de156f`. These temporary paths are not
portable CI proof; the latest CI integration remains pending.

Next gates are transactional directory/identity/revocation authorization, global
invite resolution, MLS epochs/holds/checkpoints/pruning, the public WebSocket
protocol and delivery, and separately authorized private R2 media activation.
