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
cargo test --locked -j2 -p store search_tests -- --test-threads=1
cargo test --locked -j2 -p client-core --lib tests::message_search -- --test-threads=1
cargo test --locked -j2 -p client-core --lib universal_search -- --test-threads=1
cargo test --locked -j2 -p zoen-web-core --lib
bash scripts/test-apple-message-mutations.sh
cd packages/zoen-ui
npm ci --ignore-scripts
npm run typecheck:web
npm run test:web
```

The focused native search runs passed 4 store, 4 FFI mutation/search, and 2 existing universal-search tests. The combined local library command, `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 cargo test --locked -j2 -p store -p client-core -p zoen-web-core --lib`, passed 319 FFI, 13 store and 9 initial web-core tests, with zero failures or ignored tests. After adding the rejected-deletion regression, the final web-core source separately passed all 10 tests with the same profile settings. `cargo clippy --locked -j2 -p store -p client-core -p zoen-web-core --all-targets -- -D warnings` passed. The web suite passed 22 tests and typechecking. Swift mutation state, Swift parsing, Rust formatting and diff whitespace checks passed.

## Native binding / Mac build boundary

The host Rust library and UniFFI generator built successfully. Swift and Kotlin bindings were regenerated from the host library, and both expose `searchMessages(spaceId, query, cursor, limit) -> MessageSearchPage`; ignored generated files were not committed. Mac-only XCFramework creation and XcodeGen succeeded.

`DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer xcodebuild -project apple/Zoen.xcodeproj -scheme ZoenMac -configuration Debug -destination platform=macOS -derivedDataPath <isolated managed-checkout path> CODE_SIGNING_ALLOWED=NO build` failed on unchanged baseline `apple/Shared/Sync/PushBridge.swift:58`: **`ephemeral` is unavailable in macOS**. No search/mutation integration error was reported, but that does not establish complete compilation/linking. This unrelated blocker is tracked in [issue #119](https://github.com/EnzoTironi/zoen-native/issues/119); it was not changed in this front. No app was signed or launched, and no simulator target was built or run.

## Visual fixture, not E2E

```sh
node packages/zoen-ui/web/fixtures/search-edit.mjs --serve
```

Open the printed localhost address on the same laptop. The fixture renders the production messenger/editor/search components with in-memory data and an explicitly simulated refusal. Its banner identifies those limits. Inspected screenshots in the PR show message-body results and retained edit/send drafts after refusal. They do not establish pairing, encryption, transport, persistence or native UI correctness.

## Verification boundaries

No local simulator/emulator was started, no personal checkout was accessed, no merge/force-push was performed, and disk space remained above 5 GiB. Local Android Gradle execution was attempted but could not start because this laptop has no Java runtime. Native bindings were regenerated from the changed Rust API; generated code was not edited by hand. Current-head CI is recorded separately in the PR as it finishes. Both initial Android jobs failed before compilation at the current-main ancestry gate: GitHub's synthetic merge `d0edc2b` had parents `494237a` and PR head `d795d6b`, while main advanced to `3804b61a` through unrelated PR #107. Read-only fetching confirmed the stale synthetic merge; no local merge, rebase, force-push or gate change was used. The next genuine verification-document update publishes completed local results. Its synthetic revision and CI ancestry must be inspected; publication alone does not resolve or pass that gate.

Still required for product-level parity: physical Apple/Android search and failure/retry journeys; live paired-browser/native edit/delete and reconnect/reopen convergence; complete accessibility and mobile-browser layout checks. A green CI run and a labelled fixture do not substitute for those journeys.
