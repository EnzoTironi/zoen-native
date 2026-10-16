# Android / iOS UI parity

## Status

Implementation in progress, based on `ui/native-glass` PR #124 (46de5a0).
This directory does **not** yet contain verified side-by-side screenshots of the parity changes. The older native-glass evidence is not evidence of this change.

## Capture matrix

`AndroidParityScreensTest` captures light and dark variants of onboarding, chats, direct chat, group chat, thread, Spaces (communities in Chats, as on iOS), profile sheet, settings, devices and search. It uses an isolated demo, Compose idleness, artwork settling, and `CommittedWindowCapture`'s foreground-window frame commit / PixelCopy check. Run on API 28 and API 35 using the existing Android CI matrix. The capture test is not a substitute for gesture/action journey assertions.

Expected Android output: `android/app/build/outputs/device-evidence/android-parity/` (the device runner collects app evidence). Pair each fresh image with an iOS capture of the same demo, screen, appearance and state. Inspect every image for first-frame content before including it here; reject black/blank or mismatched-state images. Keep API level and appearance in filenames. Motion and haptics need journey/device verification in addition to static images.

## Scope implemented

- Shared SwiftUI-like body, callout, footnote and caption type scales within native Material 3.
- Conversations / communities: native cards and controls, correct empty states, one-layer search/archive Back handling.
- Onboarding / creation: native controls, selection gates, trimmed titles and duplicate-submit protection.
- Chat / threads: reduced-motion behavior, accessible actions, swipe/long-press haptics, persistent thread drafts, reaction participant and replacement sheets, friendly mutation errors.
- Settings / profile / devices: native controls and profile sheet, edit-level Back, device loading/cards and duplicate-check protection.
- Sky background files are unchanged.

## Verified locally

- `python3 scripts/test-android-core-contract.py`: 6 tests passed.
- `cd android && ./gradlew :app:assembleDebug :app:testDebugUnitTest :app:lintDebug :app:assembleDebugAndroidTest --max-workers=2` with JDK 17: passed (120 unit tests, no failures/errors/skips; both APKs compiled).
- `git diff --check`: passed.
- Initial lint analysis crashed when this new test was written during analysis; the stable-source rerun above passed without suppressing checks.

PR publication was attempted but refused because this direct-folder Session has no pinned provider checkout. No repository attachment or remote alteration was performed. Consequently this branch has not triggered CI, and no API 28/35 runtime results are claimed.

## Outstanding verification

- Full API 28 and 35 instrumentation suite and fresh capture matrix.
- Matched iOS captures and visual review of spacing, type, colors, radii, icons and layout for every screen.
- On-device motion/send animation, haptics, keyboard, dark mode and Back journeys.
- Profile call/video/report actions are not implemented by this increment.

Do not describe this increment as complete UI parity until those checks and evidence are delivered.
