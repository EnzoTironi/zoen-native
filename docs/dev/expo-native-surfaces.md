# Widgets, Live Activities and App Intents

Source audit: 2026-10-17. Active migration scope is iOS and Web. Native extension
registration, native compilation and device journeys below remain pending. Installed
packages alone do not make these features available in a running app.

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
`hideInSpotlight` remains true. Callers must replace catalogs with an empty catalog and
remove applicable donations on sign-out or access loss.

`intents.ts` accepts only `zoen.openItem` and `zoen.openChat`. It validates the invocation
age, exact parameter shape, current account/device, item version and current Space
access, checking the owner again after asynchronous reads. Unknown names and mutations
are rejected. This is repeatable navigation; the native queue may acknowledge only an
actual `opened` result. Invalid/retired actions need an explicit discard policy; transient
failures stay pending. App-owned Swift declarations now expose these read-only actions with local device authentication and opening dialogs. A single ready-session root consumer handles pending navigation; unknown actions and transient/foreign-owner failures remain pending, while known invalid/expired or stale-version requests are retired. Native metadata compilation and actual queue journeys remain pending.

`miniapp-widget.ios.tsx` is a real SDK58 pure layout registered as `ZoenMiniapp`, registered by the source-controlled CNG plugin. Native extension compilation and gallery verification remain pending. It uses native Text timers, progress,
rows, rendering-environment adaptation and `privacySensitive` content. It has no local
mutation buttons. The text layout does not claim parity with animated baseline artwork
or the baseline's extra hike/ticket tiles.

Nine focused tests cover snapshot binding, unsafe content, bounded projection, default
redaction, explicit catalog selection, stale/foreign/unauthorized navigation and account
switches during asynchronous reads. They exercise shared code, not an installed extension.

## Integration and command map

| Journey                                              | Existing authoritative operations                                                         | Remaining integration                                                                                                                                                                          |
| ---------------------------------------------------- | ----------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Read a miniapp widget                                | `items`, `item`, `item_at`, `app.snapshot_json`                                           | CNG widget kind/App Group, per-instance item selection, active-account publisher, freshness and cleanup on access loss.                                                                        |
| Home/chat miniapp pinning and ordering               | Baseline device-local UserDefaults; `set_chat_prefs` only covers conversation preferences | Account-scoped presentation preference policy and UI; a conversation pin command must not be repurposed as an item pin. No new preference command is implemented here.                         |
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
node ../../scripts/build-expo-core.mjs ios --simulator
```

Build the generated workspace for the simulator UDID selected through Argent:

```sh
DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer xcodebuild \
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
