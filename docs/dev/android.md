# Android verification

The port uses Kotlin/Compose and the real Rust JNI library. The completion gate is the current Apple prototype's functional coverage, native Android behavior, combined device journeys, release shrinking and CI. This record distinguishes completed checks from checks still running.

## Current checks — 2026-10-09

| Check | Evidence |
|---|---|
| Shared core | 42 `roda-ffi` tests pass; formatting and strict Clippy pass. Coverage includes user catalog installation, typed media versions, immutable historical content and three shutdown/reopening regressions. |
| JVM | 73 tests pass in 11 suites, with zero failures, errors or skips. Coverage includes structured generation, routing/context, Unicode search, rich page editing, onboarding drafts/routes, MCP boundaries, snapshots, media edits and globe geometry. |
| Combined build | The normal both-ABI debug/test/release build, R8, lint and all 73 JVM tests pass with the `8739825` source, including the ML Kit constructor fix and committed-window capture. The build uses the release Rust profile and matching generated bindings; native build tasks are retained. |
| Isolated relay | Independent Android identities exchanged encrypted messages, a chunked attachment, a new typed version and an encrypted photo background. Closing/reopening and offline outbox recovery passed. The local relay uses the existing Postgres/FoundationDB backend. |
| Native core/system integration | All five `NativeParityTest` cases pass: private account data erasure, batch Markdown/image content URIs and thumbnails, rich page/history reopening, all seven real RemoteViews templates with private content hidden, and notification preview/read/mute/visible-chat behavior. Widget checks now exercise 160 × 180 dp at font scales 1 and 2, full Portuguese action labels, 48 dp buttons and compact-to-expanded reapplication. |
| Focused media | Android codecs and image/PDF rendering were exercised; actual image ink produced a signed v2, preserved original v1 bytes, and survived activity recreation. Microphone review, cut, AAC send and playback ran against the actual core. |
| Agents and permissions | Context/routing and fallback tests pass. Encrypted profile and browser takeover checks passed. Native globe interaction, complete signed audit verification and scoped standing/device grant persistence/revocation passed. |
| HTML MCP | Real system finger input updates HTML List and bundled React Hike, their live DOM and signed Rust versions. Offline map pan/zoom/fit, detail rendering and voting pass. Cross-app resource access and foreign origin bridge access are denied; irreversible calls require native confirmation. |
| Complete local device run | Matching API35 runs at `827f312` and `8739825` each pass all 48 cases, with zero failures and skips. They include real launcher pin/open/Edit/Feed/cleanup, native page restoration as signed v3, microphone cut/send/playback after rotation, retained thread ownership, two-account relay recovery and persisted chat appearance. The preceding 700ba13 run passed 47 and exposed the appearance readiness race; its failure is retained in the audit trail. |
| Current CI | The complete 8739825 API35 matrix passes all 48 cases; full Rust CI passes. API28 passes 45 cases, skips the two provider-gated MCP cases and fails only the launcher pin acceptance fixture: Android9 exposes the native button label in uppercase and the preview as an image. The fixture now matches the actual localized launcher Button and the older launcher-owned raster preview, while keeping exact Item/owner binding, live title, signed Feed and cleanup requirements. The final rerun remains pending. The 827f312 jobs failed during Docker Hub image retrieval; both CI workflows now use the verified Docker Official Image on ECR Public. Current results and artifacts are linked from PR 41. |
| Release/package checks | The rebuilt release passes R8, and its exact ML Kit registrar constructor remains in the mapping and is absent from the removal report. All six packaged ARM64/x86_64 native libraries have at least 16 KB LOAD alignment and the APK passes 16 KB zip alignment. Unsigned release SHA256 is `d30943cea82247589d0ea1747d64600f623402a90c91225d51a7c7b3756bb3e5` (55,524,318 bytes). Both engine libraries contain Hike bundle `8307eacf13a2f4e7d5154bb3bb163d9f764fd76c738ffa1c355e80e50159d292`. A copy signed with the development key cold-starts as a non-debuggable app, loads native JNA, renders onboarding and has no ML Kit registration error. |

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

Hike success images use Android's frame-commit callback and PixelCopy to capture the actual submitted app Window within the original ten-second draw deadline. The test also retains a full-display image and render timing/result JSON. The committed capture at 8739825 visibly shows “Voted · 1” and matches the trusted-input trace and signed version 2.

## Visual evidence

[PR 41](https://github.com/EnzoTironi/zoen-native/pull/41) contains running-app screenshots and recordings uploaded with `gh --attach`. Evidence covers image ink and signed v2, native globe, actual Hike voting, page restoration, microphone editing and real launcher pin/configure/feed actions alongside the original phone/tablet, large-text, Portuguese/dark-mode, plan/version and mini-app captures. Captions identify earlier source versions and edited video timing. Images are visually reviewed before publication; invalid captures are excluded.

Original visual checks covered a 1080 × 2400 phone at 420 dpi, a 2200 × 1400 tablet at 240 dpi, font scale 1.5, per-app English/pt-BR switching and Android light/dark themes. Final source changes require the new integrated review. Generated APKs, videos, screenshots and logs remain in ignored build directories; source records stay in Git.

## Platform conditions

- Gemini Nano generation requires a supported physical device and a downloaded ML Kit model. Availability/download/error handling and the labeled deterministic fallback are implemented. Emulator fallback results do not demonstrate Nano generation.
- On-device speech transcription requires Android's recognizer and an installed language model. Recording, editing, saving and playback work independently; the emulator has no usable speech model.
- Background delivery uses a user-enabled remote-messaging foreground service with a visible Stop control and the existing relay. Android force-stop and power policy apply. No FCM backend or production relay result is implied by isolated local relay tests.
- Home widget pinning, opening, native Edit/configuration, signed Feed refresh and removal of only the test widget pass on the actual launcher. Existing launcher widgets and account ownership are preserved.
- Apple prototype placeholders—calls, passkey recovery, simulated external actions and debug browser guest—remain identified placeholders on Android. The same source has seven actual catalog apps; fake store listings are not counted as functional apps.

See the [feature parity record](android-parity.md), [append-only decisions](android-parity-decisions.tsv) and [Android README](../../android/README.md).
