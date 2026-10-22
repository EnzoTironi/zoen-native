[Native surface index](../expo-native-surfaces.md).

## Fullscreen miniapps and native consent — signed v80

V80 source `131e81dea72f7aec846be5a66f229a80b4cda40d` passes bounded native
portrait full-width HTML/one primary WK scroll without an outer RN scroll wrapper.
An oriented landscape screenshot verifies safe side/top/bottom insets. Dedicated
Details retains widget/Siri/actions and native Home/conversation pin/organizer/history
controls; no setting is changed. Back creates a fresh private session at Today/idle,
so previous calendar conflicts are not reused.

The actual opaque native sheet displays purpose, scope and three choices. Deny returns
a localized error; retry works. Background/return of an already-presented sheet and
interactive swipe dismissal deny; a later explicit Allow once reads the existing
Monday fixture and marks 11:00/11:30. Resetting **only isolated Zoen Calendar permission**
as setup, then restarting, proves item Allow once fully dismisses before the real OS
calendar prompt; actual Allow Full Access succeeds. Business 09:00 selects without
booking. No new messages, bookings or calendar events are created during v80 QA.

Both public executable/JavaScript hashes and strict signatures pass before/after
preserving v79 → v80 installs. Isolated five Home miniapps / recipe 0/8 and existing
fixtures remain. Human Home four cards / recipe 6/8 / Exact BRL QA card, original
pinned miniapps, six-page chat history at 100%, old 0:11 Delivered voice/prior Delivered
texts and empty larger-text composer/closed keyboard remain; no human calendar
access or permission changes. Six scoped tooling services stop; relay PID 19737 remains.

App 629/629 plus TypeScript/Oxlint/Oxfmt passes. Consent 17/17 is a subset: 12 queue
and five compiled-hook/renderer/installed-SDK-wrapper tests, with eight meaningful
source negatives. Exact-source Expo `schedule-miniapp`, `client` and `release-web-e2e`
jobs pass separately. Rust is unchanged; prior full 444/444 keeps v74 attribution.

Native preappearance cancellation, two queued sheets and stale/double callbacks remain
source-only; the real background probe covers an already-presented sheet. Foreground
may briefly show an inert denied sheet so actual dismissal can retire its barrier;
no timer/binding callback grants. Fresh HTML keyboard geometry, VoiceOver, physical
and Android/Web manual acceptance, full FFI and comparative performance remain open.
Retired one-use recordings retain tree/coordinate warnings and rediscovered launch/
foreground/animated-tap races; no formal replay/E2E or full product parity claim.

Evidence: `/private/tmp/zoen-miniapp-native-ui-qa-v80/runtime.json` and canonical
`/private/tmp/zoen-iphone-release-v80-verification.json`, with both preserving-install
receipts and exact-source CI in the QA root. Earlier calendar, accessibility, media,
peer, draft and offline proofs retain their original artifact attribution.
