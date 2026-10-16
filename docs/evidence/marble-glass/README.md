# Shared marble glass: second pass

Original rounded-lens math replaces the white colour-effect overlay. Apple now samples the local material field with a Metal `layerEffect`; Android uses the equivalent AGSL field. Both use IOR 1.5, quadratic edge displacement, ±0.04 RGB dispersion, Schlick Fresnel, a top-left glint, rim, inner shadow and opposite-side caustic. Full surfaces use 26% black shadow, offset (2,41), blur 60, spread -8; small controls have a shallower profile and compact shadow. Custom Apple glass surfaces now use the shared modifier rather than independent system glass effects. Native toolbar glass is system-owned.

The invite action is a secondary glass button, still labelled “Colar” in Portuguese. Clipboard access occurs only on a tap. The system may request paste permission; this permission interaction was not exercised by the capture test.

## Verified captures

One iPhone 18 Pro simulator, iOS 27.0, Xcode at `/Applications/Xcode.app/Contents/Developer`. The simulator was shut down after capture. Screenshots are genuine XCTest attachments, not mockups; demo timestamps/countdowns differ between runs.

| Screen | Main before | Changed after |
| --- | --- | --- |
| Onboarding CTA + invite | ![](before-onboarding.png) | ![](after-onboarding.png) |
| Chats list | ![](before-chats-list.png) | ![](after-chats-list.png) |
| Group chat card | ![](before-group-chat.png) | ![](after-group-chat.png) |

## Checks

All commands used `DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer` where applicable.

- PASS: `python3 scripts/test-glass-optics.py` — four tests. Before replacement the integration contract failed because main had no sampled layer; after replacement all pass. CPU equation and source-contract checks are not GPU equivalence tests.
- PASS: `bash scripts/check-apple-glass.sh` — Metal compilation and Swift type checking for iOS and macOS.
- PASS before and after: `xcodebuild test -project apple/Zoen.xcodeproj -scheme ZoenUITests -destination 'platform=iOS Simulator,id=E748ECE3-BD66-46CF-A20E-00BEFB24EA2B' -derivedDataPath .reason/glass-build -parallel-testing-enabled NO -only-testing:ZoenUITests/UIPolishJourneyTests CODE_SIGNING_ALLOWED=NO` — two UI journeys, onboarding/chats scrolling, group/thread/actions; final run includes the localized, hittable invite action assertion.
- PASS: `git diff --check`.
- FAILED: full `ZoenMac` build — unchanged `PushBridge.swift:58` references `.ephemeral`, unavailable on macOS. No unrelated push-notification fix is included. The isolated macOS material checks pass.
- NOT RUN: Android build / runtime screenshots — Java runtime unavailable on this Mac. No SDK/JDK installation attempted.
- NOT VERIFIED: hosted CI, dark mode, reduced transparency, physical devices, Android GPU output, clipboard permission flow, visual parity with the reference video.

## Material limitation

This is local-field refraction, not arbitrary backdrop capture: SwiftUI's system material rendered black when sampled inside `SwiftUI::Layer`, so the live blurred backdrop stays outside the lens shader. Compose's drawing modifier likewise has no backdrop texture. The lens bends a local tint/illumination field and adds volume on flat backgrounds; actual scene refraction behind the surface is still an unmet requirement. Do not describe this implementation as full backdrop refraction or claim Android/iOS pixel parity.

Generated bindings were produced only by `scripts/build-core.sh` and have no tracked diff. No force-push, merge, sudo or brew use. The personal `~/zoen/zoen-native` checkout was not accessed. Free disk remained above 263 GiB at the final check.
