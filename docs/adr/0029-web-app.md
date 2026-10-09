# ADR 0029: The Zoen web app

Status: accepted (Enzo, 2026-10-09); planned after the native app is stable

## Context
People reach Zoen from invite links. Today that means installing an app before seeing
anything. A web version would let someone join in seconds, but code served by our server on
every visit is a weaker trust story than a signed app binary, and the core (identity, MLS,
log, store) is Rust that the native apps already share.

## Decision
- **A real web framework, not SwiftWasm or Tokamak.** The web app is its own UI, built with a
  mainstream web framework and its tooling. The SwiftUI code stays native.
- **The Rust core is reused via WASM.** The same crates the apps use through FFI (identity,
  MLS, event log, sync, local store) are compiled to `wasm32` and called from the web UI.
  The web app does not get its own protocol or crypto.
- **It starts as a phone-paired web version.** The first web release is a companion paired to
  the user's phone, like a linked device under ADR 0003: its own device key, certified by
  the phone, revocable from it.
- **Web is the instant-onboarding entry.** An invite link opens the web app straight into
  the conversation or Space it points to. There is no marketing landing page in that path.
- **Install the native app where native features are needed.** Where the web can't do
  something well (background delivery, notifications, device integrations, larger media,
  local agents), the web app shows a clear call to install the native app and carries the
  session over.
- **Mitigating served-code risk.**
  - **Verifiable builds:** the web bundle is built reproducibly from a tagged commit; its
    hashes are published, and the served files are pinned with Subresource Integrity, so
    anyone can check that what we serve is what the source builds.
  - **Non-exportable keys:** device keys are WebCrypto keys created with
    `extractable: false` and kept in IndexedDB, so served code can use them but can't read
    them out. A compromised bundle could still act during a session; it couldn't take the
    keys away.
- **When:** after the native app is stable. No web work starts before then.

## Consequences
- The core crates must keep building for `wasm32` (no threads or native sockets in the
  shared code paths); CI gains a `wasm32` check when the web work starts.
- A second UI codebase to maintain, kept small by putting logic in the shared core.
- The web device is a full device under ADR 0003, so revocation, E2E (ADR 0026/0027) and
  approvals work the same as on the phone.
