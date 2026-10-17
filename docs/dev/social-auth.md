# Apple and Google entry

The entry flow is **Continue with Apple/Google → provider confirmation → Zoen**.
New users do not fill a registration form. After server verification, the Rust core
automatically creates identity/device/agreement keys in its existing vault, generates
an initial handle, and installs the initial profile. Names and handles remain editable
later. Permissions and backup are separate later decisions.

A provider login identifies an existing Zoen identity but cannot decrypt its history
or enroll a new device. A bound account with no matching local keys must use the
existing secure device-link or encrypted-backup recovery. Email is never an automatic
account-linking key. The provider proof stays in memory until its signed, idempotent
binding completes. A retry of a pending setup preserves the already generated keys.

Native provisioning first writes one protected vault staging record containing the
fixed identity, device and agreement keys. It then validates existing key slots and
installs account/profile metadata atomically. On cold open, `resumeProviderSetup`
finishes that installation before orphaned-key checks or unlock. Conflicting keys or
accounts are preserved and rejected. The staging record is removed only after all
installed keys and metadata have been verified. Eight native regression tests cover
interrupted vault writes, SQL failures, cold reopen and lost bind responses; the app
store also refreshes a retained pending identity after a failed request, allowing a
same-session retry.

## Public Google configuration confirmed in Chrome

On 2026-10-17, the existing Google Cloud project **Zoen** (`zoen-506921`) was inspected.
The user explicitly confirmed creation of the iOS client and authorization of the
production Web origin. Actions ran in a separate Chrome window with the same existing
profile. No client secret or plist was downloaded, changed, rotated or copied.

| Setting                  | Confirmed value                                                            |
| ------------------------ | -------------------------------------------------------------------------- |
| iOS client name          | `Zoen iOS`                                                                 |
| iOS bundle               | `xyz.tironi.zoen`                                                          |
| iOS OAuth client         | `325478030576-giiqrc2gq04t7l0jhf7i48k3vbc528qo.apps.googleusercontent.com` |
| iOS URL scheme           | `com.googleusercontent.apps.325478030576-giiqrc2gq04t7l0jhf7i48k3vbc528qo` |
| Existing Web client name | `Zoen`                                                                     |
| Web/server OAuth client  | `325478030576-2b1200lljfof4b89iq7mh5ufju55dni1.apps.googleusercontent.com` |
| Added JavaScript origin  | `https://app.tryzoen.com`                                                  |

Creation was confirmed by the console's success dialog and subsequent client list.
The added Web origin was confirmed by reopening the saved client. Existing origins
(`https://zoen.tironi.xyz` and the two prior loopback development origins) and all four
existing redirect URIs were preserved. Console success does not prove a real sign-in
or immediate provider propagation.

Public app values are supplied through `app.config.ts` / public auth configuration:

```text
EXPO_PUBLIC_GOOGLE_IOS_CLIENT_ID=<iOS OAuth client above>
EXPO_PUBLIC_GOOGLE_WEB_CLIENT_ID=<Web/server OAuth client above>
```

The relay uses the Web client as native Google audience and the iOS client as its
authorized presenter, plus the exact Web origin:

```text
ZOEN_AUTH_GOOGLE_IOS_AUDIENCE=<Web/server OAuth client above>
ZOEN_AUTH_GOOGLE_IOS_PRESENTER=<iOS OAuth client above>
ZOEN_AUTH_GOOGLE_WEB_AUDIENCE=<Web/server OAuth client above>
ZOEN_AUTH_WEB_ORIGINS=https://app.tryzoen.com
```

The native implementation uses SDK58 Expo AppleAuthentication and MIT Nitro Google
Sign-In 2.3.0 / Nitro Modules 0.37.1. Google calls the interactive SDK path with a fresh
server nonce and no additional API/offline scopes; its app session is signed out after
capturing the credential. An older binary checks optional native modules before loading
their JavaScript adapters. The Expo config plugin adds the Google URL scheme. The local
`ZoenGoogleSignInDelegate: ExpoAppDelegateSubscriber` forwards Google's documented
`GIDSignIn.handle(url)` callback through Expo's cold and warm URL lifecycle. Its subscriber
is registered in the local module's Apple configuration, with `GoogleSignIn ~>10.0`
matching Nitro's native dependency. CNG and the signed RN0.88 simulator build pass. The actual native Google confirmation
opens, and cancellation returns to onboarding without creating an account. Successful
provider credential return and cold/warm callback completion still require acceptance.

Expo Router also sees incoming URLs. Its native intent handler recognizes only the
configured Google iOS reverse scheme and `/oauth2callback`, and returns the protected
root route without forwarding the authorization code or state into screen parameters.
The native Google SDK remains the only credential handler; this navigation hint never
authenticates an account. A unit regression covers the callback and foreign schemes.

## Apple portal work remains pending

Apple App ID capability and device provisioning have not been verified. The source
already declares the native capability; successful simulator signing cannot establish
physical-device entitlements.

To finish **iPhone** configuration:

1. Inspect the existing paid developer team and explicit App ID `xyz.tironi.zoen`.
   Enable Sign in with Apple on that App ID and update development/distribution
   provisioning through the normal CNG/EAS signing path. The app already declares
   the Expo Apple plugin and `ios.usesAppleSignIn`.
2. Set `ZOEN_AUTH_APPLE_IOS_AUDIENCE=xyz.tironi.zoen` on the chosen relay. Native
   Apple authentication uses the bundle ID; it needs no Web Services ID or HTTPS
   browser redirect.
3. Use a relay reachable from the physical phone. The isolated simulator build uses
   `http://127.0.0.1:8790`; on a physical phone that address refers to the phone itself.
   Configure the intended HTTPS relay before building a device release.
4. Generate matching development/distribution provisioning with the Apple capability
   enabled. A signed Simulator app does not prove device signing or Apple entitlement
   availability. Local EAS login/project linking and device credentials remain setup
   steps; use existing team identifiers and credentials when available.

Web setup is deferred. Later, inspect or create a Services ID associated with the
primary App ID, register `app.tryzoen.com` and its exact owned HTTPS return URI, and
provide `EXPO_PUBLIC_APPLE_SERVICE_ID`, `EXPO_PUBLIC_APPLE_REDIRECT_URI` and relay
`ZOEN_AUTH_APPLE_WEB_AUDIENCE`. Those values are not prerequisites for native iPhone
authentication.

Apple private keys remain backend-only. Team identifiers also appear in normal device
provisioning and are not secrets. The present relay verifies ID tokens and discards
optional authorization codes; exchange, refresh-token handling and account-deletion
token revocation need a separately verified integration before claiming those provider
lifecycle features.

## Web isolation and entry page

The main app retains `COOP: same-origin` and `COEP: require-corp` for the Rust MLS
worker's SharedArrayBuffer. Social entry runs on `/sign-in` with only that route's
non-isolated headers (`COOP: same-origin-allow-popups`, `COEP: unsafe-none`). It uses
official Google Identity Services / Apple JS credentials and keeps token/proof state
in memory until binding; it then reloads the isolated root after durable setup.
Global isolation must not be relaxed to make a provider popup work.

The Web adapter refuses embedded contexts and isolated popup contexts, loads only
fixed official SDK URLs, and accepts credentials only through its owned live SDK
callback or Apple sign-in promise. It installs no generic `window.message` credential
receiver. Google requests FedCM button support with automatic selection disabled.
Native and credential SDK flows do not expose an app-managed PKCE code exchange;
the server owns exact nonce/state verification. Provider code-exchange flows added
later need the applicable provider-specific security and redirect contract.

## Client contract and evidence

`src/auth/types.ts` exposes `AuthService`, `ProviderConfiguration`, and the verified
result passed to the parent `SignInButtons.onVerified`. `createExpoAuthService` uses
`expo/fetch` on native and normalizes the relay's wire names (`id`, `expires_ms`,
`account.status`, `name`) into internal `challenge_id`, `expires_at_ms`, `account.kind`,
and `profile.name`. Native Apple's first-login name is a separate initial-profile hint.

`guardCredentialContext` only rejects nonce/audience/issuer/expiry/state mismatches;
decoding a matching JWT does not authenticate it. Only the relay's signature/JWKS
verification produces a provider proof. [Relay contract and database acceptance](provider-auth.md)
documents that authority. Parent entry UI calls
`provider_sign_in { relay_url, proof_token, name }` for unbound or matching pending
accounts; existing bound identities without matching keys route to recovery.

Ten client and entry unit tests pass: malformed/expired challenges and credentials, audience/
nonce/state/issuer mismatches, exact-origin transport and bounded responses, one-use
owned tickets, late account/unmount results, stale SDK callback rejection, new-device
recovery before key creation, and pending binding retries without duplicate identities. Actual signed simulator acceptance covers native module loading, the branded Google
control, provider presentation and cancellation. Real credentials and account creation
remain outside that evidence. Test command:

```sh
cd apps/zoen
node --test src/auth/*.test.ts
```

Runtime acceptance still requires real Google and Apple confirmation, cancellation,
first account provisioning, a restart that loads the same identity, repeat binding
without duplicate keys, and an existing-account new-device recovery. Verify native
redirect handling and Web `/sign-in` popups plus the subsequent isolated MLS startup
in supported browsers. No real provider login or new-device recovery is claimed by
the console configuration or the unit tests.

Further Web provider acceptance is deferred. The earlier browser entry check failed
origin/FedCM validation; the acceptance ledger records it. Console configuration
and a visible provider button do not establish successful authentication.

## Primary references

- [Expo Google authentication](https://docs.expo.dev/guides/google-authentication/)
- [Expo AppleAuthentication SDK58](https://docs.expo.dev/versions/v58.0.0/sdk/apple-authentication/)
- [Nitro Google source and setup](https://github.com/react-native-nitro-google-sign-in/google-signin)
- [Google iOS integration and redirect handling](https://developers.google.com/identity/sign-in/ios/sign-in)
- [Google backend authentication](https://developers.google.com/identity/sign-in/ios/backend-auth)
- [Google GIS API and FedCM button](https://developers.google.com/identity/gsi/web/reference/js-reference)
- [Google Web setup/CSP and popup policy](https://developers.google.com/identity/gsi/web/guides/get-google-api-clientid)
- [Apple native/server authentication](https://developer.apple.com/documentation/signinwithapple/authenticating-users-with-sign-in-with-apple)
- [Apple JS setup](https://developer.apple.com/documentation/signinwithapple/configuring-your-webpage-for-sign-in-with-apple)
- [Apple official Web button image API](https://developer.apple.com/documentation/signinwithapplerestapi/get-a-sign-in-with-apple-button-that-is-center-aligned.)
