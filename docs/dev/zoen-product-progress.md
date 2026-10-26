# Product progress

## Current execution — 26 October 2026

Use the [complete roadmap checklist](../roadmap/README.md), its
[screen and Telegram review](../roadmap/index.html), and
[issue #170](https://github.com/EnzoTironi/zoen-native/issues/170) for current work.
PRs #632 and #633 are merged at `6c02022d` and `477ac2f3`. Product App718 is
`b7ab2265` with the retained Core707/`eb62cdaf`; it does not contain #632's Core.
Agents and conversation are active. Apps, editors, shared creation and App SDK
expansion are paused by Enzo. All 30 journeys, ten creation capabilities and six
platforms remain in the final scope, with all global acceptance flags false.

The following records are historical evidence at their named sources. They do
not describe current PR state, active workstreams or final product acceptance.

## Historical checkpoint — 20 October 2026

Finish iOS first against the real backend; Web/Android and desktop follow later.
See the [goal and milestone exit criteria](zoen-complete-product-goal.md#execução-atual--26-de-outubro-de-2026).
The draft at that checkpoint was [PR144](https://github.com/EnzoTironi/zoen-native/pull/144).
App source `bd10bf35` covers1,039 cases across the batch and an isolated retry:
1,038 passed in the broad run; one Swift helper compiler deadline failed during
overlapping compiler work, then the same-source isolated probe passed. The broad
`npm run check` exit remains failed in its receipt. Types, Oxlint/Oxfmt and the iOS
Release build pass. Preserving install matches all430 sealed files and strict
signature. Release04 native checks qualify all-card44pt X/Pin, Home drag/pin/hide,
canonical chat cards and catalog/permissions/dirty-close return without the old54pt
gap. Release07 corrects native card gesture frames; secondary-card editing drag
works and a real cold restart preserves chat order C,B,E. Initial normal-mode
secondary-card long-press remains inconsistent and unaccepted. Recorded diagnostic
walkthroughs contain cross-tree warnings and are not replay-qualified.

The [Signal matrix](signal-test-contracts.json) contains51 overlapping scenario
families across six pinned repositories. Source study is bounded; upstream suites
were not executed or copied. At `4f175f40`, two original test-only regressions add
complete canonical backup/new-device/cold reopen/media verification and actual
malformed-wire/signature-rejection receive continuity without false Delivered
or duplicate rows. All543 client-core library tests and strict all-target
Clippy pass at `4f175f40`; inherited partial classifications remain partial. Earlier541-test core and strict Clippy evidence
retain their own source, and the new cfg(test) modules do not alter runtime policy.

DeckCraft's private pinned engine bridge passes11 real-engine regressions.
Canonical encrypted binary/MCP storage passes10 actual core cases and36 tests
through the installed SDK. The optimized managed WASM build completed, with exact
JavaScript/WASM hashes and pinned source/license/font evidence. Native UI/bootstrap,
core resource registration and real certified-agent execution are in parallel
implementation. The complete iPhone/cold-recovery/PPTX/PDF/share journey and the
other ArtCraft editors remain pending. Earlier photo and landscape proofs retain
their separate immutable artifacts. All global readiness flags remain false.

## Earlier evidence

- [Native releases, linking and Android retirement through v131](zoen-product-progress-native-v131.md).
- [Bounded App UI, naming, validation and backend environment](zoen-product-progress-apps-and-backend.md).

These records preserve their original scope; their old PR states are historical.
