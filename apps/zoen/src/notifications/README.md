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
log and decryption validation. Currently the running app refreshes its existing
Rust projection on a valid wake. Headless background delivery still needs a safe
native single-session sync entry point and TaskManager integration; starting a
second JS-owned engine is deliberately avoided.

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

References: [Expo SDK58 Notifications](https://docs.expo.dev/versions/v58.0.0/sdk/notifications/),
[Firebase client configuration](https://docs.expo.dev/push-notifications/fcm-credentials/).
The Expo guide's EAS/Expo Push Service steps are not part of Zoen's direct relay
transport.
