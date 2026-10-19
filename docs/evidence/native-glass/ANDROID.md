# Android native Material3 verification

## Captures

All six API 35 ARM64 emulator PNGs were opened and visually inspected. They render the real demo repository with the production onboarding, conversations and chat composables. No physical-phone installation was performed.

| Screen | Before | After |
| --- | --- | --- |
| Onboarding | [Before](android-before-onboarding.png) | [After](android-after-onboarding.png) |
| Chats | [Before](android-before-chats.png) | [After](android-after-chats.png) |
| Group chat | [Before](android-before-group.png) | [After](android-after-group.png) |

Before: isolated `git archive origin/main` at `71bc1ebb857e7756ba659f64da1b962bc289fcb6`, with `NativeGlassScreensTest` copied in solely for capture. Its build reused current generated Rust bindings/JNI libraries and excluded `buildClientCore` and `verifyClientCore`; `git diff origin/main -- Cargo.lock Cargo.toml crates resources` was empty. This is UI baseline evidence, not an independent baseline Rust build. The shared checkout was never switched or reverted.

After: native Material3 wrappers on the task branch. `NativeGlassScreensTest` uses a 1500ms wait after Compose idle so native animated artwork can draw, then committed-window PixelCopy capture. Before captures used the same screen test before that extra artwork wait; animations and demo timestamps therefore differ. These are visual comparisons, not pixel-diff assertions. Group-chat page card now has a flat native surface rather than the custom glass rim. Chats keep their existing illustrated widgets.

## Passed checks

Temporary Amazon Corretto JDK 17 installed under `/tmp/zoen-jdk17` without changing global Java. SDK emulator 37.2.12 and Google APIs ARM64 images for API 28 and 35 installed using sdkmanager. Both isolated AVDs booted on this Apple Silicon Mac.

- `./gradlew :app:testDebugUnitTest :app:lintDebug :app:assembleDebug :app:compileDebugAndroidTestKotlin`: **passed**, BUILD SUCCESSFUL in 7m24s. Both native ABIs built and source/generated ABI verification passed. Existing compiler warnings remain unrelated to this change.
- API35 `./gradlew :app:connectedDebugAndroidTest -Pandroid.testInstrumentationRunnerArguments.class=xyz.tironi.zoen.ui.NativeMaterialComponentsTest`: **passed**, 1 test, zero skipped/failed. Checks enabled clicks, disabled button/card behavior, and nonclickable card display.
- API35 `./gradlew :app:connectedDebugAndroidTest -Pandroid.testInstrumentationRunnerArguments.class=xyz.tironi.zoen.NativeGlassScreensTest`: **passed**, 3 tests, zero skipped/failed; final capture run completed in 33s.
- API28 `./gradlew :app:connectedDebugAndroidTest -Pandroid.testInstrumentationRunnerArguments.class=xyz.tironi.zoen.ui.NativeMaterialComponentsTest,xyz.tironi.zoen.NativeGlassScreensTest`: **passed**, 4 tests, zero skipped/failed, BUILD SUCCESSFUL in 1m39s. This run preceded the screenshot-only 1500ms artwork-wait addition.
- API35 baseline `./gradlew :app:connectedDebugAndroidTest -x :app:buildClientCore -x :app:verifyClientCore -Pandroid.testInstrumentationRunnerArguments.class=xyz.tironi.zoen.NativeGlassScreensTest`: **passed**, 3 tests, zero skipped/failed, BUILD SUCCESSFUL in 1m42s.
- `git diff --check`: **passed**.

Commands selected emulator serials explicitly with `ANDROID_SERIAL`; the attached Samsung SM_S916B was not used. Full connected-device suite, dark mode, other sizes and real-account/network journeys were not run. Screen capture tests render each production composable independently; they do not prove navigation between screens.
