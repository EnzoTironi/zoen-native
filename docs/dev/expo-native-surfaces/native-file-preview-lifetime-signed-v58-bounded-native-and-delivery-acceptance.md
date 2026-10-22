[Native surface index](../expo-native-surfaces.md).

## Native file preview lifetime — signed v58, bounded native and delivery acceptance

Source `3a5f5485` replaces early Expo preview completion on iOS with a public native
Quick Look owner. Each preview transfers a separate exact owned copy and retains it
through viewer/Share dismissal and verified deletion. Close and erasure invalidate
presentation admission and await active, queued and retained copies; failed deletion
remains owned for retry. Canonical cache validation accepts OS ancestor aliases such
as `/var` and `/private/var`, while rejecting substituted generated roots and files.

All 428 application tests, TypeScript, Oxlint and Oxfmt pass. Eleven real Foundation
lifetime/filesystem scenarios and strict native typechecks pass. Source negative
controls detect premature completion, skipped concurrent cleanup, skipped queued-file
drain, swallowed deletion failure and the rejected legitimate cache alias. These
establish source ordering and validation, independently of the UIKit observations.
Logs: `/private/tmp/zoen-quicklook-lifetime-alias-app-check.log`,
`/private/tmp/zoen-file-preview-alias-positive.log` and
`/private/tmp/zoen-file-preview-alias-ios-typecheck.log`.

The signed ARM64 simulator Release has 769 frozen inputs and source SHA256
`21a7b6b8737a575a0d0790c70a5e2ff0974d1cf251e818bc35fc5ed1920132b5`.
Exact installed executable/JavaScript hashes and strict signature pass before and
after isolated UI. Actual ordinary PDF preview displays both pages and totals 53
and 161. Share shows the 4 KB PDF, opens the real Save to Files destination picker,
and one Save returns to the still-open viewer. Close, reopen and close pass. The
forwarded photo also opens; tapping the image reveals hidden native controls and
Close returns without reactions. No Zoen send or import occurred; the only content
mutation was one synthetic OS PDF export. No exported-copy byte hash is claimed.

Cold restart retains the exact incoming/outgoing PDF captions below media and one
each of the prior forwarded video and PDF. Playback keeps its v56 attribution.
Evidence: `/private/tmp/zoen-file-preview-qa/v58/runtime.json` and
`/private/tmp/zoen-iphone-release-v58-verification.json`. The 45-step diagnostic has
eight cross-tree warnings and three unmet assumptions, corrected by scrolling to
page two, selecting the real Save control and revealing photo controls. Incorrect
exported selectors were retired; no formal replay passed. Two explicitly undelivered
touch attempts recovered through a device-scoped transport reconnect, without an
app reset. Human preserving v58 installation and exact installed executable/JavaScript/
strict signature pass before and after UI. Restart retains four Home cards, recipe
6/8, Bilingual history, existing audio/delivered receipt, latest 100% and empty input.
The native attachment dropdown opens and dismisses without selection. No content,
preference, account, capture, send or import mutations occurred there. Its 13-step
diagnostic has two cross-tree warnings and was retired without a replay claim.
Evidence: `/private/tmp/zoen-file-preview-qa/v58/human-runtime.json`.

Induced account close/erasure while Quick Look or Share is active, physical-device
behavior and Markup/new-version persistence remain pending. Historical v57 failure
remains attributed to that artifact.
