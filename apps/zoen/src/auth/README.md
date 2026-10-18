# Social authentication checkpoint

The approved flow is **Continue with Apple/Google → provider confirmation → Zoen**.
There is no registration form. An unbound provider automatically provisions a fresh
Rust identity/device key pair and editable initial profile. A bound provider on a
new device identifies the account and routes to existing secure link/backup recovery;
OAuth alone never grants a device certificate or decrypts existing history.

## Approved boundary

1. `POST /v1/auth/challenge` with `{provider, client}` returns `AuthChallenge`.
   Server nonce and state are cryptographically random, single-use, valid for at most
   five minutes, and bound to the configured provider/client audience. Use a 64-character
   hex nonce unchanged across SDKs; Nitro's native iOS implementation passes it directly
   to Google Sign-In without additional hashing.
2. `POST /v1/auth/verify` with `{challenge_id, id_token, authorization_code?, state?}`
   verifies provider signature/JWKS, algorithm, issuer, audience, expiry/issuance time,
   exact nonce and state where applicable, then consumes the challenge atomically.
   It returns `AuthVerification`, including optional verified profile name.
3. A short-lived opaque `proof_token` remains in memory. A signed device request binds
   an unbound provider to a cryptographic identity. Existing bindings cannot be overwritten
   or joined by matching email. Binding and automatic provisioning are idempotent.

The backend owns token verification and signed binding. Client adapters and buttons
live under this directory with an injectable `AuthService`. Native callers use
`createExpoAuthService`, which supplies SDK58's streaming `expo/fetch` transport.
No provider tokens are persisted, logged, put in navigation URLs, or treated as relay
credentials. Native Apple's first-login full name is an optional initial user-provided
profile hint, separate from verified identity claims.

## Provider selection and setup

- iOS Apple: SDK58 `expo-apple-authentication`, native approved Apple button,
  `nonce` and `state`; plugin plus `ios.usesAppleSignIn=true` and the Apple App ID capability.
- iOS Google: MIT `react-native-nitro-google-signin` 2.3.0 and
  `react-native-nitro-modules` 0.37.1. Use `presentExplicitSignIn`, not cached restore,
  with exact nonce, explicit iOS/web client IDs and no additional API/offline scopes.
  The declared RN range includes 0.88, but upstream's detailed compatibility matrix
  currently stops at 0.87; a successful native build/runtime check is still required.
- Google plugin without Firebase:
  `['react-native-nitro-google-signin', {iosUrlScheme: '<REVERSED_IOS_CLIENT_ID>'}]`.
  The iOS OAuth client uses bundle `xyz.tironi.zoen`; web/server audience uses a Web
  OAuth client. Web origins must match the real deployed origin.
- Web uses official Google Identity Services and Apple JS credential flows, supplying
  exact nonce/state. Their native/credential SDK flows do not expose an app-managed
  authorization-code PKCE exchange. No client secret is shipped to the app.
- Apple Web requires an associated Services ID, owned HTTPS domain/return URL and
  the app's primary App ID. Apple Team ID/Key ID/private key stay on the backend for
  authorization-code exchange/revocation. Provider console setup is not runtime proof.

## Primary references consulted

- [Expo Google authentication](https://docs.expo.dev/guides/google-authentication/)
- [Expo AppleAuthentication SDK58](https://docs.expo.dev/versions/v58.0.0/sdk/apple-authentication/)
- [Expo AuthSession SDK58](https://docs.expo.dev/versions/v58.0.0/sdk/auth-session/)
- [Nitro public types](https://github.com/react-native-nitro-google-sign-in/google-signin/blob/main/src/specs/nitro-google-signin.nitro.ts)
- [Nitro native nonce implementation](https://github.com/react-native-nitro-google-sign-in/google-signin/blob/main/ios/HybridNitroGoogleSignin.swift)
- [Google server verification](https://developers.google.com/identity/sign-in/ios/backend-auth)
- [Google GIS JavaScript API](https://developers.google.com/identity/gsi/web/reference/js-reference)
- [Google web setup/CSP](https://developers.google.com/identity/gsi/web/guides/get-google-api-clientid)
- [Apple authentication](https://developer.apple.com/documentation/signinwithapple/authenticating-users-with-sign-in-with-apple)
- [Apple JS setup](https://developer.apple.com/documentation/signinwithapple/configuring-your-webpage-for-sign-in-with-apple)

Seven client tests pass, covering challenge/credential context, transport bounds,
ticket ownership, teardown and official SDK callback ownership. They do not perform
provider signature verification or authenticate a real provider account. Google iOS
client and Web origin configuration were saved and re-read in the console; Apple
console setup and real provider login/recovery remain pending. See
[social auth setup and acceptance](../../../../docs/dev/social-auth.md).
