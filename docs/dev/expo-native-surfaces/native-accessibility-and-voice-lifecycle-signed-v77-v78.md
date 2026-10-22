[Native surface index](../expo-native-surfaces.md).

## Native accessibility and voice lifecycle — signed v77/v78

V77 source `04bfddbb` adds voice-capture ownership/finalization fixes and localized
full-read calendar purposes alongside separate write-only purposes. Audio source
checks pass 70 cases / 40 compiled handlers with seven negative controls; calendar
plugin/localization/adapter checks pass 11 cases with two original-config failures.
Actual isolated microphone reset → first prompt → Deny → localized explanation →
ordinary composer recovery passes without capture or sending. Its native audit
**fails** Search 18.333 × 18.333 pt and generic receipt 14 × 14 pt hit regions, with
no whitelist. V77 remains diagnostic; the human target stayed v75.

V78 source `468111710b9f6ed5b995ae8da7f309ea912ed883` fixes the native search Button
Label child's configured 44 × 44 pt frame and exposes the receipt as StaticText.
Normalized native Search AX frame `[0.841, 0.071, 0.109, 0.05]` supports the configured
frame, without a separately exported exact CGRect. Actual touch opens/closes search
without query typing. Seven-category native audit passes with **zero issues/no
whitelist**, limited to the current synthetic conversation viewport.

Actual public `XCUIDevice.shared.voiceOverService` on iOS 27 makes 18 forward moves,
reaching message/audio, receipt **Enviada at 16**, message actions at 17 and attachment
at 18, then restores initially-disabled VoiceOver OFF. Playback activation, named
custom actions, rotor, physical-device and all-route/font/theme acceptance remain open.
Human preserving delivery retains Home 4 cards / recipe 6/8, six-page history at 100%,
old 11-second voice Delivered, empty current larger-font composer and closed keyboard,
with no content/account/preference changes. Isolated history/old 0:01 voice Sent remain;
no new recording, send or import. Both public executable/JavaScript hashes and strict
signatures pass before/after; 797 inputs have source SHA256
`af99210af059573d9a377b360196ddcc54d846b91d42eccf898999e71cdc75ff`.
App 603/603 plus TypeScript/Oxlint/Oxfmt and exact-source Expo client/release-web-e2e CI
pass. Unchanged Rust 444/444 retains v74 attribution; no all-CI/full parity claim.

Calendar signed plist/English/pt-BR purposes pass with reminders absent; there is no
shipped freebusy caller, so **native EventKit prompt/read is not accepted**. Paused
recorder/audio-mode failures remain source-only. V77's 8-step flow has a bad step-7
portable conversion; v78 isolated 8 steps have three post-XCTest coordinate fallbacks
and human 4 steps have one cross-tree warning. Five early screenshot temporaries were
lost before copying; AX records, XCTest final screenshots and nine copied tool PNGs
remain. Recordings are retired one-use diagnostics, not formal E2E. Six scoped services
stopped and only two owned isolated harness apps were removed; Zoen data/relay remain.

Evidence: `/private/tmp/zoen-native-lifecycle-qa-v77/runtime-summary.json` and
`/private/tmp/zoen-native-accessibility-qa-v78/runtime.json`, with separate
`/private/tmp/zoen-iphone-release-v{77,78}-verification.json` artifact receipts.
Earlier media, peer, draft and dedicated v76 offline proofs keep their attribution.
