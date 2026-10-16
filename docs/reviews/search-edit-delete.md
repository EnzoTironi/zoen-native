# Search, edit and delete: front (d)

This front is independent of the thread, presence, reaction/receipt and browser-preference PRs. The source inventory was read from the lead's published `reason/task-a1e26d3e` branch; that branch was not merged. The implementation baseline is main `494237a`.

## Behavior

- Native message search has an explicit conversation scope, membership checks, deterministic keyset ordering and a maximum page size of 100. Cursors bind the query, conversation and committed index generation. A rebuild (including reopen or another indexed write) invalidates old cursors; callers must restart rather than concatenate stale pages.
- Apple and Android expose conversation filtering and load-more in universal search. Errors are distinct from no results, and refresh follows the model revision. Message hits navigate to the exact message.
- Web search uses the browser's current decrypted message projection, including replies, rather than conversation titles alone. Its UI explicitly excludes phone-only history. It does not add remote search or a separate search index. Deleted messages and unconfirmed local messages are excluded; quoted old text is not independently indexed. Pages use stable timestamp/conversation/message keys.
- Native edit/delete controls retain their draft and error until success or explicit cancellation. Web editing uses a separate dialog so it cannot replace the send draft. Missing controllers and mutation refusals are visible rather than silently succeeding.
- The web core rejects author mutations after confirmed or still-pending deletion. A permanently rejected deletion does not freeze an otherwise valid message. Author checks and terminal deletion folding remain enforced when replaying signed events.
- New user-facing copy is plain Portuguese. Voice/media behavior is unchanged.

## Regression evidence

| Check | Before | After |
| --- | --- | --- |
| Mounted production web UI: body-only search | Failed: no message results | Passed |
| Mounted production web UI: edit separated from send draft | Failed: no independent edit dialog | Passed; refusal keeps edit text and cancellation keeps send draft |
| Native store: equal-score search ordering | Failed: insertion-order ties | Passed: deterministic reference ordering |
| Actual signed/MLS web client: edit after deletion | Failed: queue reported success for an ineffective edit | Passed: explicit refusal, unchanged outbox |
| Actual signed/MLS web client: edit after rejected deletion | Failed: failed deletion incorrectly blocked editing | Passed before and after saved reload |

The native mutation-dialog baseline was established by source inspection, not a dynamic pre-fix native UI run. The Swift state journey proves retry/draft state, not a running SwiftUI screen. The JSDOM tests mount production React components with an in-memory controller: they are behavioral UI tests, not relay E2E.

## Reproducing the focused checks

```sh
cargo test --locked -j2 -p roda-store search_tests -- --test-threads=1
cargo test --locked -j2 -p roda-ffi --lib tests::message_search -- --test-threads=1
cargo test --locked -j2 -p roda-ffi --lib universal_search -- --test-threads=1
cargo test --locked -j2 -p zoen-web-core --lib
bash scripts/test-apple-message-mutations.sh
cd packages/zoen-ui
npm ci --ignore-scripts
npm run typecheck:web
npm run test:web
```

The focused native search runs passed 4 store, 4 FFI mutation/search, and 2 existing universal-search tests. The web-core run passed 10 tests. The web suite passed 22 tests and typechecking. Swift mutation state, Swift parsing, Rust formatting and diff whitespace checks passed.

## Visual fixture, not E2E

```sh
node packages/zoen-ui/web/fixtures/search-edit.mjs --serve
```

Open the printed localhost address on the same laptop. The fixture renders the production messenger/editor/search components with in-memory data and an explicitly simulated refusal. Its banner identifies those limits. Inspected screenshots in the PR show message-body results and retained edit/send drafts after refusal. They do not establish pairing, encryption, transport, persistence or native UI correctness.

## Verification boundaries

No local simulator/emulator was started, no personal checkout was accessed, no merge/force-push was performed, and disk space remained above 5 GiB. Local Android Gradle execution was attempted but could not start because this laptop has no Java runtime. Native bindings must be generated from the changed Rust API; generated code must not be edited by hand. Full native compilation, the combined Rust checks and current-head CI are recorded separately in the PR as they finish.

Still required for product-level parity: physical Apple/Android search and failure/retry journeys; live paired-browser/native edit/delete and reconnect/reopen convergence; complete accessibility and mobile-browser layout checks. A green CI run and a labelled fixture do not substitute for those journeys.
