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
failures stay pending. No native declarations or root consumer have been registered yet.

`miniapp-widget.ios.tsx` is a real SDK58 pure layout registered as `ZoenMiniapp`, currently
unreferenced by the app and unregistered in CNG. It uses native Text timers, progress,
rows, rendering-environment adaptation and `privacySensitive` content. It has no local
mutation buttons. The text layout does not claim parity with animated baseline artwork
or the baseline's extra hike/ticket tiles.

Six focused tests cover snapshot binding, unsafe content, bounded projection, default
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
