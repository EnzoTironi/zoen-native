# Native media batch 17 — 21 October 2026

This is a bounded installed-app qualification, not full Telegram parity or
production acceptance. The Release simulator artifact was built from
`ebd4e0452b8fa16382467e854c66ed70b29df832`. Its 417 installed files matched the
saved artifact exactly; the app-tree digest is
`cd6134fd02d6e5ac97b68b7054e0fd805dfffc6e0cad706a9143340a36339472`.
Later qualification-document commits do not change those production inputs.

The target was the existing iPhone 18 Pro simulator
`591F1216-9B73-4BE1-807A-309DB937D6FA`, 402 × 874 points, with real local backend
services. No account reset, logout, application-data erasure, public deployment
or physical-device operation was performed.

## Build and source evidence

- Configured Rust core, UniFFI generation and Release iOS build passed. The
  installed core uses the existing exact DeckCraft resource-manifest tuple.
- `npm run check` passed with 1,530 tests, TypeScript, Oxlint and Oxfmt at
  `7f69f07e`. All 1,036 app/package source files remain byte-identical at the
  installed head. Later Rust changes have their own scoped HTTP/WS/storage
  proofs in the upload-refusal qualification; the Node check does not execute
  those backend paths.
- Expo 58 patch alignment was checked in a clean private install, with Doctor
  20/20. The actual root install, targeted local pod updates, idempotent CNG and
  final native linkage passed. No broad pod update or validation suppression
  was used.

## Executed journeys

1. Opened the upgraded app with the existing account, Home cards and night
   preferences. A stopped-app backup preceded installation. Read-only database
   comparison after the batch found all 122 original confirmed events, with
   identical hashes, sequence, timestamp and serialized-event digest.
2. Selected two newly generated geometric PNG fixtures, portrait B then
   landscape A. Their library bytes matched the original fixture digests.
   Entered a different caption for each in the native gallery and sent once.
   One album displayed B/A in the selected order with both captions.
3. Opened each child from that album. Both full images displayed without
   cropping in the existing single-image viewer. Long-pressing A and choosing
   Reply targeted A's caption; cancellation did not send a reply.
4. Opened A's thread and sent a fresh two-image album there. The thread displayed
   one album and the correct original message. After a process restart, that
   album and its caption remained available.
5. Selected the same two fixtures anew and used the native Photos menu's
   **Send without grouping** action. It sent once and produced two individual
   rows. Canonical events contained no album metadata for those two messages.
6. Confirmed exactly six new synthetic MessagePosted events and six File
   creations. Signed metadata preserved the two album orders, per-child
   captions and exact thread target. The sender's outbox and upload journal
   were empty after confirmation. This is sender/canonical-log evidence, not a
   separate recipient-device or remote object-store durability test.
7. Held an existing QA PDF in File, cancelled outside its extracted menu, then
   reopened its menu and chose Preview. Actual Quick Look rendered the PDF;
   closing it returned to the populated File sheet. X then closed the sheet.
   No File row was tapped for immediate sending.
8. Independently reviewed all 586 decoded frames of the recorded night-mode
   File journey. Outside cancellation left no selected-row highlight, and
   populated File rows remained through Preview/X dismissal. The Quick Look
   renderer's own brief black pre-render surface is recorded separately.
9. Switched to day mode and repeated File hold/outside cancellation. The row
   background matched the rest of the table. Returned to night mode with the
   previous accent, message and sky colors, automatic mode off, then left Home
   open in portrait with the keyboard closed.

Local receipts, screenshots, canonical-message assertions and build inputs are
under `output/telegram-attachment-port-v242/`. The independent frame proof is
`/Users/enzotironi/.codex/builds/zoen-native17-motion-review-v264/source-ready.json`.
These ignored local artifacts are evidence references, not public CI fixtures.
The original recording and FFmpeg best-effort timestamps differ; frame inspection
does not establish animation duration, FPS or pixel parity.

## Remaining scope

The authored UIKit XCTest cases were SDK-typechecked, not executed. Sensitive
interruption, search/caption return, both landscape orientations and several
earlier DeckCraft edit/export/recovery journeys have separate prior evidence;
they were not all repeated on Native17. Mixed-document and oversize album
fallback, injected decode failure and delivery refusal were not exercised in
this UI batch; their scoped contracts remain distinct from runtime acceptance.

An album with a child reply/reaction/reveal intentionally projects individual
canonical messages so that those actions remain reachable. The new paginated
native conversation viewer and the Codex/Workers AI provider changes are not in
this artifact. Physical camera/microphone/haptics, public infrastructure,
multi-device durability and complete platform/performance qualification remain
open. Full-app, production and scale acceptance flags remain false.
