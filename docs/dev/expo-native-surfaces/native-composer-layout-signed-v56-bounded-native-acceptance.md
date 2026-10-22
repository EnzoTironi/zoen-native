[Native surface index](../expo-native-surfaces.md).

## Native composer layout — signed v56, bounded native acceptance

Source `af97fbff` replaces the composer's layer-only movement with animated native bottom
layout, driven by the same keyboard-controller height/progress clock. Native chat insets,
constant measured composer reservation, safe-area correction and rail motion remain.
There is no second keyboard animation or per-frame JavaScript measurement. This lets hosted
SwiftUI controls observe their actual position: in the portrait fixture, actual native
button and Host centers both reach Y512, replacing the earlier stale accessibility Y814.
Bottom updates now perform native layout rather than transform-only compositing; its cost
has not been profiled.

The 766-input signed simulator Release has source SHA256
`5288d408d542008294b71c5dfe8735360d7cb29f8a659236b66b4ed6c0313ecb`.
Both QA simulators received preserving installs with exact executable/JavaScript hashes and
strict signatures verified before and after UI. All 415 application tests plus TypeScript,
Oxlint and Oxfmt pass (`/private/tmp/zoen-v56-app-check.log`). Rust is unchanged; its v55
results retain their original attribution rather than becoming a new run.

Actual isolated checks accepted:

- Native attachment Menu opens with the keyboard up. Conversation search `QA` shows `1/7`,
  moves to older `2/7` and back to newer `1/7`. A fresh zero-result `Zo` offers Ask Zoen,
  without invoking AI.
- Actual native Send creates exactly one `QA v56: keyboard aligned.` message in Message
  Studio QA, canonical ID `01M56ND0QEPW6EMK8XHDSTN87T`, with receipt Enviada. No further
  recipient-delivery claim is made. The composer empties and history reaches 100%.
- Cold Home retains four cards; Studio retains that message, latest 100% and an empty
  composer. Two 30fps clips show no obvious jump in the reviewed paths; they are not FPS
  or comprehensive motion evidence.
- Landscape attachment accessibility geometry agrees with the actual native HostingUIButton
  despite its Host safe-area offset. The landscape microphone target remains unproven.

Human E2B had an empty-draft/latest-100%/audio-idle preflight. Preserving delivery and normal
restart retain four Home cards, Recipe 6/8 and Bilingual QA history/audio/delivered-to-all
receipt at 100%, with empty input. Focus only with its existing larger text places native
record-voice center `(364.1667, 512)` at the Host's `(364, 512)`; input normalized Y0.551,
height0.064, and cards are absent while input is focused. Back and reopening restore the closed
keyboard, latest 100% and empty composer. No text was entered, audio captured, or content,
preferences, send or account data changed on that account.

Evidence: `/private/tmp/zoen-composer-accessibility-qa/v56/runtime.json` and
`/private/tmp/zoen-iphone-release-v56-verification.json`. The isolated 43-step diagnostic
has four cross-tree warnings and one recorded unmet obsolete `conversation-search-input`
wait, corrected to `message-input`. A separate premature refocus/Return-visible wait also
failed; the second rail refocus is not accepted. Human seven-step diagnostics have one
cross-tree warning and no recorded unmet waits. Original YAML is saved outside the repo,
and executable flows are retired. These are bounded observations, not formal replays.
Full VoiceOver, broad Dynamic Type, physical-device and landscape-microphone checks,
interactive/multiline/thread/refocus coverage, measured performance and comparative parity
remain pending. Earlier v38 keyboard and v55 profile/overlay/Rust evidence remains historical.
