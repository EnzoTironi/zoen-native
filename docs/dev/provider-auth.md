# Provider authentication

The relay authenticates Apple and Google subjects independently of Zoen's encryption keys. A provider assertion cannot recover keys, enroll a new device into an existing identity, or merge identities by email.

Each configured provider/client pair has an exact audience:

| Environment variable                | ID token audience                                                                   |
| ----------------------------------- | ----------------------------------------------------------------------------------- |
| `ZOEN_AUTH_APPLE_IOS_AUDIENCE`      | Apple iOS bundle identifier                                                         |
| `ZOEN_AUTH_APPLE_WEB_AUDIENCE`      | Apple Services ID                                                                   |
| `ZOEN_AUTH_GOOGLE_IOS_AUDIENCE`     | Audience emitted by native Google sign in, usually its server/web client ID         |
| `ZOEN_AUTH_GOOGLE_WEB_AUDIENCE`     | Google browser client ID                                                            |
| `ZOEN_AUTH_GOOGLE_ANDROID_AUDIENCE` | Android server/backend client audience; requires its own explicit Android presenter |

An optional `ZOEN_AUTH_<PROVIDER>_<CLIENT>_PRESENTER` supplies the exact `azp` when it differs from the audience. Native Google uses the iOS OAuth client ID here. Otherwise an asserted `azp` must equal the audience. A missing audience disables that pair. No real provider credentials are checked into this repository.

Set `ZOEN_AUTH_WEB_ORIGINS` to a comma separated list of exact HTTPS origins. HTTP is accepted only for localhost/loopback development. Web challenges require an allowed Origin; verification and binding preserve that same origin. The auth routes use POST/JSON, permit no credentials or cookies, and use their own IP rate buckets (`auth_challenge_ip`, `auth_verify_ip`, `auth_bind_ip`).

`POST /v1/auth/challenge` accepts `{provider: "apple" | "google", client: "ios" | "web" | "android"}`. It returns `{id, nonce, state, audience, expires_ms}`. The ID, nonce and state are independently generated 256 bit hex values. Use the nonce verbatim in the provider flow and preserve the returned state. Challenges expire after five minutes.

`POST /v1/auth/verify` accepts `{challenge_id, id_token, state, authorization_code?}`. The ID token is verified with the provider's fixed HTTPS JWKS endpoint, RS256, exact issuer/audience/presenter/nonce and bounded issue/expiry times. Token key URLs are refused. A successful assertion atomically consumes its challenge and returns `{proof_token, expires_ms, account, name?}`. Account is `{status: "unbound"}` or `{status: "bound", identity_id, handle}`. Name is an optional display hint. Email never participates in account lookup. The optional authorization code is not retained or exchanged; this flow requires an ID token.

`POST /v1/auth/bind` accepts the shared `protocol::provider_auth::ProviderBindRequest`. The device signs `provider_bind_message`; the root certifies the device with `device_cert_message`. The Person directory profile has empty name/bio; private fields go through the existing encrypted profile pipeline. First registration, device enrollment, provider binding and proof consumption share one SQL transaction. A retry with the identical signed request returns `{identity_id, handle}`. A revoked device stays revoked. Binding an already registered identity requires an existing active device and never edits the directory profile.

Proofs are opaque random tokens with a five minute lifetime; only hashes are stored. Provider bindings use `(provider, issuer, subject)`. Raw ID tokens, authorization codes and proof tokens are not stored in application logs or the database. The RSA key under `crates/relay-admission/src/auth/fixtures/` is a disposable synthetic test fixture, with no relationship to provider keys.

Run the database concurrency test in an isolated schema:

```sh
ZOEN_TEST_PG=postgres://user@127.0.0.1:5432/database cargo test -p zoen-relay --lib auth::storage::tests -- --ignored
```

The test creates and drops its own random schema and leaves existing application data intact.

## Native policy and verification

`relay-admission::auth` owns bounded RS256/JWT/JWKS validation and exact
issuer/audience/presenter/nonce/time checks. Native HTTP owns provider-specific
fixed HTTPS endpoints, caching, entropy and the authoritative PostgreSQL
transaction. Staged enrollment helpers execute under its existing row/advisory
locks; there is no alternate account store or host-supplied subject route.

```sh
cargo test --locked -p relay-admission --lib auth::tests
cargo test --locked -p relay-admission --test provider_enrollment
cargo test --locked -p zoen-relay --lib auth::tests
```

The disposable RSA fixture signs synthetic assertions only. Tests cover actual
signature tampering, issuer/audience/nonce/presenter/time and fixed-key selection;
provider confirmation is a separate real credential/HTTPS journey. The local
composition intentionally has no invented provider credentials. Configure the
real public IDs and origins before accepting Apple/Google login.

The retired private Cloudflare host and WASM assertion exports are no longer a
supported backend. Their old differential receipts retain historical attribution
in the acceptance manifest and immutable source; they are not current native
runtime acceptance. Static Web hosting and the provider popup isolation policy
remain separate from relay authentication.
