# Android verification

The port uses Kotlin/Compose and the real Rust JNI library. The completion gate is the current Apple prototype's functional coverage, native Android behavior, combined device journeys, release shrinking and CI. This record distinguishes completed checks from checks still running.

## Current checks — 2026-10-09

| Check | Evidence |
|---|---|
| Shared core | 39 `roda-ffi` tests pass; strict Clippy passes. User catalog installation, typed media versions and immutable historical content have engine tests. |
| JVM | 73 tests pass in 11 suites, with zero failures, errors or skips. Coverage includes structured generation, routing/context, Unicode search, rich page editing, onboarding drafts/routes, MCP boundaries, snapshots, media edits and globe geometry. |
| Combined build | The normal both-ABI debug/test/release build, R8, lint and all 73 JVM tests pass at `f47812f`. The next matching build includes the focused shutdown, history actionability, Files scroll target and Hike detail-frame fixes. |
| Isolated relay | Independent Android identities exchanged encrypted messages, a chunked attachment, a new typed version and an encrypted photo background. Closing/reopening and offline outbox recovery passed. The local relay uses the existing Postgres/FoundationDB backend. |
| Native core/system integration | All five `NativeParityTest` cases passed: private account data erasure, batch Markdown/image content URIs and thumbnails, rich page/history reopening, all seven real RemoteViews templates with private content hidden, and notification preview/read/mute/visible-chat behavior. |
| Focused media | Android codecs and image/PDF rendering were exercised; actual image ink produced a signed v2, preserved original v1 bytes, and survived activity recreation. Microphone review, cut, AAC send and playback ran against the actual core. |
| Agents and permissions | Context/routing and fallback tests pass. Encrypted profile and browser takeover checks passed. Native globe interaction, complete signed audit verification and scoped standing/device grant persistence/revocation passed. |
| HTML MCP | Real HTML List touch updated signed Rust state and live DOM. Cross-app resource access and foreign origin bridge access were denied; irreversible calls required native confirmation. The full bundled React/MapLibre Hike journey is in the final combined run. |
| Final device/CI run | The complete `f47812f` matrix ran all 47 cases without a JNI abort. API 28 passed 42, failed three and skipped two unsupported HTML-provider cases; API 35 passed 43 and failed four. Current full Rust CI passed. Lifecycle, voice capture/edit/playback and retained-thread review tests pass. The remaining history, relay reopen, Files scroll and Hike vote failures have focused source fixes; their combined verification is pending. Failed runs do not count as passing evidence. |
| Release/package checks | The `f47812f` normal release build passes R8. All six packaged ARM64/x86_64 native libraries have at least 16 KB LOAD alignment and the APK passes 16 KB zip alignment. Release SHA256 is `05583e1d060a70a42ae43288f06b2845aefd9a822b90eed2be3b867abe54cc9c`; both engine libraries contain the current Hike bundle. The shutdown and UI changes require rebuilding and checking the new artifact. |

Final integrated UI journeys exercise all eight onboarding areas, real background connection and message deep links, separate threads and quotes, pinned plans, stationary Home voice hold/release, rich page editing/autosave/old-version restoration, chat appearance, native media, browser takeover, globe, audit/grants and the HTML host. All six stationary-hold/accessibility gesture cases and all six native media/codec cases pass on the completed matrix. New regressions check consumable cold/warm links, notification handback and retained voice reviews across different thread roots. Hike keeps its real touch, rendered-frame and signed-vote gates; Android tonal surfaces replace expensive backdrop-filter readbacks before the first paint.

## Reproduce

Use JDK 17, SDK 36 and NDK 27.1.12297006. Both ARM64 and x86_64 native libraries are built from the same source as generated bindings.

```bash
cargo test -q -p roda-ffi
cargo clippy -p roda-ffi -- -D warnings
cd android
./gradlew :app:assembleDebug :app:assembleDebugAndroidTest :app:assembleRelease \
  :app:testDebugUnitTest :app:lintDebug -PcoreProfile=release
# In another terminal, from the repository root:
# scripts/dev-stack.sh
../scripts/test-android-device.sh \
  -Pandroid.testInstrumentationRunnerArguments.zoenRelay=http://10.0.2.2:8787
```

The relay argument is required for the two-identity and onboarding/background cases. They create and clean up their own accounts; without the argument they report a skip. They refuse to replace an existing real account. Demo peers alone do not establish network delivery.

Captures use AGP's additional instrumentation output directory, which AGP copies before uninstalling the test app. The device runner validates PNG signatures, copies the collected MCP, globe and media files, and preserves Gradle's test exit status. The earlier post-test collector returned text errors after package removal; those files are excluded from visual evidence. App and instrumentation APKs are separate packages and must be installed separately when installing them manually.

## Visual evidence

[PR 41](https://github.com/EnzoTironi/zoen-native/pull/41) contains running-app screenshots and recordings uploaded with `gh --attach`. New native image ink, persisted signed image v2, native globe reveal and markup-controls video are attached alongside the original phone/tablet, large-text, Portuguese/dark-mode, plan/version and mini-app captures. Further captures will accompany the final combined run. Images are visually reviewed before publication; a black voice capture was excluded.

Original visual checks covered a 1080 × 2400 phone at 420 dpi, a 2200 × 1400 tablet at 240 dpi, font scale 1.5, per-app English/pt-BR switching and Android light/dark themes. Final source changes require the new integrated review. Generated APKs, videos, screenshots and logs remain in ignored build directories; source records stay in Git.

## Platform conditions

- Gemini Nano generation requires a supported physical device and a downloaded ML Kit model. Availability/download/error handling and the labeled deterministic fallback are implemented. Emulator fallback results do not demonstrate Nano generation.
- On-device speech transcription requires Android's recognizer and an installed language model. Recording, editing, saving and playback work independently; the emulator has no usable speech model.
- Background delivery uses a user-enabled remote-messaging foreground service with a visible Stop control and the existing relay. Android force-stop and power policy apply. No FCM backend or production relay result is implied by isolated local relay tests.
- Home widget pinning/configuration and real launcher actions remain part of the final manual gate; RemoteViews inflation alone is insufficient.
- Apple prototype placeholders—calls, passkey recovery, simulated external actions and debug browser guest—remain identified placeholders on Android. The same source has seven actual catalog apps; fake store listings are not counted as functional apps.

See the [feature parity record](android-parity.md), [append-only decisions](android-parity-decisions.tsv) and [Android README](../../android/README.md).
