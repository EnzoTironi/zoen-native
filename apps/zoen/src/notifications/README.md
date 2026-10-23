# Notification and link boundary

`NotificationProvider` runs inside `ClientProvider`. It reads OS permission without
prompting on foreground; only the explicit Settings action prompts. Native APNs
and Android FCM tokens use the existing authenticated Rust `register_push`
operation. APNs accepts variable-length hex and normalizes its case. FCM preserves
case and matches the relay's ASCII alphanumeric/colon/underscore/hyphen grammar,
up to 4096 bytes. Empty tokens are reserved for explicit removal. The JavaScript
coordinator keeps tokens only in memory; native provider storage remains SDK-owned.
The app never obtains an Expo Push Service token. Token rotations and
removals cannot overtake earlier writes, and a slow token query cannot replace a
newer native rotation. Account changes invalidate callbacks.

The current Rust operation is online-only and returns `Reply::Done`. Therefore
this client reports `acknowledged` only after that reply, `waiting` for a transport
failure, and never invents durable `queued` state or verified provider delivery.
Retries wait 10/20/40/80/160 seconds and then stop. A future durable registration
DTO must come from Rust/relay, including explicit provider availability. Closing
the app does not unregister its token; denied OS permission sends an empty token.

Remote registration is disabled unless the build explicitly supplies
`extra.zoenPush = { enabled: true, environment: 'sandbox' | 'production', topic }`.
The environment must match the signed entitlement; it is never inferred from
`__DEV__`. The CNG notifications plugin, valid signing and provider credentials
are separate requirements. No token or key is printed or returned by this UI.

## iOS configuration and native wake

The same explicit `extra.zoenPush` opt-in configures the SDK58 notifications
plugin with `enableRemoteNotifications` and
`enableBackgroundRemoteNotifications`. The topic must equal the iOS bundle ID.
Sandbox maps to the `development` entitlement; production maps to `production`.
A conflicting existing entitlement is refused. iOS configuration does not depend
on Android FCM configuration. Public Info.plist flags independently gate the
native subscriber; disabling the opt-in clears those flags on the next CNG build.

The dynamic app config also accepts this complete public build-time tuple:

```text
ZOEN_IOS_PUSH_ENABLED=true
ZOEN_IOS_PUSH_ENVIRONMENT=sandbox
ZOEN_IOS_PUSH_TOPIC=xyz.tironi.zoen
```

Use `production` for the production environment. An explicit tuple overrides the
iOS fields in `extra.zoenPush`; the topic must still match the bundle and any
existing signed-environment pin must agree. Partial tuples, whitespace/coerced
values and invalid environments are refused. With all three variables absent,
the existing app configuration is unchanged. `ZOEN_IOS_PUSH_ENABLED=false` alone
explicitly disables it; supplying an environment/topic alongside `false` is an
error. These inputs contain no token, key or provisioning material and do not
themselves register with APNs. CNG and a new native binary are required.

An enabled running JavaScript client with already-granted notification permission
can call the SDK's device-token API on foreground, which invokes native APNs
registration even on Simulator. A simulator wake injected locally is separate
from that registration and from provider delivery. Do not assume that enabling
the native configuration makes a foreground QA launch network-free.

`ZoenPushDelegate` receives the content-free OS callback without starting a
TaskManager/headless JavaScript runtime. A process transition gate borrows the
existing module's `ZoenSession`, or opens one temporary session only after the
previous owner has closed. A cold wake requires an existing account database,
existing vault keys and a registered account. It never creates an identity,
resumes provider setup or imports an owner's keys. An unreadable/locked keychain
after boot is a refusal; AfterFirstUnlockThisDeviceOnly remains unchanged.

The latest foreground module keeps its lease after an ordinary Core close, so
its existing erasure recovery can still finish. Replacement retires that lease:
old closes become no-ops, old opens/destructive completions are refused, and
transfer waits for accepted recovery work. Same-session close/closeForce/erase
reach the existing actor's coalesced closeTask instead of queuing force or erase
behind a graceful seal they need to release. No durable custody state is added.

The callback has an eight-second whole-work ceiling, shortened by OS expiry.
Duplicates receive no new engine or extended budget. Foreground open, module
destruction, account replacement and cancellation retire late work. A failed
cold retirement quarantines the sole owner rather than opening a second Client
against the same journal. Existing foreground graceful-close recovery stays in
its own actor. System completion and background-task cleanup each occur once.
The OS can deny or terminate the window; completion is not delivery assurance.

Sync uses the existing strict `wait_until_settled` predicate and only the time
remaining from callback admission. It does not treat online/empty outbox as MLS
completion. A cold baseline is captured after unlocking and before sync; a warm
baseline comes from the last authenticated snapshot observed by the foreground
session. The same current-account, membership, mute and archive checks produce
the same local identifiers as JavaScript. The native adapter rechecks scope and
the projection around asynchronous OS writes and removes its note if retired.
These checks are observations, not an atomic transaction with Notification Center.

In the matching opt-in iOS binary, JavaScript background snapshot events delegate
to that native presenter through `notificationWake`. The bridge receives only
expected public identity/device IDs; the module lease and native engine/generation
must still match at execution. It cannot select another account or pass content.
WS and APNs share the same in-flight admission/baseline, while JavaScript retains
token handling, taps, badges and cleanup. Android, native-off and older bridge
versions retain the existing JavaScript presenter. A matching JS/native build is
required. Identifiers replace pending requests; reusing an already-delivered ID
alone is not proof of exactly-once visual alerts. No such device claim is made.

This source requires genuine regenerated UniFFI bindings/checksums and a new
native build. Model/config/backend tests are distinct from physical-device
APNs registration, locked/background/cold delivery and navigation acceptance.
No provider credentials or device delivery are established by this change.

## Android configuration

Android has its own build opt-in. Download the public Firebase client
`google-services.json` for package `xyz.tironi.zoen`, then build with:

```text
ZOEN_ANDROID_FCM_ENABLED=true
ZOEN_ANDROID_GOOGLE_SERVICES_FILE=/absolute/path/to/google-services.json
```

The existing `android.googleServicesFile` config may supply that path instead.
`app.config.ts` checks the public Firebase project ID and matching Android package,
sets the native file path and adds the notifications config plugin when absent,
with the default
`zoen-messages` channel. It exposes only
`extra.zoenPush.android = {enabled:true, platform:'fcm', projectId}`. APNs settings
never enable Android registration. This Android setup preserves configured iOS
options and does not itself enable the iOS remote-notification entitlement.

The private FCM service-account JSON belongs only to the relay's
`ZOEN_FCM_SERVICE_ACCOUNT`, outside the app, public extras and repository. The
app's Firebase client file is a different document; supplying a service-account
key as that file is refused. The native relay sends FCM HTTP v1 directly. No EAS
push credential upload or Expo Push Service is involved in this registration flow.

The SDK58 adapter creates the native Android channel before an explicit permission
request and before `getDevicePushTokenAsync`, then accepts only a token whose SDK
type is `android`. Passive permission checks do not create a channel or prompt.
Local notifications use that same channel. On Android versions where notification
permission is already granted by the OS, foreground registration can reuse it;
the build must still opt in explicitly.

This implementation supports received wakes while the existing app runtime is
running. Real Firebase configuration, a newly signed native build, device/provider
delivery and full background operation still require separate acceptance.
Enabling this configuration requires regenerating the native project and building
a new binary; reloading JavaScript alone cannot configure Firebase.

## Wake and presentation boundary

Remote notifications carry only a wake. They never navigate, approve, execute,
or show untrusted remote message text. `decodeWake` bounds the input and keeps
the entire UInt64 cursor exact. JavaScript object input has already lost lexical
JSON integer-vs-float information; strict raw APNs type validation still belongs
at the native boundary. Rust sync remains responsible for account, membership,
log and decryption validation. The running JavaScript adapter refreshes its
existing Rust projection on a valid wake. The iOS native subscriber above uses
the same session ownership boundary when React is absent; Android background
execution remains a separate qualification boundary.

The native FCM sender's complete data payload is `{wake:"1"}`; it has no APNs
`aps` object, content or cursor. Expo58 exposes Android data in
`trigger.remoteMessage.data`, and iOS APNs in `trigger.payload`. The adapter also
accepts the Android serializer's injected `dataString:null` for an absent body.
It refuses a non-null notification, a body string, actions, routes, extra data
fields and coerced wake values. A valid wake only refreshes the current
authenticated core; it cannot choose a route or grant access.

Local notification content comes only from current authenticated snapshots after
sync, excludes muted/archived spaces and own messages, and uses deterministic
identifiers. Taps accept only the current device's strict local metadata and a
current member space. Cold-start notification responses are read once and stale
callbacks are discarded. A remote payload cannot supply an arbitrary URL.

`+native-intent` accepts documented `zoen://app`, `chat`, `join` and `agent` links
and HTTPS links on `tryzoen.com`. Capability invitations reach a preview screen
before joining. Browser `/a`, `/join` and `/app` routes use the same UI. Universal
link association/hosting still require the matching deployment configuration.

Boundary tests live in `src/core/notifications.test.ts` and `deep-links.test.ts`.
They prove input bounds, serialized rotations, acknowledgement timing, account
invalidation, bounded retries and safe navigation; they do not prove APNs delivery.
Android-specific cases in `src/core/notifications-android.test.ts` cover the
explicit build opt-in, actual SDK event shape, backend token bounds, channel order,
serialized rotations and stale callbacks. They do not prove FCM delivery.
`native-push-wake.test.ts` prepares shared JavaScript/native presentation vectors
and a Foundation-only Swift ownership/payload model harness;
`notifications-ios-build.test.ts` exercises actual CNG configuration. Neither
harness compiles the UIKit/Core adapter or establishes physical push delivery.
`notification-presentation.test.ts` checks the single-presenter port and actual
SDK adapter opt-in/version gate without invoking an OS notification service.

The relay attempts content-free wakes for all currently authorized recipient
devices except the author, including an identity whose sibling has a live WS.
Presence does not prove another device executed the event. Existing whole-job
admission remains capped at 32 concurrent jobs, including lookup and provider
work; overload still drops best-effort wakes before allocation/spawn. No new
retry or delivery queue is added. Provider attempts can increase from only
offline identities to every registered recipient device; throughput and cost
must be measured with deployment traffic. Aggregate attempt counts omit device
IDs, tokens and provider error strings, and are not APNs/FCM delivery receipts.

References: [Expo SDK58 Notifications](https://docs.expo.dev/versions/v58.0.0/sdk/notifications/),
[Firebase client configuration](https://docs.expo.dev/push-notifications/fcm-credentials/).
The Expo guide's EAS/Expo Push Service steps are not part of Zoen's direct relay
transport.
