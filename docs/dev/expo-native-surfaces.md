# Widgets, Live Activities and App Intents

Source audit: 2026-10-17. Current acceptance priority is iPhone; Web and Android follow later. The signed ARM64
iOS simulator Release has compiled the widget extension, Live Activities and App
Intents metadata. Signed v7 accepts the redacted Portuguese Widget gallery preview. Selected content,
Siri and Lock Screen journeys below remain pending.
Installed packages and successful compilation do not establish those journeys.

## Apple baseline

`apple/Shared/Widgets/WidgetSnapshot.swift` validates seven content templates: stat,
progress, countdown, list, caption, ticket and photo. `SnapshotCard.swift` renders them
inside the app. `ChatTiles.swift` adds local presentation tiles and pinned strips.
`AppModel.swift` stores Home ordering/unpinning and chat tile ordering in UserDefaults.
Conversation pin/archive/mute preferences separately use the Rust Personal Space.

There is no WidgetKit extension target in `apple/project.yml`, or an ActivityKit,
AppIntent or AppShortcuts declaration in the baseline Swift sources. Comments describe
future widget-extension reuse. The in-app pinned-card journeys therefore do not prove
a launcher widget, Live Activity or Siri integration.

The portable `roda-apps::snapshot` supplies each built-in miniapp's canonical compact
content. Native `ItemDetail.app.snapshot_json` already exposes it. It must remain the
source of displayed values; an extension must not recreate pet, poll, schedule or
approval business rules.

## Verified SDK58 surfaces

`expo-widgets ~58.0.20` supports a pure `'widget'` layout using Expo UI SwiftUI views,
snapshot/timeline updates, configurable widget parameters, and Live Activity factories.
That layout runs in an isolated extension runtime with no hooks, asynchronous work or
access to the application engine. All required data enters through bounded props.
Its button callback can update local props while the app is closed; the application
interaction listener only runs while the app process is alive. Neither establishes a
signed Zoen mutation. [Expo Widgets SDK58](https://docs.expo.dev/versions/v58.0.0/sdk/widgets/)

`expo-app-intents ~0.5.9` is alpha. Real AppIntent, AppEntity, EntityQuery and shortcut
types must be declared in Swift compiled into the app target using Expo inline modules.
JavaScript cannot register those types or phrases dynamically. The invocation queue is
persistent and at least once; dispatch does not await a JavaScript result. On-screen
entity associations additionally require Xcode 27, and the relevant OS availability.
[Expo AppIntents SDK58](https://docs.expo.dev/versions/v58.0.0/sdk/app-intents/)

Live Activities fit ongoing events with a defined end, generally within eight hours.
Private content should remain in the application; Lock Screen information is visible
to nearby people. A distant trip countdown belongs in a widget rather than an activity
kept alive for days. [Apple ActivityKit](https://developer.apple.com/documentation/activitykit/displaying-live-data-with-live-activities),
[Apple Live Activities guidance](https://developer.apple.com/design/human-interface-guidelines/live-activities)

## Shared implementation checkpoint

`apps/zoen/src/native-surfaces/snapshot.ts` checks snapshot item ID, app ID and deep link
against the actual Item, with bounded JSON/text/arrays and known local art. It produces
an immutable item/version/account/device projection. The default projection omits names,
values, rows, bars, artwork and domain actions. Explicit content selection exposes only
the compact content, never `view_json`, page bodies, account keys or database paths.

`catalog.ts` publishes at most 50 explicitly selected, currently accessible item IDs and
titles, with account/device/version metadata. The default selection is empty and
`hideInSpotlight` remains true. Callers must replace catalogs with an empty catalog on
sign-out or access loss. These navigation intents are not donation-enabled, so there
are no registered donations to remove. Calling the SDK's donation deletion API for
their names fails even on a fresh install.

`intents.ts` accepts only `zoen.openItem` and `zoen.openChat`. It validates the invocation
age, exact parameter shape, current account/device, item version and current Space
access, checking the owner again after asynchronous reads. Unknown names and mutations
are rejected. This is repeatable navigation; the native queue may acknowledge only an
actual `opened` result. Invalid/retired actions need an explicit discard policy; transient
failures stay pending. App-owned Swift declarations now expose these read-only actions with local device authentication and opening dialogs. A single ready-session root consumer handles pending navigation; unknown actions and transient/foreign-owner failures remain pending, while known invalid/expired or stale-version requests are retired. Native metadata compilation and actual queue journeys remain pending.

`miniapp-widget.ios.tsx` is a real SDK58 pure layout registered as `ZoenMiniapp`, registered by the source-controlled CNG plugin. Signed v7 compiles the extension and its actual Portuguese redacted gallery preview
renders. Selected-content projection and refresh acceptance remain pending. It uses native Text timers, progress,
rows, rendering-environment adaptation and `privacySensitive` content. It has no local
mutation buttons. The text layout does not claim parity with animated baseline artwork
or the baseline's extra hike/ticket tiles.

Nine focused tests cover snapshot binding, unsafe content, bounded projection, default
redaction, explicit catalog selection, stale/foreign/unauthorized navigation and account
switches during asynchronous reads. They exercise shared code, not an installed extension.

## Sky appearance and conversation chrome

The iPhone appearance setting is **Your sky / Seu céu**. Six curated phases replace
the light/dark/system control: deep night, dawn, morning, day, sunset and night.
Automatic follows the device's local civil clock, using anchors at 00:00, 05:00,
08:00, 13:00, 18:00 and 21:00. It does not request location or use weather or solar
services. Existing light/dark/system preferences resolve to day/night/automatic;
the existing serialized preference writes remain authoritative.

The shared backdrop uses bundled photorealistic sky textures over quiet Fabric gradients, with a bounded radial haze. The photos are cropped to a small patch of sky, enlarged 1.8× and blurred natively by 20 points; illustrated stars and celestial objects are absent.
Native Stack headers are transparent, with blur, shadows and scroll-edge fills disabled. The per-scene sky continues behind navigation and scroll content; native Back, glass title/actions and system search remain UIKit controls. Static Search and Forward fields reserve the actual native header height, while scroll-first screens use automatic insets without adding that height twice. Content surfaces retain their own contrast. The selector uses native Expo UI rows, photographic previews and
an explicit dismissal action. The selected phase also sets UIKit appearance, so
native sheets, buttons and glass agree with the sky.

The clock samples only while Automatic is active and the app is in the foreground.
Normal samples can fade on the native driver after checking the outgoing and
incoming color envelope. Manual selections, clock/timezone jumps, foreground
resumption, scheme changes and reduced motion update immediately. The backdrop
has no perpetual animation and cannot receive touches or accessibility focus.
Eight deterministic tests cover all 1,440 local minutes, gradient/haze/photo contrast,
alpha-composited surfaces, paired controls, midnight, preference migration and
clock/transition guards. Photographic overlays use 28% intensity in the four light
presets and 20% in the two night presets; automatic twilight can reduce intensity
to preserve contrast. All semantic inks are checked against arbitrary image pixels.
Native tab controllers paint their own scene, so each tab also hosts its backdrop
inside that scene; relying only on a root underlay left a white canvas in v10. These checks complement installed iPhone visual QA.

Conversation headers preserve UIKit Back and group avatar, name and disclosure in
a native glass title. Native phone/video buttons remain disabled because the core
has no calling operation; the adjacent magnifier opens real search scoped to that
conversation. The composer uses native circular glass buttons and a growing field:
empty input offers voice recording, while text offers Send. Existing attachment
review, recording lifecycle, encrypted send and draft retention remain in place.
Delivery marks use vector symbols and canonical core acknowledgements: pending
clock, one sent check, two delivered checks, and blue checks only after Read.
Local-only and failed messages have distinct device/warning symbols.

The composer overlays the full timeline in main chat and threads. Its wrapper is
transparent, with no full-width blur or painted panel; the existing native glass
input and controls retain their own material. A shared flex viewport shrinks with
the keyboard avoider's padding, while the absolute composer measures its height
for list bottom padding. The latest message can clear the controls, and older
history can scroll behind them. The composer is above the fixed rail in stacking
order. Own bubbles are white/black text in light skies and black/white text in dark
skies; received bubbles use neutral #E9E9EB/#262628. Actual signed v20 light-sky
capture confirms own white bubbles, gray received messages and history behind the
transparent footer. A later signed v22 capture confirms black own bubbles with
white text on the night sky, hidden cards while typing and approximately 8.7 points
between the composer and keyboard. Interactive restoration, multiline/thread and
cold acceptance remain separate from that bounded screenshot.

Keyboard motion now follows actual `useKeyboardHandler` move/interactive/end
frames shared by the input and card rail. The keyboard avoider remains the sole
owner of keyboard overlap. The input reduces home-indicator padding to 8 points
above the keyboard, and a measured native animated list footer reserves its real
height without setting React state on each animation frame. Cards stay mounted,
preserving horizontal position; their matching list spacer collapses with them.
Hidden cards lose touch/accessibility targets, reduced motion removes the extra
slide/fade, and notices remain visible.

Conversation search reuses the actual message input rather than replacing the
header. The header magnifier switches that same field into search; the microphone
slot becomes older/newer arrows, and a completed empty query offers Send to Ask
Zoen. Loading, failures and stale results never admit AI. Canonical paginated hits
reveal exact message IDs, including thread replies temporarily rendered in the
same conversation. Closing restores the normal field, saved text and audio owner.
Only an opaque ticket enters navigation; the question remains in memory. The
earlier v23 header-search observation is historical and does not validate this
new composer layout.

Ask Zoen uses real Apple Foundation Models when the current iPhone supports and
enables the model and language. It is a bounded, read-only temporary conversation,
not a new durable Agent, Space or chat message. Each turn reads a current bounded
canonical text excerpt and rechecks membership and device/session ownership after
generation. Closing or erasing cancels and drains the lease. Unsupported hardware,
disabled/downloading models and unsupported languages produce truthful capability
states; there is no remote or canned-answer fallback. A root-reviewed signed v23
capture during human use shows a Portuguese model reply grounded in the limited
text excerpt. That observation is not a controlled known-query or physical-device
acceptance run.

The normal composer stays mounted while search hides it, so entering and closing
search preserves a staged audio draft as well as text. Entering search stops a live
recording into its retained draft; asynchronous recording startup checks suspension
after permission and preparation, preventing a hidden recording from starting later.

Conversation miniapps stay fixed below the measured native header, with 6-point
rail padding. Their wrapper is a transparent absolute overlay; messages scroll
behind the cards and remain visible in their gaps, without a fade, mask or painted
background. The list reserves the measured rail height only at its content start,
and message jumps offset below that overlay. In signed v19, two actual screenshots
during human scrolling show timeline96%→27% while all three card frames stay at
normalized y0.140/height0.174. No automation swipe was performed during that user
interaction. Earlier v17 independently confirmed fixed frames across one bounded
automated swipe. Final keyboard/jump, isolated pin/unpin and cold acceptance remain
pending.

## Voice editing checkpoint

The immutable original-frame edit model and session coordinator pass 37 focused
tests covering trim/cuts, restoration, transcript rebasing, undo/redo, obsolete
exports, cancellation and cleanup ownership. The actual macOS AVFoundation harness
encodes and decodes exactly66,150 retained frames at44.1kHz (1,500ms): synthetic
440/880Hz regions survive, the removed1,200Hz region does not, and contained15ms
fades preserve frame counts and original hashes. The real Swift module compiled in
signed iOS v17 and updated v21/v22. Current source checks pass TypeScript, Oxlint,
Oxfmt and212/212 tests, including16 owner-lifecycle,6 scoped-search and9 temporary
conversation lifecycle regressions in addition to the37 edit/session/suggestion tests.

The 37 pure checks comprise 15 edit, 14 session and 8 suggestion tests. Native
regressions also clamp extreme/invalid Speech times before numeric conversion and
bound analysis to a duration-aware 60–900 seconds. Text fragments may be phrases;
controls retain their exact ranges and never invent inner word times. Optional
hesitation cuts require complete retained fragments; pause proposals use intact
original-adjacent timing gaps over 1.2 seconds and retain 300 milliseconds. Such
gaps may contain other sounds. Nothing is removed automatically, proposals are
revalidated and each accepted batch is one undo step.

Signed v22 imports the known synthetic 5.54-second Portuguese audio through the
ordinary file picker and mounts the real editor. The attachment menu now uses the
direct SwiftUI sheet's fully-finished dismissal callback before presenting another
picker. Earlier v21 stopped before the picker; no causal UIKit warning was captured.
Current source defaults to a compact audio strip above the normal input. Dragging
up or using its accessible expand control opens the detailed editor; collapsing
preserves the same draft/edits, while explicit discard releases its owned files.
Recording and preview retain the same send/cleanup fences. No transcription or
removal starts automatically. Installed gestures, retained preview, on-device
transcription, edited encrypted send and independent-peer playback remain pending;
these checkpoints do not establish full parity with the previous Swift editor.

## In-app Home and miniapp cards

The source now defines four native tabs: Home (`/`), Chats (`/chats`), Files and You.
Activity remains a root Stack screen at `/activity`; the Home menu always opens it,
and a review tile appears when canonical requests are Pending or Stale. Chats aligns
its heading and native glass search/compose capsule in one row, with the same
18-point top and 20-point horizontal content padding as Home. The whole heading and
miniapp rail live inside the automatic-inset list; the separate Stack header is
hidden. There is no expanded search field. Search opens the actual `/search` route,
and the cards retain separate spacing below the heading.

`HomeProvider` is mounted under the canonical client. It reads actual `items` with
miniapp content and filters them against currently known conversation membership.
Personal or unknown cached spaces remain hidden until the core supplies an explicit
authorization projection for them.
Home renders explicit favorites followed by remaining miniapps; Chats renders the
accessible miniapps, and an individual conversation rail filters by its own Space.
Compact cards render the canonical snapshot using bundled Zoen artwork and open
`/miniapp/[id]`. Invalid snapshots have a bounded title/headline fallback. These
in-app cards do not require external widget-content consent and do not donate titles,
start Live Activities or publish anything to the system launcher.

Photographic cards preserve a clear image hero and place every caption/value in a
lower68–82% black gradient region. A contrast regression checks primary/secondary
inks against arbitrary image pixels, including pure white. Photo press feedback
uses scale rather than alpha to preserve text contrast. Installed visual acceptance
on actual photographic cards remains pending.

Favorites persist only Item IDs in a versioned device-local preference namespace
bound to identity and device. There are no implicit default pins. Restore/read/write
results are fenced across account changes; failed pin writes retain the prior pins,
and device erasure awaits their cleanup. Demo pins are volatile. Pin order is the
selection order; drag ordering and a separate per-conversation item-pin preference
are not implemented. Conversation pin/archive/mute commands retain their own meaning.

Home uses native SwiftUI add and per-card pin menus. Add opens the existing miniapp
creation flow, new conversation, Activity or search. On iOS 26+, the native tab bottom
accessory provides a stateless Search Zoen launcher while Home is selected. It stays
mounted, and the SDK58 documented `unstable_nativeProps.ios.bottomAccessoryHidden`
host option hides the entire accessory on other routes. Merely conditionally removing
its children left an empty rounded native plate on Chats in installed v12. The
installed React Native Screens host maps this flag to UIKit `setBottomAccessory:nil`,
rather than hiding only the React content. Actual signed v13/v15 screenshots confirm
the empty plate absent on Chats and Files. Search opens the search screen. Loading, failure and empty
states expose real state rather than synthetic content. Twelve focused controller tests
cover owner changes, persistence failures, stale reads, access loss and cleanup.
Actual v12 screenshots show canonical Recipe/Countdown/Pet/Poll cards in Home and
Chats, and Recipe opened its native detail. Signed v15 also confirms the Files
heading and actions below the notch. Isolated menu/pin and final cold-relaunch
acceptance remain pending during concurrent user exploration.

## Integration and command map

| Journey                                              | Existing authoritative operations                                                         | Remaining integration                                                                                                                                                                          |
| ---------------------------------------------------- | ----------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Read a miniapp widget                                | `items`, `item`, `item_at`, `app.snapshot_json`                                           | CNG widget kind/App Group, per-instance item selection, active-account publisher, freshness and cleanup on access loss.                                                                        |
| In-app Home favorites and miniapp rails | Canonical `items`, `app.snapshot_json`; identity/device-scoped local Item-ID preferences | Home pin/unpin UI and membership-filtered Home/Chats/conversation cards are implemented in source. Installed interaction/relaunch QA, drag ordering and independent conversation item pinning remain pending. No conversation preference command is repurposed. |
| Open an item/chat through Siri                       | `item` plus current conversation membership                                               | Swift entities/query and read-only intents, app-target setup, local device authentication, single queue consumer, catalog update/clear lifecycle and actual Shortcuts/Siri QA.                 |
| Perform a miniapp tool from a widget/intent          | `app_specs`, `app_call_tool`; Rust grant/confirmation checks                              | Durable invocation-ID deduplication atomic with the mutation and outcome, then native result/confirmation semantics. No props-only Feed/Nap/vote simulation.                                   |
| Send a message or decide a request through an intent | `send_message`, `decide_request`                                                          | Persistent idempotency for invocation replay; approval/account/access checks and honest queued/acknowledged/refused results. These actions are not accepted by this router.                    |
| Live countdown or current operation                  | Rust-derived item state and target time                                                   | Explicit start/stop UX, system availability, bounded duration/staleDate, activity ownership/recovery and immediate end on sign-out or access loss; actual Dynamic Island/Lock Screen checks.   |
| Remote Live Activity updates                         | Existing `register_push` is a content-free message-wake registration                      | Separate token purpose/ownership/provider contract for push-to-start and per-activity tokens. Do not treat a notification token as an activity token or send private E2E content through APNs. |

Before the next CNG integration, register only actual home/Lock Screen widget kinds with
supported families. A Live Activity uses `createLiveActivity` and must not receive an
empty `widgets[]` entry. The App Group stores compact projections and selected public
assets only; the existing primary SQLite database and Keychain remain application-owned.

Runtime acceptance must cover first-run widget gallery, selection of two distinct items,
updates after a real signed change and relaunch, light/dark/tinted/Lock Screen rendering,
content redaction, item removal and account change cleanup. App Intents additionally need
warm/cold delivery, rename/delete/access loss and queue-removal failure/replay. A future
mutating intent needs a crash between commit and dequeue to demonstrate durable deduplication.

Useful primary references: [Expo SwiftUI Text timers](https://docs.expo.dev/versions/v58.0.0/sdk/ui/swift-ui/text/),
[Expo SwiftUI modifiers](https://docs.expo.dev/versions/v58.0.0/sdk/ui/swift-ui/modifiers/),
[Apple intent authentication](https://developer.apple.com/documentation/appintents/intentauthenticationpolicy).

## Current CNG integration

`app.config.ts` applies `plugins/with-native-surfaces.cjs`, composing the installed
SDK58 `expo-widgets` and `expo-app-intents` plugins. The extension uses bundle
`xyz.tironi.zoen.widgets` and isolated App Group `group.xyz.tironi.zoen.widgets`.
The primary SQLite database and Keychain service are not shared with the extension.
Push-to-start and frequent remote activity updates are disabled.

`NativeSurfacesProvider` publishes from the live canonical driver and refreshes on
store changes and foreground. Device preferences store only selected item IDs,
owner/device binding, explicit content choices and active system activity IDs.
`MiniappSurfaceControls` offers one device-wide chosen widget item, explicit content
reveal, opt-in Siri/Shortcuts titles, and start/stop for countdowns ending within eight
hours. Multiple widget instances currently use the same selected item; independent
per-instance selection is not implemented. A future redacted timeline entry is
scheduled at 15 minutes; WidgetKit owns scheduling, so this is not a guaranteed erasure
or refresh deadline. No private-content freshness guarantee is claimed while the
app is terminated. Live Activities use canonical ends and system stale dates.

The root mounts the provider beneath `ClientProvider`, the single
`NativeIntentConsumer` beneath ready navigation, and controls in the miniapp screen.
These mounts must be included before runtime acceptance. Surface deep links route
through `/surface`, reread item/access/version and then replace with the actual item.

Apply CNG and generate the Rust simulator framework:

```sh
cd apps/zoen
npx expo prebuild --platform ios
env -u SDKROOT DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer \
  node ../../scripts/build-expo-core.mjs ios --simulator
```

Build the generated workspace for the simulator UDID selected through Argent:

```sh
env -u SDKROOT DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer xcodebuild \
  -workspace ios/Zoen.xcworkspace -configuration Debug -scheme Zoen \
  -destination "id=$ZOEN_SIMULATOR_UDID" ARCHS=arm64 ONLY_ACTIVE_ARCH=YES \
  COMPILER_INDEX_STORE_ENABLE=NO
```

Keep Xcode's default ad-hoc simulator signing enabled. `CODE_SIGNING_ALLOWED=NO`
omits the simulated application entitlements and can cause Keychain access to fail.
The Rust simulator framework currently contains ARM64, so a generic destination
that also links Intel is not supported by this local artifact.

Install the resulting `Zoen.app` through `npx expo run:ios --binary <app-path>
--no-bundler`, which preserves the existing app data. Explicitly open the current
Metro dev-client URL afterwards if Expo selects a different server. Do not uninstall
the existing app or reset its account to make these features appear.

Native App Intents metadata extraction, extension entitlements,
App Group provisioning for device distribution, gallery rendering, Dynamic Island,
Siri warm/cold delivery and accessibility require runtime verification. No paid build
or developer portal changes were performed by this source integration.

## Local device erasure

The You screen's **Sair e apagar dados** action captures the current identity and
device before displaying its destructive confirmation. The store rejects an old
confirmation after an account change and immediately fences old UI results,
timelines, live status and new requests while the operation runs.

Before deleting canonical data, it awaits the registered projection cleanup: widget
content, entity catalogs, activity termination, queued App Intents,
presentation preferences and device notifications. Notification cleanup drains local
notification and badge writes already in flight. If any cleanup fails, the operation
stays retryable and does not delete the account's keys or database. A process restart
at that preflight stage may restore the unchanged account; erasure has not committed.

The iOS bridge then records a small nonsecret intent scoped to the exact device,
drains active core requests and history reception, stops sync and calls the existing
Rust erasure operation. It additionally verifies removal of the account's complete
Keychain namespace and clears the six owned cache namespaces: `ZoenMedia`,
`ImagePicker`, `ImageManipulator`, `DocumentPicker`, `ExpoAudio` and `zoen-exports`.
The preflight also drains active SDK file writers and clears image memory/disk caches
through the public Expo Image APIs. Private image views disable caching. Picker
originals are disposed after profile processing, including failure paths. Cleanup
preserves photo-library originals, Documents, assets, fonts and update caches; re-audit
the owned namespace list when upgrading the SDK packages. Missing files are distinguished
from failed metadata inspection: inaccessible cache or journal parents fail closed instead
of being treated as empty. The intent is removed last; a cold
open resumes an interrupted destructive stage before allowing another account to
open. Neither the account on other devices nor their histories is deleted.

The browser driver uses a strict, atomic IndexedDB transaction to clear the complete
protected vault while retaining its exclusive account lock. It aborts provider setup,
stops messaging and pairing, and releases staged files and photos. A failed storage
transaction remains fenced and retryable; a successful clear removes device keys,
session and root authority together. The action is not offered on Android until its
native lifecycle has separate implementation and acceptance.

Store, browser-vault and pure Foundation tests cover ordering, stale ownership,
delayed writes, inaccessible files and retry behavior. Signed iOS checkpoint `f9544c3a`
passed two disposable-device confirmations, cold onboarding without Metro and exact
primary-history preservation; it also accepted profile save/cold reopen and canceled
silent recording. Actual Web checkpoint `03d787f` passed erasure and peer preservation
in CI. Signed v3 and v4 installed acceptance subsequently passed with the
cache-inspection correction: confirmed local erasure, cold onboarding without Metro
and exact primary-history preservation. v3 repeated cancel as well. A sanitized
runtime witness found that the official SDK launch envelope reaches the native
intent with a slash root. The minimal validator correction accepts only that root
variant while retaining the exact SDK query checks and never following its inner
address. Signed v4 opened directly into Chats through the official CLI and through
a Metro-off linked cold restart; it then passed erasure and Metro-off empty cold
onboarding. Its 244-input source SHA256 is
`f53a592b3dd40bfe6c9d52c99efa8b7cc8f6c2dc40a66b0bcd353087d13d67b0`,
with JS bundle SHA256
`9b23c6324db740ca431ecdd00c34676bf8a7e95ae51109e1f2fcef89e9288553`.
Temporary diagnostic instrumentation was removed before the final bundle.
Existing personal accounts must never be reset to produce evidence.

## Language ownership and native metadata

The custom Expo entry resolves ordered device preferences once and initializes
the typed i18n-js catalogs before Router imports screens. Native bridge open and
browser-worker initialization require that canonical `en` or `pt-BR` tag. Rust
scopes the immutable engine language around synchronous calls and every async
poll, restoring ambient thread state. Language is not persisted in the account.
The current iOS/Web implementation reopens the process/page when preferences
change; coordinated Android foreground changes remain deferred.

Locale changes separators and grammar. Existing plan/grant cents retain their
documented BRL meaning; miniapp scheduling uses its explicit currency and scale,
and the signed owner policy remains USD/6. JavaScript formats/parses exact integer
strings with BigInt, preserving signed-i64 limits without floating-point rounding.
Rust still validates financial policy, including new-negative-cost refusal.

CNG configuration declares EN/PT support and generates translated permission
descriptions, Localizable strings and a separate AppShortcuts table. The plugin
links the native tables into the app/widget targets; widget worklets receive
bounded localized projection props and import no JS translation singleton.
App-owned native errors use English source keys with Portuguese translations.
Generated-resource inspection, Swift parsing, Foundation fixtures and CocoaPods
installation passed. Signed v6 accepts app-owned English and Portuguese main journeys; permission,
gallery and Shortcuts runtime acceptance remains pending.

## iPhone-first installed acceptance

Signed v6 on the disposable iPhone18 Pro/iOS27 simulator accepts ordinary encrypted
linking/history, actual chat rendering and profile photo propagation in both
directions against a fresh current-core primary. Native stock-photo save produced
profile version 2 and an exact matching primary photo hash; the primary update
produced version 3 and visibly replaced the iPhone avatar, name and bio. This
resolves the earlier observed photo-null/version-1 convergence failure.

With Metro stopped, a cold chat deep link opened the real conversation and Back
returned to native tabs. Metro was restored afterward. Per-app PT → EN → PT kept
the same device identity and original literal encrypted history. Native Google
confirmation opened; cancellation returned to onboarding without provisioning an
account. Real provider authentication is a separate remaining setup/acceptance step.

The v5 splash-screen failure was traced through native public logs to missing
`Intl.PluralRules` in Hermes. Registering supported CLDR rules from the existing
OSS i18n-js/make-plural integration fixed startup in v6. A regression removes that
API before executing real English and Portuguese plural messages, without a global
polyfill. iOS money formatting also requires a number-only Intl path: the installed
Hermes tag rejects BigInt there and does not expose `formatToParts` on Apple.
Amount arithmetic remains exact BigInt; only small separator/sign/currency probes
reach the native formatter. Its installed v8 financial journey now passes, as recorded below.

V6 source/artifact manifest SHA256:
`c5ed9e0d2e94775b5a1a5f19a1ca9c971320877554447563788dfd9221914b94`.
JS bundle SHA256:
`8a0b2e32737db608fc665f79ec74916a0107335fb380356dc1cfac617eca5888`.
A 27-second native main-journey clip was recorded. Signed v7 subsequently accepted
the native SwiftUI Appearance Picker; v8 accepted exact money and native UIKit theme
synchronization, as recorded below.

Signed v7 also discovers Zoen in the native Home Screen Widget gallery. Its actual
Portuguese preview displays the privacy description, lock/Zoen branding and the
choose-a-miniapp placeholder without chat, profile or other private content. No
Widget was added and no content preference changed during that check. Selected
content updates, timeline refresh, Live Activities and Siri remain separate journeys.

Signed v8 recovers the exact approved Plan that v7 could not open. The original
certified QA Agent proposal and ordinary one-time owner approval are unchanged.
The corrected confirmed fold binds their exact authenticated opening and receipt,
preserving Agent authorship, history and domain guards. A normal GUI edit saves
`9223372036854775807` BRL cents; two independent canonical peers return that exact
line/total string. Every displayed digit is visible in the iPhone UI. A one-cent
overflow is refused without another version; Metro-off cold reopen retains the
maximum. Normal GUI restoration produces `12345` cents/version 3 on both peers.

The same signed build makes the native glass tab bar dark with white icons when
Dark is selected. A Metro-off cold launch retains the selection; System removes
the override. This uses React Native 0.88's supported app-window appearance API,
not a custom replacement tab bar. Dynamic Type's native-menu fallback still needs
its own runtime acceptance.

V8 source/artifact manifest SHA256:
`e185609cc67bb58599bc30a371e1ac9f5eed7cee2f33b76143b3732e374d4a4e`.
JS bundle SHA256:
`a40bb211968cf234cbef22eede165d66b1f3e73f532094974cd01379565193dc`.
Full native Rust 367/367, focused Plan 71/71, scoped Clippy, Rust formatting and
app TypeScript/Oxlint/Oxfmt plus 123/123 tests pass. These journeys do not establish
real Apple/Google entry, physical-device distribution or complete product parity.

The final signed v9 changes only two rendered Agent captions from v8: it removes
the legacy hardcoded claim that every Agent executes on this device with Apple
Intelligence. A separately owner-authorized Agent principal does not establish
that location or model. Both You and Agent detail now omit that unsupported claim;
owner administration remains available. A Metro-off cold launch accepts both
surfaces and retains the profile photo, native Dark preference and normal
`12345`-cent Plan. The money boundary/overflow matrix was not repeated for this
caption-only delta.

Final v9 source/artifact manifest SHA256:
`e8a6f4180e4aacab89a3f2a3f0541db4800e3a5153606813e9a515d901fc5a9f`.
JS bundle SHA256:
`30242242ae2fddb70e5993c4652e53f34029caa98708bc5f22fb713f074f50da`.
The simulator app contains 136,264,462 bytes. Its source checks and 123 app tests
pass. The earlier functional v8 acceptance and unchanged native Rust artifact
remain identified separately above.
