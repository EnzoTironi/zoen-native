# Push (content-free wakes, ADR 0047)

Messages are end-to-end encrypted, so the relay can't say what arrived. A push only
wakes the app: APNs `{"aps":{"content-available":1}}` (background, priority 5) and FCM
data `{"wake":"1"}` (high priority, no `notification`). The app syncs, decrypts on the
device and shows a local notification, skipping muted chats. The relay wakes only members
with no live session, and drops tokens that APNs/FCM report dead.

With no credentials, push is **off** and everything else works as before.

## What Enzo provides

### Apple (APNs, token-based)
Needs a paid Apple Developer Program membership (US$ 99/yr).
1. Certificates, IDs & Profiles → Identifiers → `xyz.tironi.zoen`: enable **Push Notifications**.
2. Keys → **+** → enable **Apple Push Notifications service (APNs)** → download `AuthKey_<KEYID>.p8` (downloadable once).
3. Hand over, as Fly secrets on the relay (never in the repo):

| Secret | Value |
|---|---|
| `ZOEN_APNS_KEY_P8` | the full contents of the `.p8` file |
| `ZOEN_APNS_KEY_ID` | the 10-character Key ID |
| `ZOEN_APNS_TEAM_ID` | the 10-character Team ID (Membership page) |
| `ZOEN_APNS_TOPIC` | `xyz.tironi.zoen` |

4. The app target then needs the `aps-environment` entitlement (Xcode → Signing &
   Capabilities → Push Notifications) on the paid team. It stays off the `Device`
   (Personal Team) config, which can't sign it. Debug builds register as `apns-sandbox`,
   release as `apns`.

### Google (FCM HTTP v1)
1. console.firebase.google.com → new project → add Android app `xyz.tironi.zoen`.
2. Project settings → Service accounts → **Generate new private key** (JSON).
   Relay secret: `ZOEN_FCM_SERVICE_ACCOUNT` = the whole JSON.
3. Project settings → General → your Android app: the app reads these Gradle
   properties (in `~/.gradle/gradle.properties` or CI secrets, never committed):
   `zoen.FIREBASE_PROJECT_ID`, `zoen.FIREBASE_APP_ID` (mobilesdk_app_id),
   `zoen.FIREBASE_API_KEY` (current_key), `zoen.FIREBASE_SENDER_ID` (project_number).
   No `google-services.json` is needed.

Set on Fly with `fly secrets set ZOEN_APNS_KEY_P8="$(cat AuthKey_X.p8)" ...` and redeploy;
the relay logs `push apns=true fcm=true` at startup.
