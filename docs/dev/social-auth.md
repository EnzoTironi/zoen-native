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
matching Nitro's native dependency. CNG, pod installation, and cold/warm redirect behavior
still require validation in the generated app. The upstream compatibility table stops
at RN0.87; RN0.88 requires build/runtime validation.

Expo Router also sees incoming URLs. Its native intent handler recognizes only the
configured Google iOS reverse scheme and `/oauth2callback`, and returns the protected
root route without forwarding the authorization code or state into screen parameters.
The native Google SDK remains the only credential handler; this navigation hint never
authenticates an account. A unit regression covers the callback and foreign schemes.

## Apple portal work remains pending

The Apple Developer portal was not inspected or modified. The Chrome tab integration
was unavailable; native Chrome control could not bind a background window on this Mac.
When the user returned to their personal window, the tooling stopped the pending UI
action. Configuration work paused rather than navigating or taking over that window.
No Apple capability, Services ID, key, agreement, payment or provisioning claim follows
from the completed Google configuration.

To finish Apple configuration:

1. Inspect the existing paid developer team and explicit App ID `xyz.tironi.zoen`.
   Enable Sign in with Apple on that App ID and update development/distribution
   provisioning through the normal CNG/EAS signing path. The app already declares
   the Expo Apple plugin and `ios.usesAppleSignIn`.
2. Inspect or create the Web Services ID associated with that primary App ID. Register
   `app.tryzoen.com` and the exact owned HTTPS return URI. Do not substitute localhost
   or invent a Services ID without checking existing identifiers.
3. Supply `EXPO_PUBLIC_APPLE_SERVICE_ID` and `EXPO_PUBLIC_APPLE_REDIRECT_URI` to the app,
   plus `ZOEN_AUTH_APPLE_IOS_AUDIENCE=xyz.tironi.zoen` and
   `ZOEN_AUTH_APPLE_WEB_AUDIENCE=<actual Services ID>` to the relay. The popup adapter
   requires the return URI to share the current page's HTTPS origin.
4. Any Apple private key / Team ID / Key ID remains backend-only. The present relay
   verifies ID tokens and discards optional authorization codes; exchange, refresh-token
   handling and account-deletion token revocation need a separately verified integration
   before claiming those provider lifecycle features.

Portal changes that create sensitive access or accept legal terms require their own
action-time human confirmation. The Google confirmation above does not authorize
unreviewed Apple changes, purchases, agreements, or private-key creation.

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
recovery before key creation, and pending binding retries without duplicate identities. Native
adapter/module availability, native branded buttons and provider behavior are source
checks until exercised on the rebuilt app. Test command:

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

The current localhost in-app-browser check renders the official Google button on
`/sign-in`, but Google reports an unauthorized-origin error and a FedCM token error.
The client ID shown in the rendered SDK iframe matches the saved Web client. Console
propagation and a supported standalone browser still need verification; no successful
Google login is inferred from the rendered button.

A later local browser check on `http://localhost:58704/sign-in` rendered the official
Google button and obtained a real relay challenge. Google then rejected the origin for
the configured client and FedCM returned a network error. This is a failed provider
entry check, pending origin/provider acceptance; no policy relaxation, silent fallback
or successful sign-in follows from the visible button. The console origins above are
the earlier observed configuration, not a claim that this browser flow was accepted.

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
