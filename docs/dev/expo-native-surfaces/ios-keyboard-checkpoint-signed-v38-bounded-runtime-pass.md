[Native surface index](../expo-native-surfaces.md).

## iOS keyboard checkpoint — signed v38, bounded runtime pass

On iOS, `KeyboardChatScrollView` owns native keyboard insets and reserves the full
measured composer height: 96 points in the controlled fixture. Supported
`KeyboardStickyView` movement, the 26-point safe-area correction and inline Expo UI
Hosts ignoring the keyboard safe area keep layout stable without per-frame
JavaScript measurement. The initial end anchor observes native content/viewport/
inset/Y before FlashList suppression, deduplicates within one point and cancels
on first focus, drag, native user scroll, search, reply/edit/jump, blur or scope
change. It uses layout/scroll events and cancellable frames, with no fixed timer
or private API. Other platforms retain their fallback.

Signed v38 source `f7682098` has 749 verified inputs, manifest SHA256
`d54125cc4742b81ec7318f520797c41f25f03d18ad725f8959bd151ec6219d34`.
Build, archive, strict signature and exact preserving-installed executable/JS
verification passed on isolated 388CA and human E2B. JavaScript is exactly v36's
6,406,723-byte bundle, SHA256
`9b5853ec029bd5f3edaf2b477df4cc9577c86bd8c856fc1d6eaf42d80ccb092d`.
The full 254-test app check, TypeScript, Oxlint and Oxfmt passed
(`/private/tmp/zoen-v38-pr1608-app-check.log`). Actual bounded outcomes:

- Two fresh entries without swiping reached 100%; reply badge bottom 0.870 cleared
  input top 0.902. Portrait field Y was 486 points, height 52 and keyboard top 546:
  an 8-point gap. Landscape field Y was 118 points, height 52 and keyboard top 178,
  giving the same gap.
- Normal opening/hiding and immediate interactive dismissal passed observation.
  Reviewed video `1792284820127` shows continuous field/keyboard progression
  without the earlier fixed-field/jump. Video `1792284864960` supports two settled
  partial/reverse trials, including a repeat after 20 seconds, with the keyboard
  remaining open; settled dismissal passed the structural flow. Keyboard-open
  landscape rotation and portrait return passed. Transient 15–25-point gaps remain;
  no perfect timing or full motion/performance claim is made.
- After user gestures and rotation, 96% is the retained reading position, not a
  latest-message assertion. Actual zero-result search/fragment replay belongs to
  v36: `qazxv36missing` exposed AI without invocation or send, and closing restored
  the empty draft. The identical v38 JavaScript does not make this a v38 rerun.

Exact metrics, recording paths/hashes and limitations are in
`/private/tmp/zoen-message-polish-qa/v38/runtime.json`; signed artifact and delivery
proof is `/private/tmp/zoen-iphone-release-v38-verification.json`. Human E2B received
a preserving verified v38 update after a fresh, user-confirmed idle preflight.
Restart retained four Home cards, 6/8 progress and Bilingual QA history. Fresh chat
without a swipe reached 100%; existing audio/receipt bottom 0.856 cleared input top
0.897 by about 36 points. The native anchored Menu opened/dismissed without selection,
imports, sends or account mutations. Its v32 delivery is now a preserved historical
receipt, not the current human artifact. Original encrypted edited-voice and selected
thread-photo proof still belongs to v29; those media were not resent here.

The earlier isolated trials retain their bounded attribution: v33 failed with a
shrinking composer; v34 passed one fresh entry and a 49–50-point opening message
gap but native controls double-avoided the keyboard; v35 passed control alignment
and Menu opening but failed fresh entry at 86%. V36 passed three fresh entries,
8-point open-keyboard geometry and the search fragment, then failed interactive
dismissal. V37's custom `Animated.View` consumer of existing move/interactive/end
events reproduced that stall and was rejected. Card opacity contributed to late
reappearance; those videos alone do not prove frozen native keyboard events.

V38 retains precisely the two Swift files from open, unreviewed upstream
[PR #1608](https://github.com/kirillzyusko/react-native-keyboard-controller/pull/1608),
commit `daf3c0081c891d442cf84cccda6ef36226276de1`, related to
[issue #1563](https://github.com/kirillzyusko/react-native-keyboard-controller/issues/1563).
Patch SHA256 is `cfd86881125882e3f46edb8f501af748b6dfb30b79d463b85111f6dbe7f1439a`.
Pinned `patch-package` 8.0.1 applies it via `postinstall` command
`patch-package --error-on-fail`; exact application/reverse/reapplication were verified.
No extra position math or private API was added. These observed paths support
retaining the pinned native backport, without establishing the application's exact
root cause or upstream approval. Physical devices and future upgrades still need
verification. At that checkpoint, SwiftUI virtual accessibility frames could remain stale after
ancestor transforms despite working Host hit tests/Menu. The later v56 native-bottom-layout
change above corrects the selected portrait controls and landscape attachment target;
actual full VoiceOver impact remains unconfirmed. Full accessibility, motion/performance, thread/multiline/cold and
comparative product acceptance remain pending, with no blanket parity or CI claim.
