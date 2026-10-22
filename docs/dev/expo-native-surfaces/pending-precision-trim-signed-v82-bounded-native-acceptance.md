[Native surface index](../expo-native-surfaces.md).

## Pending precision trim — signed v82, bounded native acceptance

Unapplied trim is an explicit transaction: Undo/Redo and unrelated audio edits wait
for Apply or Discard. Live handlers check current session/pending state; ASR metadata
may complete without replacing the pending range. Source `c71383a7` leaves the same
encrypted device-local draft model and Rust unchanged.

Actual isolated iPhone QA applies **A 3.45 s** to a **3.72 s** original, then stages
**B 0.27–3.54 s** in original coordinates. Undo is inert before cold restart;
after cold, B and A restore and Undo/Send remain inert. Explicit Discard retains A;
Undo now returns 3.72 s and Redo 3.45 s. A second explicit Apply yields 3.27 s.
One compact Send projects `01M581XGFYW21NM1D2GV0MWYF4`; another cold returns exactly
one matching native row, no draft, empty composer/closed keyboard and timeline 100%.
These are visible durations/local rows, not independent payload or physical-duration proof.

Both preserving v81d → v82 installs and public executable/JavaScript/strict-signature
checks pass before/after. Human Home four cards / recipe 6/8, six-page history at 100%,
old 0:11 Delivered voice, larger text and empty closed-keyboard composer remain;
no content, calendar or permission changes occur. App 669/669 (zero skips),
TypeScript/Oxlint/Oxfmt and all three exact-source Expo jobs pass separately.
Focused 86/86 voice tests overlap the app total; three new regressions failed on
prior source. Rust 463/463 keeps `6c338bcf` attribution, with no new Rust suite.

The 42-step isolated / 6-step human recordings and 49 / 4 observations retain
32 / 4 PNGs. Overlay-selector retargeting, corrected initial editor wait, inert first
slider tap and preflight-helper correction remain diagnostic; no formal replay/E2E.
Word/filler/pause/selection/stale-callback/ASR boundaries have source proof, with no
new native ASR, microphone or accessibility acceptance. Independent peer/decode,
PDF Markup recovery, physical/provider/Push/performance and full parity remain open.
Separate **v82a offline-media work is preparation only**, not proof in this checkpoint.
Evidence: `/private/tmp/zoen-pending-trim-native-qa-v82/runtime.json`,
`accepted-artifact-verification.json` and `source-ci.json` in that QA root. All v81d
and earlier results retain their exact original artifact attribution.
