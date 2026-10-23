# Telegram iOS conversation integration

## Historical receipt archive

The seven machine QA receipts described below are no longer tracked. Their exact
bytes, along with the original README, were preserved before untracking under
`/Users/enzotironi/.codex/builds/zoen-owned-license-qa-hygiene-v411/original-records/`.
The sibling `archive-manifest.json` records every path, Git blob, size, mode and
SHA-256; its SHA-256 is
`b594873c578c1a05b8e2098cd48b0834a30a4be875d552a5b5df5da37d97465c`.
The same original files remain recoverable from the
[merged Git snapshot](https://github.com/EnzoTironi/zoen-native/tree/f7d716158c249ab2c8cb074cacdad073d95886ac/output/telegram-ios-conversation-2026-10-20).

The descriptions and PASS/PENDING boundaries below are historical evidence from
20 October 2026. Archiving does not rerun those checks or establish that the
temporary logs, binaries and native outputs referenced inside each receipt remain
available. This README and the separate Telegram feature-matrix catalog remain
tracked references.

## Recorded integration

The reference is Telegram-iOS `f1dd7a2dbd02cbbf513e75d5695d8d36d1cf5838`.
The [implementation map](../../docs/product/telegram-ios-conversation-implementation.md)
distinguishes applied source contracts from native acceptance.

- `app-check.json`: integrated types, lint, formatting and1179 real compiled/policy
  tests PASS. This includes the composed Photos, graphics, swipe and header paths.
- `source-integration.json`: exact33-file integration, including the two explicitly
  composed overlapping files. The private first composition attempt stopped before
  any LIVE write because its additional verification used an incorrect mock name;
  the corrected composition compares both deltas byte-for-byte.
- `header-source-integration.json`: exact four-file header and independent review.
- `native-model-fixtures.json`: actual Swift cache and receipt-model execution on
  macOS; two meaningful negative controls failed and restored source passed.
  This is not UIKit or iOS bridge execution.
- `configured-core-build.json` / `configured-core-proof.json`: configured Release
  core build and exact five generated outputs after the licensing metadata change.
- `pods-build.json`: actual native project dependency regeneration, including the
  newly added Swift files.

The prior Release at `005b8b54` showed actual native typing/send, inline timestamps,
interactive keyboard dismissal and scoped search. Its native header title was
visibly off center. A full search replay passed19 steps after explicit focus and
exact AX label-plus-value readiness checks. The earlier failures are retained in
the private run: a fast run had no query, and a later exact matcher expected only
the value while AX projected both label and value. The full successful replay
began with the normal keyboard already open; the first readiness execution also
proved query entry from the initially closed keyboard. No AI request was sent.
Those results describe the prior artifact, not this newly integrated source.

The next Release's compile, installed bundle verification, UIKit geometry,
Photos permissions, native swipe/haptics and receipt animations are PENDING.
The extracted context-menu prototype is not enabled. Full product, production
and scale acceptance remain false.
