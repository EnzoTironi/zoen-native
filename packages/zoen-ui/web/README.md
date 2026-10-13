# Authenticated browser client

This entry is separate from the sample-data shell preview. React renders conversations
from the shared Rust event log; the browser core uses the native protocol 4 protobufs,
OpenMLS ciphersuite, leaf certificates and roster checks.

The first session is paired by the phone. The browser creates a nonexportable Ed25519
WebCrypto key, saves its pending link encrypted in IndexedDB, and displays the native
`zoen-link:1:` code and six-digit comparison. The phone's protocol 4 `DeliverLink` enrolls
the device; the browser's `FetchLink` opens the native X25519/HKDF-SHA256/ChaCha20Poly1305
PSK box. Its certificate must identify this browser public key. The received identity
seed is erased after certificate verification and is absent from the saved account.

IndexedDB stores the protected CryptoKeys and an AES-GCM encrypted session containing
the account, directory, event logs, MLS provider state and outbox. An encrypted session
without its keys is refused without replacing them. Only one tab holds the client Web
Lock. Forgetting the browser deletes its local database; revoke its device from the
phone to remove its relay authorization.

OpenMLS runs in a dedicated worker. It signs through a bounded SharedArrayBuffer bridge
to the main thread's nonexportable key. The device adapter checks the ciphersuite,
certificate, public key, signer proof and generated KeyPackage leaf. Closing the worker
wakes pending signing, cancels requests and terminates the worker. The static server
supplies COOP/COEP isolation; absent secure context, SharedArrayBuffer, Worker,
IndexedDB, Web Locks or nonexportable Ed25519 support produces an unavailable screen.

Every production MLS update is encrypted and durably saved before any resulting frame
is sent. Storage failure stops transport and worker and requires a reload from the last
saved state. Reconnect restarts sync from saved cursors and replays saved pending work;
stale socket generations cannot complete authentication or emit queued frames.
The phone supplies the browser's MLS Welcome for each existing encrypted conversation.
Messages use the native signed inner event inside real MLS ciphertext. Conversations
without a usable group are held with a visible waiting state; plaintext messaging is
not used as a fallback. Earlier encrypted messages that the new device cannot open are
counted and remain available on the phone. Shared native recovery and historical page
transfer are not yet exposed by this browser adapter.

`/join/CODE` stays in the invitation flow. After phone pairing, the authenticated relay
provides the preview; explicit Join queues the same certified self-membership event and
invite metadata as the native client. Existing membership opens that conversation.
The native-app link retains the invitation code.

## Development

From `packages/zoen-ui`:

```sh
npm ci
npm run typecheck:web
npm run test:web
ZOEN_WASM_BINDGEN=/absolute/path/to/wasm-bindgen npm run build:web
npm run web
```

The build requires the `wasm32-unknown-unknown` Rust target and wasm-bindgen CLI 0.2.129,
matching the locked library. It builds `zoen-web-core` with one compiler job, generates
the WASM bindings, bundles the worker/UI and writes `web/dist/build-manifest.json` with
SHA-384 hashes. HTML pins JS/CSS with SRI; the main bundle pins the worker and the worker
pins WASM using fetch integrity. `node web/build.mjs --skip-wasm` reuses already generated
bindings. The owned local server listens on `127.0.0.1:4174`; `ZOEN_WEB_PORT` can override
the port. Supply the relay origin in the pairing form or `?relay=http://127.0.0.1:PORT`.
Remote relays require HTTPS. Production hosts must preserve the same isolation and CSP
headers and publish the manifest from the exact reviewed source build.

Shared-core checks from the repository root:

```sh
cargo test -p roda-mls --features portable
cargo test -p zoen-web-core
cargo check -p zoen-web-core --target wasm32-unknown-unknown
```

The new crate contains native SQLite/OpenMLS tests for Welcome, messages in both
directions, persisted reload, membership removal and signed invitation publication.
They do not replace browser/native enrollment against a real owned relay.

## Verification status

On 2026-10-12, `typecheck:web` and all 13 Node behavioral tests passed. WebCrypto tests
use actual nonexportable Ed25519/AES-GCM keys. Vault lifecycle tests use an in-memory
IndexedDB implementation with those real keys. Transport/controller tests use codec
and socket doubles only to exercise cancellation, generation and persistence ordering.

The Rust adapter tests, WASM build and browser/native protocol 4 pairing/messages
journey have source prepared and await the coordinated compiler/service slot. No
browser/native interoperability or compiled WASM acceptance is claimed by the Node
tests. Reproducibility from a published tagged commit also remains a release gate.

The browser saves the linked account, including a pending session whose `confirming`
field holds that account, and only then sends `ack_link`. The relay deletes the box on
that receipt. A reload while `confirming` is still stored sends the receipt again;
`AckLink` succeeds when the box is already gone. A failed or timed-out receipt stays
`confirming` and is retried; that save is never replaced by a pending-only session.
A transient close must not cancel the
browser's reconnect timer. A lost `fetch_link` response before `confirming` keeps that
pending session and reads the same box again, without a new code. This is separate from
the receipt: the receipt is not sent until that save has finished.

## Final shared-main checkpoint

Root reran the exact final TypeScript sources: typecheck and all 13 behavior tests passed. `cargo test --locked -j2 -p roda-mls --features portable` passed 17 tests, including native SQLite/external-signer exchange. `cargo test --locked -j2 -p zoen-web-core` passed three native Rust tests for enrollment-box cryptography, SQLite Welcome/messages/reload/removal and certified invite publication. These do not prove the browser worker/WASM bridge or real native/browser enrollment. Independent checkpoint-history, snapshot-owner, retry/invite and pre-ack replay findings remain open in the [complete handoff](../../../docs/dev/handoff-2026-10-12.md). The user requested pausing after this source checkpoint.
