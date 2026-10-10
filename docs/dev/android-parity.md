# Android parity and verification

The completion gate includes the visible product structure and functional coverage of the current Apple app, with Android equivalents for system APIs. A capability is complete only when it is wired into the app and its real behavior has been exercised. Model availability and system permissions are explicit platform conditions, as they are on Apple. Passing builds alone does not establish feature or visual parity. The [product review](android-product-review.md) records corrections against the current SwiftUI source.

## Work sequence

- [x] Read the workflow principles and record the initial gaps.
- [x] Frame: compare the current Apple features, Android screens, shared engine APIs, and tests.
- [x] Design: isolate media, mini-app hosting, and agents in separate worktrees. Keep widgets, Android background connection, chat appearance, and integration in the primary checkout.
- [x] Run the loop: implement and verify each functional unit on the real app, with model availability conditions recorded.
- [x] Media: recording, transcription availability/error handling and word edits, audio edit/render, playback, PDF/image markup, video trim. Model-backed transcription remains untested and requires API 33+ and an installed on-device language model; timed word/filler/pause editing requires API 34+ recognizer timestamps.
- [x] Agents: availability-aware on-device generation, contextual routing, encrypted browser takeover, complete profiles. Physical Nano generation remains untested and requires supported hardware, initialized AICore, a downloaded model and a locked bootloader.
- [x] Mini-apps: store/install, grants, secure HTML MCP host, snapshot templates, tile editing.
- [x] Android: actual launcher widgets, background messaging notifications, shared chat appearance.
- [x] End-to-end: two local real accounts, encrypted chat and attachments, offline outbox, restart.
- [x] Keep the audit trail as each unit is verified.
- [x] Verify the complete local app, both ABIs, release shrinking, cold launch and fresh images/videos with `gh --attach`.
- [x] Configure full Rust and Android 9/15 CI with real relay journeys; current-head results are published on [PR 41 checks](https://github.com/EnzoTironi/zoen-native/pull/41/checks).
- [ ] Release the default real-account connection after the documented shared public relay upgrade. The currently deployed protocol-2 service cannot serve the protocol-4 core.

## Baseline

The initial PR had 34 Rust core tests, five Kotlin tests, and five device tests passing. Android 35 x86_64 CI and Android 36 ARM64 local journeys passed. This covered plans, approvals, search, persistence, and the secret vault, and left these missing units:

| Apple source | Initial Android gap | Required Android result |
|---|---|---|
| `Agent/AgentPlanner.swift`, `AppChooser.swift`, `HikePlanner.swift` | Deterministic generation only; every free-text request created a plan | Availability-aware on-device generation, context, routing, replies, strict plan validation, timeout/fallback |
| `Agent/AgentBrowser.swift` | No browser owner takeover | Same sealed frame/input core, native owner controls, explicit handback |
| `Features/VoiceNotes.swift`, `VoiceEditor.swift`, `Navigation/PlusVoiceCapture.swift` | No recording or editor | Android mic gesture, review, waveform/word edits, preview, rendered attachment, playback |
| `Features/Files/FileScreen.swift`, `FileSupport.swift` | Text/image preview only | Native PDF/image markup, audio/video viewing and trimming, signed versions |
| `Apps/McpAppHost.swift`, `McpNative.swift` | No arbitrary HTML app host | MCP JSON-RPC WebView host with origin isolation, engine grants, native capability prompts |
| `Features/StoreView.swift`, `MiniAppFlip.swift` | No complete store, install, or detail controls | Catalog/install, permissions and details, native/HTML selection |
| `Widgets/WidgetSnapshot.swift`, `SnapshotCard.swift`, `DesignSystem/EditableTileStrip.swift` | Simplified app tiles | Validated snapshot templates, hide/pin/reorder, Android home widget configuration |
| `Features/ChatBackground.swift` | No shared appearance editing | Core-backed styles and photos, crop/zoom/dim/blur, local/shared choice |
| `Features/ProfileSheet.swift`, `Sync/AccountSection.swift` | Partial profile editing | Encrypted photos, handles, block controls, shared spaces, agent drawing selection |
| `Sync/SyncModel.swift` | Real connection not exercised end to end | Two identities exchange encrypted messages/media, resume persisted state and offline outbox |

Existing native navigation, onboarding, plans/versions/Undo, pages, search, approvals, trust/budgets, chats/replies/threads, groups/invites, Keystore, language/theme, and bundled mini-apps also remain in the verification pass. Apple placeholders such as calls, passkey account recovery, and simulated external actions remain clearly identified in both apps; they do not count as implemented capabilities on either platform.

Android notifications use the existing relay through a user-controlled foreground messaging connection. This adds Android background behavior without requiring a new push backend or cloud credentials.

Source `1753725` passes all 52 native cases with zero failures or skips on both the 320 × 640 and 720 × 1600 viewports. Separate regressions fail against prior implementations and pass with screen-owned subscriptions, cancellation-safe speech audio and restored historical reading position. Both viewport comparisons use the exact same test APK for each baseline and treatment. The new regression forces an unloaded timeline after restoration: the prior app loses its reading position, while the corrected app retains historical/latest positions and follows new own messages. The normal both-ABI build, R8, lint, 77 JVM tests, 57 FFI tests, signed non-debuggable release cold launch and fresh visual evidence pass. Full Rust CI at `e4ae67a` passes all 269 tests and strict Clippy; Android follow-ups do not change Rust or relay source. Current-head Android acceptance requires 53 passes/no skips on Android 15 and 51 passes/two expected HTML-provider skips on Android 9. [PR 41 checks](https://github.com/EnzoTironi/zoen-native/pull/41/checks) publish those results. The [verification record](android.md) distinguishes source coverage, prior failures, hardware conditions and the separate [public relay release dependency](android-relay-release.md).

The append-only decisions are in [android-parity-decisions.tsv](android-parity-decisions.tsv).
