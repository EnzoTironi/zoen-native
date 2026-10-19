# Shared relay admission

`roda-relay-admission` contains the same Rust rules used by the native relay and
the portable Worker boundary. The extraction moves the existing policy and its
nine tests; it does not implement a second policy in JavaScript. The native
FoundationDB adapter re-exports the shared types and canonical invite-code hash.

The portable boundary verifies the canonical Publish frame, device certificate,
signature and authenticated subject, then admits against trusted server facts.
Rust produces the canonical Sequenced protobuf bytes, chain hash and typed effect.
UInt64 cursors and epochs cross JSON as decimal strings. Existing frame/envelope
bounds apply; trusted context is bounded to 32 KiB and two cited history hashes.

## Transaction boundary

This library does not establish directory authorization or write durable state.
An adapter must authenticate a currently registered, unrevoked device before use.
After signature validation it must look up the original `(Space, author,
client_id)` receipt **before fresh membership admission**. A valid retry retains
its original acceptance even after membership changes or different retry bytes.

For a fresh event, the storage owner loads scoped membership, metadata, history,
MLS and invite facts. It must fence the observed head and atomically commit the
event, dedupe receipt and every effect, including invite consumption. Delivery
uses current/final membership, preserving the native legacy-roster restrictions.
Directory registration/revocation and invitation expiry/use conflicts must not be
replaced by client-supplied JSON. Public callers must not supply trusted facts or
invoke the storage-only `appendAdmitted` interface.

## Reproduce

Use Rust 1.99.1 with `wasm32-unknown-unknown`, Node 26 and
`wasm-bindgen-cli` 0.2.129. From the repository root:

```sh
npm ci --prefix infra/cloudflare
cargo test --locked -p roda-relay-admission
cargo clippy --locked -p roda-relay-admission --all-targets -- -D warnings
CARGO_BUILD_JOBS=2 node scripts/verify-cloudflare-admission.mjs \
  --out-dir .dev/cloudflare-admission
```

The runner builds public deterministic fixtures using native Rust, compiles the
same library to WASM and compares its results in Node and actual local workerd.
Workers imports the precompiled WASM module and uses `initSync`; it does not depend
on browser WASM fetching or dynamic compilation. Checks include exact native
protobuf bytes/effects, signature rejection, scoped lookup keys and sequence
values above JavaScript's exact integer range. `summary.json` records source and
artifact provenance. `--skip-build` explicitly uses existing fixture/module files.
Neither mode contacts an account, deploys or runs a public relay.

## Recorded foundation

On 19 October 2026, the source committed as `db3f3885` passed:

- 20 tests: nine original rule tests and eleven portable-boundary regressions.
- Strict Clippy, formatting and locked/offline Cargo resolution.
- All eleven scenarios of the native relay's actual FDB `log_store` contract,
  using isolated synthetic cells.
- Release WASM build and the full tracked native/Node/workerd verification runner.

The generated WASM is 885,129 bytes, SHA256
`24c629124795efc9c7ea8f2382259a5370a18163fbf6a303972b71106cf6bdc6`.
The machine-local evidence root is
`/private/tmp/zoen-cloudflare-admission-v96/validation/`;
`foundation-acceptance.json` hashes all fourteen source inputs and command
receipts, and `complete-run/summary.json` records the final runtime result.
These temporary paths are not portable downloads. The `cloudflare-admission`
Expo CI job preserves equivalent repeatable evidence without deployment credentials.

The later [private closed-group adapter](closed-groups.md) reuses this policy in a
separate deployed namespace, with its own 29-test/differential/remote evidence.
The original storage-only Worker retains its own contract. Complete MLS projection,
directory/revocation, WebSockets, provider login and public encrypted synchronization
remain integration work. See [deployment](README.md).
