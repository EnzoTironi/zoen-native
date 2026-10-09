# Android verification

Verified on 2026-10-09 with JDK 17, SDK 36, NDK 27.1.12297006, and an ARM64 Android 36 emulator. The emulator's normal display was 1080 × 2400 at 420 dpi. A temporary 2200 × 1400 display at 240 dpi exercised the tablet layout; font scale 1.5 exercised larger text. After the checks the emulator returned to its phone display, font scale 1.0, enabled animations, English, and light mode.

## Automated checks

| Check | Local result |
|---|---|
| `cargo test -q -p roda-ffi` | 34 tests passed. |
| `:app:testDebugUnitTest` | Five planner tests passed: currency/cents, overflow, Portuguese, normal and small budgets. |
| `NativeJourneysTest` | Three real UI journeys passed: search opens the matching chat, approval is recorded by Rust, and a created plan plus completed task survive activity recreation. |
| `NativeCoreTest` | A real local identity was created using Android Keystore, then its signed plan, page, and file were recovered after closing and reopening the database. Every log verified. This isolated test does not connect to a relay. |
| `SecretVaultTest` | Keystore encryption, a new vault instance, tampered ciphertext rejection, and deletion passed. |
| `:app:lintDebug` | Passed with no errors. Remaining warnings include dependency updates, unused resources, pluralization suggestions, and Kotlin convenience APIs. |
| Debug and release builds | Both ARM64 and x86_64 built with the release Rust profile. The release app passed R8 and resource shrinking. |
| Native packaging | Every packaged Rust, JNA, and Compose graphics library has 16 KB ELF LOAD alignment. `zipalign -c -P 16 4` passed for the release APK. |
| Release startup | The minified release APK, signed locally with a development key only for installation, opened native onboarding. Passing the debug demo extra did not enable the demo. |
| Gradle configuration cache | A debug build stored the cache successfully, and an identical build reused it. |

The five instrumentation tests were run against the final debug APK using AndroidJUnitRunner. The Kotlin/Compose screens call the generated UniFFI bindings and packaged Rust library; these tests do not replace the repository or engine with mocks.

The local build commands were:

```bash
cargo test -q -p roda-ffi
cd android
./gradlew :app:assembleDebug :app:assembleDebugAndroidTest :app:assembleRelease \
  :app:lintDebug :app:testDebugUnitTest -PcoreProfile=release \
  -Pkotlin.compiler.execution.strategy=in-process --no-daemon --max-workers=2 \
  --no-configuration-cache
adb install -r app/build/outputs/apk/debug/app-debug.apk
adb install -r app/build/outputs/apk/androidTest/debug/app-debug-androidTest.apk
adb shell am instrument -w -r xyz.tironi.zoen.test/androidx.test.runner.AndroidJUnitRunner
```

The device run used `adb shell am instrument -w -r xyz.tironi.zoen.test/androidx.test.runner.AndroidJUnitRunner` after installing the app and test APKs. CI uses Gradle's connected test task on Android 35 x86_64. Building an x86_64 APK locally does not establish that it ran on an x86_64 device; that runtime check belongs to CI.

## Visual checks

Reviewed native onboarding and chats, plan task edits and version history, approval details and confirmation, MapTap touch selection and persisted scoring, pet feeding and its persisted state, and Donkey Dash's native controls. At font scale 1.5 the pet actions and game controls remained readable and reachable. The tablet used a navigation rail and separate chat list/detail panes.

Switching Android's per-app language from an open English demo item to pt-BR reopened the Portuguese engine and returned to valid navigation. The Portuguese demo contains translated source data, and dark mode follows Android's setting. Activity recreation and a fresh process launch recovered stored demo edits. The final app was also installed over a minified release smoke build without losing the separate demo data.

Screenshots and screen recordings are uploaded to the Android pull request with `gh pr create --attach`. They show demo peers; no messages, invitations, or payments were sent to other people. Generated APKs, recordings, and local logs stay in ignored build directories.

## Scope of this evidence

Live two-device relay sync, encrypted media transfer against the production relay, physical hardware, and the Android 9 minimum version were not exercised locally. The transport, signatures, MLS, and outbox come from the existing shared Rust engine.

Android currently uses the explicitly labeled local planner fallback. Background push delivery, PDF/video markup, the microphone editor, home-screen widgets, and arbitrary third-party HTML mini-app hosting remain outside this port. The bundled mini-apps have native Compose implementations. The [Android README](../../android/README.md#current-limits) describes these limits and how to build and run the app.
