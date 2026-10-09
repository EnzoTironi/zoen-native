# Zoen for Android

Zoen's Android app uses Kotlin, Jetpack Compose, Material 3, and the existing Rust engine through UniFFI. The engine owns messages, signed history, permissions, budgets, encryption, pages, files, and sync. Compose reads its projections. This directory contains no duplicate database or mock messaging service.

## Build and run

Requirements: JDK 17, stable Rust, Android SDK 36, build-tools 36.0.0, NDK 27.1.12297006, and `cargo-ndk`. Android 9 or newer can run the app. Builds support ARM64 phones and x86_64 emulators.

```bash
cargo install cargo-ndk --locked
rustup target add aarch64-linux-android x86_64-linux-android
export ANDROID_HOME=/path/to/android/sdk
# macOS, if Java 17 is not the default:
export JAVA_HOME="$(/usr/libexec/java_home -v 17)"

cd android
./gradlew :app:assembleDebug
./gradlew :app:testDebugUnitTest :app:lintDebug
./gradlew :app:connectedDebugAndroidTest
```

The Gradle `buildRodaCore` task compiles the Rust library and generates matching Kotlin bindings before Android compilation. Its inputs include Rust source, bundled mini-app resources, Cargo.lock, and the UniFFI configuration. Generated files live under `app/build/generated/roda`; do not edit or commit them.

For a faster build on an ARM64 emulator or phone:

```bash
./gradlew :app:assembleDebug -PandroidAbis=arm64-v8a
```

Run on a connected device from the repository root:

```bash
scripts/run-android.sh -PandroidAbis=arm64-v8a
```

Or open `android/` in Android Studio and run the `app` configuration. Rust and cargo-ndk must be on Android Studio's PATH. For a release build, run `./gradlew :app:assembleRelease -PcoreProfile=release`. Configure your own release signing before distributing the unsigned release APK. No signing keys are included.

## Accounts and demo

The default flow creates a real local identity and connects it to `https://relay.tryzoen.com`, the same default relay as the Apple app. Connection settings on the last onboarding step let a developer choose another relay. Debug builds allow HTTP for local development; release builds require HTTPS in onboarding.

The Android emulator reaches a relay on your Mac at `http://10.0.2.2:8787`. Use the repository's existing `scripts/dev-stack.sh` to run the backend.

The debug app has an explicit **Explore the demo** button. You can also start it with:

```bash
adb shell am start -n xyz.tironi.zoen/.MainActivity --ez demo true
```

Demo data uses a separate database, has a visible Demo label, and contains simulated peers. It does not connect to a relay. Release builds ignore the demo extra and do not show the demo entry point. **Your context > Use a real account** leaves the demo without copying its people or keys into a real account.

## Android behavior

| Area | Android implementation |
|---|---|
| Navigation | Material bottom navigation on compact windows, a rail from 600 dp, and chat list/detail panes from 840 dp. Navigation 3 saves each tab's stack and supports system and predictive Back. |
| Appearance | Zoen's moss green, original mascot drawn by Compose Canvas, Material surfaces, system typography, light/dark themes, and English/pt-BR resources. Decorative mascot motion follows Android's animator duration setting and stops when its screen is not started. |
| Onboarding | Seven steps, local plan preview, real per-space agent trust, Android notification permission, and an optional location preference. Location access is not requested without a consuming feature. |
| Conversations | Engine-backed timelines, delivery state, inline replies, threads, draft persistence, unread counts, pinned chats, and scrolling that keeps your place while reading. |
| Plans | Signed edits, task completion, costs, line addition/removal, Undo, full version history, and restoration as a new version. |
| Approvals and agents | Content-bound approval/denial, standing decisions, revocation, per-space autonomy, budgets, and read-only controls for agents owned by someone else. |
| Files and pages | Android document and photo pickers, system camera, bounded file import, Markdown page import/export, a block editor, images and text previews, versioned text edits, and Android sharing through FileProvider. |
| Mini-apps | Native pet care and Donkey Dash, polls, shared lists, cooking and ingredient scaling, MapTap with Natural Earth polygons and accessible coordinate sliders, hike voting, and countdowns. Every mutation passes through the engine's Grants. Irreversible actions ask through a Material dialog. |
| Search | Local full-text search across messages, items, apps, people, agents, and spaces. A message result opens its chat at the matching message. |
| Sync | Existing Rust WebSocket transport, outbox, signatures, MLS, and encrypted media. App callbacks refresh Compose state. Android HTTPS media uses WebPKI roots, matching the WebSocket transport, without requiring a JNI certificate verifier. |
| Keys | AES-256-GCM wrapped secrets in no-backup private storage, a non-exportable Android Keystore wrapping key, authenticated records, and atomic writes. Both cloud backup and device transfer exclude app data. |
| Sharing and links | Android share sheet, content URI grants, and `zoen://chat`, `zoen://app`, `zoen://item`, and `zoen://join` links. Invite links are previewed before joining. |

## Current limits

The Android planner is the labeled deterministic local fallback. Apple Foundation Models has no Android equivalent in this app yet. No remote AI API or API key is silently substituted.

The shared prototype's simulated external actions remain simulated. The app has no FCM push service or persistent Android background service; Android may suspend its connection when the app is backgrounded. Outgoing events remain in the Rust outbox. Notification permission does not imply background push delivery.

PDF/video markup, a microphone waveform editor, Android home-screen widgets, and a sandboxed host for arbitrary third-party HTML mini-apps are not implemented in this port. The bundled mini-apps use native Compose screens. File export lets other Android apps open formats that have no inline preview.

The existing licensing for `apple/Shared/Resources/land110.json` applies to its Android copy. It contains Natural Earth public-domain land polygons.

## Verification

`NativeJourneysTest` drives the real Compose UI and Rust library for plan creation, task completion, recreation, approvals, and search. `NativeCoreTest` creates a real local account with Keystore keys and reopens its signed plan, Markdown page, and attachment. `SecretVaultTest` exercises Android Keystore encryption, fresh-instance loading, ciphertext tampering, and deletion. `LocalPlannerTest` checks English/Portuguese output, currency parsing, overflow, and budgets.

The Android CI workflow builds both ABIs, runs unit tests and lint, tests on an x86_64 Android emulator, and uploads the debug APK and test reports. See the [device verification record](../docs/dev/android.md) for local results and review evidence.
