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

The default flow creates a real local identity and connects it to `https://relay.tryzoen.com`, the same default relay as the Apple app. Connection settings in the profile step let a developer choose another relay. Debug builds allow HTTP for local development; release builds require HTTPS in onboarding.

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
| Onboarding | Eight configurable steps, encrypted profile photos, all eight life areas, a rotation-safe starter preview, real per-space agent trust, and optional native notification/location permissions. Acquisition links select their configured landing. |
| Conversations | Engine-backed timelines, delivery/presence/typing state, selectable mentions, quote navigation, swipe-to-reply/thread, separate threads, inline approvals, pinned plans, draft persistence, unread counts, and scrolling that keeps your place while reading. |
| Plans | Signed edits, task completion, costs, line addition/removal, Undo, full version history, and restoration as a new version. |
| Approvals and agents | Content-bound approval/denial, standing decisions, revocation, per-space autonomy, budgets, and read-only controls for agents owned by someone else. |
| Files and pages | Folder browsing and batch imports through Android document/photo pickers and camera; rich text and all ten page block types; durable drafts, autosave, undo/redo, Markdown import/export; read-only historical previews and signed restoration; native PDF/image ink and actual audio/video trims saved as typed versions. FileProvider grants support Android sharing. |
| Voice notes | Hold, slide-to-cancel, lock and review; AAC recording, waveform/transcript editing, word/cut/filler/pause controls, undo/redo, playback, and signed reply/thread attachments. Offline speech availability and language downloads are explicit. |
| Mini-apps | Native pet care, Donkey Dash, shared lists, cooking/scaling/timers, an offline draggable MapTap globe, hike voting/filters/comparison, and countdowns. Store installation preserves trust. The HTML MCP host isolates each document's origin, validates JSON-RPC, and uses explicit native capabilities and shared engine grants. Irreversible actions require a Material confirmation. |
| Widgets | Seven snapshot templates, editable tile order/hiding/pins, and Android home widgets with per-card pinning, configuration, current signed actions, account binding, private locked content, and best-effort countdown refresh. |
| Search and activity | Native search scopes, grouped/highlighted results, recent searches, suggested people and real catalog discovery. Messages open at the matching entry. Activity separates approvals, mentions, and tasks. Audit history exposes and verifies every signed event; permission screens show and revoke exact standing/device grants. |
| Chat appearance | Local or shared styles and encrypted photos, original wallpapers, crop/pan/zoom, dimming, blur and dark adjustment. |
| Sync | Existing Rust WebSocket transport, outbox, signatures, MLS, and encrypted media. App callbacks refresh Compose state. Android HTTPS media uses WebPKI roots, matching the WebSocket transport, without requiring a JNI certificate verifier. |
| Keys | AES-256-GCM wrapped secrets in no-backup private storage, a non-exportable Android Keystore wrapping key, authenticated records, and atomic writes. Both cloud backup and device transfer exclude app data. |
| Sharing and links | Android share sheet, content URI grants, and `zoen://chat`, `zoen://app`, `zoen://item`, and `zoen://join` links. Invite links are previewed before joining. |

## Platform conditions

HTML mini-apps require an Android System WebView provider supporting AndroidX WebKit's `WEB_MESSAGE_LISTENER` and `MULTI_PROFILE` features. Zoen checks the installed provider, not the Android API level, and shows an update screen when either feature is unavailable. It never substitutes a shared WebView profile or an unrestricted JavaScript bridge. All seven native mini-apps remain available on Android 9 and later without the HTML host. See [AndroidX WebKit feature detection](https://developer.android.com/reference/androidx/webkit/WebViewFeature).

The Hike HTML bundle draws its local route map with SVG when map-tile access is denied. Pan, pinch zoom, zoom buttons and fit-to-route work without network access or a WebGL renderer. MapLibre initializes only after the MCP handshake confirms access to the declared tile host; unavailable online maps fall back to the same local route map.

On supported physical devices with initialized AICore, a downloaded model and a locked bootloader, the planner uses Gemini Nano through ML Kit's on-device Prompt API. Availability, download progress and generation failures are visible. Structured results are validated before signed writes. Other devices use a clearly labeled deterministic local fallback. Physical-device generation was not exercised; emulator fallback tests and successful release registration do not demonstrate model generation. See [ML Kit setup requirements](https://developers.google.com/ml-kit/genai/prompt/android/get-started). No remote AI API or key is silently substituted.

Offline transcription requires API 33+, Android's on-device recognizer and an installed language model. Word, filler and pause editing additionally require API 34+ and recognizer-provided word timestamps. Supported languages can request a native model download. Model-backed transcription was not exercised because the emulator has no usable model; recording, waveform cuts and playback work independently.

Background messages use an opt-in foreground remote-messaging service with a visible connection notification and Stop control. Incoming messages and requests have private notifications and native deep links; active chats and muted people suppress alerts. This uses the existing relay. Android force-stop or restrictive power policies can suspend it, and the Rust outbox persists outgoing work. There is no FCM backend.

Apple prototype placeholders—calls, passkey recovery, simulated external actions and the simulated browser guest—remain labeled placeholders. Native browser owner takeover uses the actual sealed frame/input core; its sample guest is debug-demo only.

Home widget pinning, opening and signed Feed actions are exercised on the real launcher. Native Edit/reconfiguration requires API 31+ and launcher support; older Android versions retain pin/open/actions. See [Android widget configuration](https://developer.android.com/develop/ui/views/appwidgets/configuration).

The Natural Earth globe data retains the source app's public-domain licensing.

## Verification

The device suites drive Compose, the actual Rust JNI library and Android system APIs: plans, approvals, search, rich pages and historical restoration, imports, markup/media, voice gestures, appearances, on-device globe interaction, browser takeover, origin-isolated HTML MCP, signed audit and scoped grants, widgets and notifications. Core/Keystore tests also cover persistence and tampering. JVM tests cover structured generation, routing, page edits, search, snapshot schemas, origins, globe geometry and EN/PT behavior.

For encrypted two-account and onboarding/background journeys, run an isolated local relay and pass `-Pandroid.testInstrumentationRunnerArguments.zoenRelay=http://10.0.2.2:8787` to `connectedDebugAndroidTest`. These tests create and clean up their own identities and never replace an existing real account. Without that argument they report a skip rather than pretend that demo peers test the network.

CI builds both ABIs, runs JVM tests and lint, and exercises x86_64 Android 9 and Android 15 with an isolated real relay. APKs and reports are uploaded. The [parity record](../docs/dev/android-parity.md) records coverage and verification status; the [device verification record](../docs/dev/android.md) includes visual review evidence.

The provider UI test runs on every device. Secure HTML journeys report a skip only when the installed provider lacks a required isolation feature; they otherwise require actual rendered HTML, Android touch events, and signed Rust state changes. A provider skip does not skip the native mini-app or other Android journeys.
