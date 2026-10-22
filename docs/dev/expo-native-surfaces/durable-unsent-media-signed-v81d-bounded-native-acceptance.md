[Native surface index](../expo-native-surfaces.md).

## Durable unsent media — signed v81d, bounded native acceptance

Source `aeb4127f7e4311f7d55fa358c5182e60f02b2dfd` adds encrypted device-local
attachment/voice drafts scoped by account/device/chat/thread, with CAS save ACKs
and atomic idempotent Send receipts. Rust owns durable bytes/metadata; JS holds
projections and disposable leases. Drafts are excluded from backup/history/link.

Isolated v81d recovers the edited **3.37-second** draft retained after v81c failed
Send. One compact Send projects `01M57ZXME73TF49F4CJZKCAXVS`; actual cold return
shows exactly one matching native row and no draft. A fresh ordered photo/video
batch restores both previews and separate “QA v81d photo recovery” / “QA v81d video
recovery” captions after cold. One Send projects two IDs recorded in the ledger;
another cold retains those IDs/no draft, with timeline 100% and the final receipt
above composer. These are local native rows, not independent delivery/byte proof.

Both preserving installs and public executable/JavaScript/strict-signature checks
pass before/after. Human Home four cards / recipe 6/8, six-page chat history at 100%,
old 0:11 Delivered voice and empty composer/closed keyboard remain. No human content
mutation, calendar read or permission change occurs. App 666/666 (zero skips,
including the macOS Foundation/Node actual-Swift codec regression) and
TypeScript/Oxlint/Oxfmt pass. All three exact-source Expo jobs pass separately;
unchanged Rust 463/463, including 19 media cases, keeps `6c338bcf` attribution.

V81b false prepare-error/reopen and v81c pending-trim cold/Apply/Discard followed by
invalidRecording Send remain partial diagnostics, never accepted candidates or a
fresh v81d trim replay. Final recordings have 24 isolated / 3 human steps, with
62 / 13 observations and 26 / 6 copied PNGs. Four owned flows were archived/retired.
The unmatched human Fixados wait and corrected Miniapps wait, plus overlay-selector
warnings, remain visible; no formal replay/E2E claim. Scoped tool servers stopped
only on the two QA targets.

Independent recipient/payload decode, offline media, microphone/ASR, induced native
storage/owner/erase/ambiguous-ACK failures, edited-PDF process-death recovery,
physical/provider/Push, broader accessibility/performance and full parity remain open.
Evidence: `/private/tmp/zoen-media-draft-native-qa-v81/runtime.json`,
`accepted-artifact-verification.json`, `final-checks.json` and `source-ci.json` in
that QA root. Earlier media/draft/offline/miniapp proofs retain exact attribution.
