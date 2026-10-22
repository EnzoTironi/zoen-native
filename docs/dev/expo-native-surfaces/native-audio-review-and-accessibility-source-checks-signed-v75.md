[Native surface index](../expo-native-surfaces.md).

## Native audio review and accessibility source checks — signed v75

V75 source `c33f2760eadd1e9b4c9bfa252c14d4a778616906` keeps recording-stop retries,
preview ownership and pending-trim admission explicit. Apply reads the latest
interval, including before React renders; pending edits block Done/collapse/Send.
Publication awaits audio-mode restoration and rechecks the current owner/trim.
VoiceOver activation, quote navigation, metadata and lifecycle have compiled source
regressions; **native VoiceOver was not activated or accepted**.

Actual isolated native acceptance uses a fresh Safari-downloaded synthetic M4A,
imported through the real document picker, with no microphone/private media.
Native original duration is 3.72 s. First pending trim shows 1.87 s, removes the old
player and displays Apply/Discard in one native row; attempted collapse and Send
leave the pending editor intact. Discard restores 3.72 s. A second trim settles at
1.91 s; Apply keeps that exact displayed edited duration and native 0:01 preview.
Collapse returns the compact preview. One explicit compact Send creates one visible
0:01 voice/Sent and clears the dock. Cold restart and actual Home/Chats navigation
retain that sole visible bubble, old Read text and tombstones. No exact new message
ID, encrypted payload, independent recipient/Read or playback completion is proved.

The human preserving update retains Home 4 cards / recipe 6/8, the Chats carousel,
Bilingual six-page history at 100%, existing 0:11 voice/Delivered, empty composer at
its current larger font and closed keyboard. No human content mutations.
Both installed executable/JavaScript hashes and strict signatures pass before and
after runtime; all 795 source inputs remain unchanged. Source SHA256 is
`433775c732988025e4dc52ae2d9841c4ce3e30a57eedf76fc9fc03b9414fb865`.

App 571/571 and TypeScript/Oxlint/Oxfmt pass; unchanged native Rust retains the v74
444/444 attribution. Source checks include 14 added compiled audio cases / 70 focused,
8 iOS Babel/ReactCompiler accessibility cases and 8 platform-transform cases, with
meaningful pre-fix/guard-omission failures. Fault tests mock native ports. The v74
Web field-mount regression is corrected; actual `release-web-e2e` and `client` Expo
CI jobs pass on source `c33f2760`, not an inferred later documentation head.

Native 33 steps plus fixture 3 and human 6, with 32 copied PNGs, are three retired
one-use recordings, not formal reusable replay/E2E. The non-AX-visible voice-dock
ID wait and human Fixados/Miniapps selector mismatch remain harness diagnostics;
subsequent actual UI was verified. Evidence: `/private/tmp/zoen-voice-accessibility-qa-v75/`
`runtime.json`, `source-checks.json`, `native-observations.json`, `human-observations.json`,
`evidence-manifest.json` and `expo-ci-source-head.json`; the artifact verifier is
`/private/tmp/zoen-iphone-release-v75-verification.json`. Six scoped tooling services
and only the task-owned fixture server stopped; relay PID 19737 and the old consumed
peer were untouched. Native VoiceOver, OS fault injection, ASR/filler-removal success,
offline cold/reconnect, new encrypted-byte/recipient, physical/provider/performance
and full comparative product acceptance remain open. Earlier artifact proofs stay
unchanged historical evidence.
