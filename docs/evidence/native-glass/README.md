# Apple native glass evidence

Captured on iPhone 18 Pro, iOS 27.1 simulator `E748ECE3-BD66-46CF-A20E-00BEFB24EA2B`, using Xcode `/Applications/Xcode.app` (`DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer`).

## Screenshots

| Screen | Before | After |
|---|---|---|
| Onboarding name + keyboard | [Before](before-onboarding.png) | [After](after-onboarding.png) |
| Chats | [Before](before-chats.png) | [After](after-chats.png) |
| Group chat | [Before](before-group-chat.png) | [After](after-group-chat.png) |

The original six images were visually inspected. The three after images were refreshed after the native sizing correction and visually inspected again; [compact glass Undo](after-compact-undo.png) was also captured and visually inspected. Screenshots are unmodified XCTest attachments: onboarding from `OnboardingKeyboardJourneyTests`, chats/group from `InvestorShowcaseTests/testCaptureInvestorLightPT`. Demo timestamps, countdowns and mascot animation naturally differ. The onboarding CTA no longer renders the custom swollen gradient pill; it uses the native prominent glass button. Group/chat content and layout remain intact.

The before app was built from a `git archive origin/main` at `71bc1ebb857e7756ba659f64da1b962bc289fcb6` in `/tmp/zoen-glass-baseline`, not by switching the working checkout. XcodeGen generated its ignored project. Generated RodaFFI framework and Swift bindings are excluded from Git, so the existing local generated artifacts were copied into the archive after confirming `git diff origin/main -- crates Cargo.lock apple/Packages/RodaCore` was empty. This establishes baseline Swift UI source, not an independently rebuilt Rust artifact.

Direct simctl launch captures initially contained black content; they were replaced by the rendered XCTest attachments above, not used as evidence.

## Checks

All xcodebuild commands used `CODE_SIGNING_ALLOWED=NO`; builds used Debug defaults.

- PASS baseline iOS: `xcodebuild -project /tmp/zoen-glass-baseline/apple/Zoen.xcodeproj -scheme ZoeniOS -destination 'platform=iOS Simulator,id=E748ECE3-BD66-46CF-A20E-00BEFB24EA2B' -derivedDataPath /tmp/zoen-glass-baseline-dd CODE_SIGNING_ALLOWED=NO build`.
- FAIL reproduced baseline macOS: corresponding `-scheme ZoenMac -destination 'platform=macOS'` full build failed at `PushBridge.swift:58`: `.ephemeral` unavailable on macOS. The initial branch build reproduced the same error. The separately authorized iOS-only guard fixes it.
- PASS final iOS full build: `xcodebuild -project apple/Zoen.xcodeproj -scheme ZoeniOS -destination 'platform=iOS Simulator,id=E748ECE3-BD66-46CF-A20E-00BEFB24EA2B' -derivedDataPath build/DerivedData CODE_SIGNING_ALLOWED=NO build`.
- PASS final macOS full build: `xcodebuild -project apple/Zoen.xcodeproj -scheme ZoenMac -destination 'platform=macOS' -derivedDataPath /tmp/zoen-glass-mac-dd CODE_SIGNING_ALLOWED=NO build`.
- PASS baseline and branch targeted UI tests, each 3 tests/0 failures: `xcodebuild -project <baseline-or-branch>/apple/Zoen.xcodeproj -scheme ZoenUITests -destination 'platform=iOS Simulator,id=E748ECE3-BD66-46CF-A20E-00BEFB24EA2B' -derivedDataPath <separate-derived-data> -only-testing:ZoenUITests/InvestorShowcaseTests -only-testing:ZoenUITests/OnboardingKeyboardJourneyTests CODE_SIGNING_ALLOWED=NO test`. Includes investor light/dark captures and onboarding keyboard interaction. Branch UI tests preceded the PushBridge platform-only fix; final app builds include it.
- PASS `bash scripts/test-apple-message-mutations.sh`: failed edits preserve draft/error, retry commits latest draft, deletion closes after success.
- PASS `bash scripts/test-apple-money-input.sh`: 59 passed/0 failed.
- PASS `bash scripts/test-apple-page-display.sh`: 8 cases passed.
- PASS `bash scripts/test-apple-page-drafts.sh /tmp/zoen-glass-mac-dd/Build/Products/Debug`: 10 checkpoint journeys passed (real CryptoKit/filesystem/core).
- PASS `bash scripts/test-plan-page-layout.sh /tmp/zoen-glass-mac-dd/Build/Products/Debug`: 14 passed/0 failed.

## Native sizing regression verification

The corrected source was verified with full iOS and macOS builds using the final build commands above; both passed. Logs: `/tmp/zoen-glass-review-ios.log` and `/tmp/zoen-glass-review-mac.log`.

Regression tests now assert the onboarding Continue button is at least 56 points high and the actual `.wabiGlassProminent` revoke-toast Undo (`toast-undo`, not the plain approval-card or voice-editor Undo) is narrower than one third of the 402-point window. Undo also restores the revoked standing decision.

Fail-before checks used an isolated `git archive 46de5a0` at `/tmp/zoen-glass-regression`, with the new test files copied in, an XcodeGen project, and existing local generated RodaCore artifacts copied in. The working source was not reverted. With `DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer`, commands used:

```sh
xcodebuild -project /tmp/zoen-glass-regression/apple/Zoen.xcodeproj -scheme ZoenUITests -destination 'platform=iOS Simulator,id=E748ECE3-BD66-46CF-A20E-00BEFB24EA2B' -derivedDataPath /tmp/zoen-glass-regression-dd -resultBundlePath /tmp/zoen-glass-regression-ui.xcresult -only-testing:ZoenUITests/OnboardingKeyboardJourneyTests -only-testing:ZoenUITests/VoiceEditorMultiCutTests CODE_SIGNING_ALLOWED=NO test
xcodebuild -project /tmp/zoen-glass-regression/apple/Zoen.xcodeproj -scheme ZoenUITests -destination 'platform=iOS Simulator,id=E748ECE3-BD66-46CF-A20E-00BEFB24EA2B' -derivedDataPath /tmp/zoen-glass-regression-dd -resultBundlePath /tmp/zoen-glass-regression-toast.xcresult -only-testing:ZoenUITests/ApprovalsJourneyTests/testAlwaysDenyThenRevokeInPermissions CODE_SIGNING_ALLOWED=NO test
```

- EXPECTED FAIL before: onboarding height **34.3333 < 56**; log `/tmp/zoen-glass-regression-ui.log`. The companion voice-edit journey passed; its temporary width assertion targeted a plain button and was removed.
- EXPECTED FAIL before: glass Undo width **139 >= 134**; log `/tmp/zoen-glass-regression-toast.log`.
- PASS after: `xcodebuild -project apple/Zoen.xcodeproj -scheme ZoenUITests -destination 'platform=iOS Simulator,id=E748ECE3-BD66-46CF-A20E-00BEFB24EA2B' -derivedDataPath build/DerivedData -resultBundlePath /tmp/zoen-glass-review-ui.xcresult -only-testing:ZoenUITests/InvestorShowcaseTests -only-testing:ZoenUITests/OnboardingKeyboardJourneyTests -only-testing:ZoenUITests/VoiceEditorMultiCutTests CODE_SIGNING_ALLOWED=NO test`: **4 tests, 0 failures**; light/dark screenshot journeys, onboarding minimum height, keyboard interaction and voice-edit render.
- PASS after: corresponding test command with `-resultBundlePath /tmp/zoen-glass-review-toast.xcresult -only-testing:ZoenUITests/ApprovalsJourneyTests/testAlwaysDenyThenRevokeInPermissions`: **1 test, 0 failures**. Captured tree reports glass Undo **114.3 × 60 points**; Undo restores the decision.

Refreshed screenshots are unmodified attachments exported with `xcrun xcresulttool export attachments`. Onboarding/chats/group come from `/tmp/zoen-glass-review-ui.xcresult`; compact Undo comes from `/tmp/zoen-glass-review-toast.xcresult`.

The complete UI suite, physical-device behavior, macOS visible UI and independent Rust framework rebuild were not run. This evidence covers selected iOS screens/interactions and full Apple app compilation, not all application journeys.
