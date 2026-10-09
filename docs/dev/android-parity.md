# Complete Android port

The completion gate is functional coverage of the current Apple app, with Android equivalents for system APIs. A capability is complete only when it is wired into the app and its real behavior has been exercised. Model availability and system permissions are explicit platform conditions, as they are on Apple. Passing builds alone is not feature verification.

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
- [ ] Verify the whole app, Android 9 compatibility, release shrinking, and CI. Upload new images and videos to PR 41 with `gh --attach`.

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

Complete matching API 35 runs at `827f312` and `8739825` each pass all 48 native cases without failures or skips. The normal both-ABI debug/test/release build, R8, lint, all 73 JVM tests, signed release cold launch and committed-frame visual captures pass. The current Android 9/15 and full Rust CI results are tracked in the [verification record](android.md) and [PR 41](https://github.com/EnzoTironi/zoen-native/pull/41).

The append-only decisions are in [android-parity-decisions.tsv](android-parity-decisions.tsv).
