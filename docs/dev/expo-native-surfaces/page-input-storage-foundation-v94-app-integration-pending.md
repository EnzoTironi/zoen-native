[Native surface index](../expo-native-surfaces.md).

## Page input storage foundation (v94; app integration pending)

Core source `68b9900c` retains exact native input bytes, UTF-16 selection and IME
composition in the existing encrypted private Page/Plan journal. Captures are
account/device/item scoped, revision checked and bounded together with the saved
document. Page Apply consumes the capture in the same write as the edit receipt;
Discard is exact and retryable. Repeating an older request returns its original
acceptance without replacing newer input. Untouched legacy journals keep their
existing envelope; input activation uses a distinct authenticated v3 domain.

All 509 FFI tests, strict Clippy, Rust formatting, generated client types and the
client's TypeScript/Oxlint/Oxfmt/777 tests pass. New tests use actual SQLite,
encrypted journals and cold Engine reopen; injected write failures and lost
responses preserve the previous state. Evidence:
`/private/tmp/zoen-page-input-v94/verified-core-foundation.json`.

Separately, an external enriched-markdown producer candidate passes 16 real UIKit
tests and public Swift 6 conformance, including composition, paste, undo/redo,
retention failure/retry and final immutable capture. Evidence:
`/private/tmp/zoen-page-native-producer-v94/native-seal-api-v94-06.json`.
The separate Swift 6 host proof compiles the lease against real generated UniFFI
bindings and Rust, then verifies capture recovery across processes, an actual SQL
write failure, exact retry, completed Apply/Discard replay and invalidation:
`/private/tmp/zoen-page-input-bridge-v94/host-proof/acceptance.json`.
The producer patch is not installed in the app. Native session ownership,
recovery UI, safe teardown and source/plain producers remain integration work;
canonical Plan Apply remains closed. No combined iPhone recovery journey is
accepted by these separate results.

### Session retirement and empty input release (v95)

Graceful account replacement keeps the existing UI, snapshot and native owner
mounted until admitted draft cleanup and native close acknowledge completion.
Separate work generations stop new work and fence late responses. Failed cleanup
or close retains the previous owner and exposes a retry notice. Concurrent
replacement coalesces; confirmed erasure drains superseded closes before an empty
account reopens. Provider teardown can escalate a native close after admitted
message and media writes finish.

The core can also release an exact empty input binding after a rejected first
checkpoint. Its encrypted receipt survives cold reopen and lost responses; replay
cannot discard a newer capture. Plan projections expose pending input without
granting edit authority. All 512 FFI tests and the client's 791 tests pass, alongside
TypeScript, Oxlint and Oxfmt. These results validate source behavior, not a newly
signed application build.

The next enriched-markdown producer and editor bridge remain external candidates.
The v95 producer's Swift public API check passed, but its UIKit run could not start
because the prepared React frameworks and archive were absent. The unfinished
bridge was preserved externally and removed from the app. No producer patch,
raw-input bridge or recovery UI was installed. Evidence is kept separately under
`/private/tmp/zoen-page-editor-v95/` and
`/private/tmp/zoen-page-native-producer-v95/`.
