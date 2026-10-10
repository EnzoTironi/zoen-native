# Native Android product review

The first Android pass covered shared-core workflows but changed visible product structure. Passing behavioral tests did not establish visual parity. This correction compares the current SwiftUI source with the actual Android app, rather than treating the older poster screenshots as the current specification.

## Corrections

| Product behavior | Original source | Android mismatch | Correction |
|---|---|---|---|
| App identity and startup | `apple/Shared/Assets.xcassets/AppIcon.appiconset/icon-ios-1024.png` | Android replaced the actual app artwork with a different mascot icon and green startup background. | The original PNG is preserved byte-for-byte in the adaptive icon, with a native monochrome layer and light/dark startup themes. |
| Main navigation | `apple/iOS/ZoeniOSApp.swift` | Files was inside a secondary menu; Zoen floated over the conversation list. | Chats, Spaces, central Zoen action, Files and Activity keep the original order, using a native Material navigation bar and an adaptive rail. Files has its own saved back stack. |
| Home hierarchy | `apple/Shared/Features/ConversationsView.swift` | Permanent search, repeated headings, subtitle and filters pushed conversations down. | Mascot and lowercase wordmark, live app strip, then compact conversation rows. Local search appears on demand; global search remains in the central action sheet. |
| Contact portraits | `apple/Shared/Features/ConversationsView.swift` | Agents appeared as tinted square tiles in contact positions. | Circular contact portraits in the chat list, chat header and messages. Agent-specific profile art remains available. |
| Palette | `apple/Shared/DesignSystem/Theme.swift` | Green-tinted backgrounds and bubbles replaced the original neutral surfaces. | Shared light/dark neutral palette, moss-green actions, black/light own-message contrast and grey incoming messages. Quotes and voice controls retain readable contrast. |
| Message structure | `apple/Shared/Features/SpaceView.swift` | Avatar and sender sat above every direct message; plan cards were nested inside text bubbles. | Avatars beside messages, group sender names where needed, separate artifact cards below text and tighter message spacing. Reply, thread, delivery and version actions remain connected to the same core. |
| Onboarding | `apple/Shared/Features/OnboardingFlow.swift` | Large left-aligned serif headline and a different mascot expression. Ordinal-based poses put props on the wrong steps. | Centered bold headings, paper background and darker textured mascot. Named poses match the original wave, phone, map, run, juggling, walking and celebration steps. |
| Enlarged text | Android runtime inspection | Main-navigation labels clipped at font scale 2. | Additional navigation height and compact label typography at enlarged text sizes. Narrow layouts use single-line ellipsis while retaining full accessibility names and native touch targets. |

## Verification of the corrected app

App source `161c487` includes the corrected palette and original launcher artwork. The matching native APKs pass **53/53 cases, zero failures or skips**, at 720 × 1600/density 280 (61.86 seconds) and 320 × 640/density 160 (53.44 seconds). Their uncut recordings last 61.84 and 53.33 seconds with matching encoded dimensions.

The normal ARM64/x86_64 debug, instrumentation and minified release builds, lint and all 77 JVM tests pass. All six packaged native libraries and the unsigned/development-signed release APK satisfy 16 KB alignment checks. R8 retains the exact ML Kit registrar and constructor. The development-signed, non-debuggable release performs a true cold launch in 239 ms, with matching installed APK hash and an inspected onboarding screen. This startup measurement is one emulator sample, not a performance comparison.

The first full device run of the visual correction passed 51 cases and failed two journeys that assumed the previous navigation. The global-search journey now opens the action sheet's Search destination; image markup opens the primary Files tab. Their real search, signed image version, recreation and deadline assertions remain. The failed log and recording are retained separately from the successful runs.

Current-head CI acceptance requires 53 passing cases on Android 15 and 51 passes with only the two documented HTML-provider skips on Android 9. [PR 41 checks](https://github.com/EnzoTironi/zoen-native/pull/41/checks) publish those results. Native/visual evidence is separate from successful physical model execution and the shared public relay rollout.

## Evidence and scope

The primary Files journey uses the same test APK against the previous and corrected app. The previous app fails because the primary Files destination is absent. The corrected app opens Files, preserves folder search through recreation, returns to Files with one Back action and switches to Chats and back. The screen-inspector accessibility conflict was retained as a separate failed harness run, then resolved by closing the inspector before instrumentation.

The screenshots and uncut device recordings are attached to PR #41 using `gh --attach`. Earlier attachments and failed-run evidence are preserved. Build artifacts, installed APK hashes, native results and individual image inspection records are retained under the ignored `build/` directory.

Successful physical Gemini Nano generation and model-backed speech transcription still require supported hardware and downloaded models. The public relay remains a separate rollout dependency. These checks do not prove that every visual detail is perfect; they establish the corrected structures and exercised behavior.
