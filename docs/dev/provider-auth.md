# Provider authentication

The relay authenticates Apple and Google subjects independently of Zoen's encryption keys. A provider assertion cannot recover keys, enroll a new device into an existing identity, or merge identities by email.

Each configured provider/client pair has an exact audience:

| Environment variable | ID token audience |
| --- | --- |
| `ZOEN_AUTH_APPLE_IOS_AUDIENCE` | Apple iOS bundle identifier |
| `ZOEN_AUTH_APPLE_WEB_AUDIENCE` | Apple Services ID |
| `ZOEN_AUTH_GOOGLE_IOS_AUDIENCE` | Audience emitted by native Google sign in, usually its server/web client ID |
| `ZOEN_AUTH_GOOGLE_WEB_AUDIENCE` | Google browser client ID |

An optional `ZOEN_AUTH_<PROVIDER>_<CLIENT>_PRESENTER` supplies the exact `azp` when it differs from the audience. Native Google uses the iOS OAuth client ID here. Otherwise an asserted `azp` must equal the audience. A missing audience disables that pair. No real provider credentials are checked into this repository.

Set `ZOEN_AUTH_WEB_ORIGINS` to a comma separated list of exact HTTPS origins. HTTP is accepted only for localhost/loopback development. Web challenges require an allowed Origin; verification and binding preserve that same origin. The auth routes use POST/JSON, permit no credentials or cookies, and use their own IP rate buckets (`auth_challenge_ip`, `auth_verify_ip`, `auth_bind_ip`).

`POST /v1/auth/challenge` accepts `{provider: "apple" | "google", client: "ios" | "web"}`. It returns `{id, nonce, state, audience, expires_ms}`. The ID, nonce and state are independently generated 256 bit hex values. Use the nonce verbatim in the provider flow and preserve the returned state. Challenges expire after five minutes.

`POST /v1/auth/verify` accepts `{challenge_id, id_token, state, authorization_code?}`. The ID token is verified with the provider's fixed HTTPS JWKS endpoint, RS256, exact issuer/audience/presenter/nonce and bounded issue/expiry times. Token key URLs are refused. A successful assertion atomically consumes its challenge and returns `{proof_token, expires_ms, account, name?}`. Account is `{status: "unbound"}` or `{status: "bound", identity_id, handle}`. Name is an optional display hint. Email never participates in account lookup. The optional authorization code is not retained or exchanged; this flow requires an ID token.

`POST /v1/auth/bind` accepts the shared `roda_proto::provider_auth::ProviderBindRequest`. The device signs `provider_bind_message`; the root certifies the device with `device_cert_message`. The Person directory profile has empty name/bio; private fields go through the existing encrypted profile pipeline. First registration, device enrollment, provider binding and proof consumption share one SQL transaction. A retry with the identical signed request returns `{identity_id, handle}`. A revoked device stays revoked. Binding an already registered identity requires an existing active device and never edits the directory profile.

Proofs are opaque random tokens with a five minute lifetime; only hashes are stored. Provider bindings use `(provider, issuer, subject)`. Raw ID tokens, authorization codes and proof tokens are not stored in application logs or the database. The RSA key under `crates/roda-relay-admission/src/auth/fixtures/` is a disposable synthetic test fixture, with no relationship to provider keys.

Run the database concurrency test in an isolated schema:

```sh
ZOEN_TEST_PG=postgres://user@127.0.0.1:5432/database cargo test -p zoen-relay --lib auth::storage::tests -- --ignored
```

The test creates and drops its own random schema and leaves existing application data intact.


## Shared Cloudflare assertion policy

`roda-relay-admission::auth` owns the RS256 verifier, bounded JWT/JWKS parsing,
issuer/audience/presenter/nonce/time checks and subject/display-hint projection.
The native relay retains its provider-specific HTTPS fetch/cache and calls this
same policy. The portable backend compiles the existing ring verifier to WASM;
it does not reimplement verification in JavaScript.

```sh
rustup component add llvm-tools-preview
node scripts/verify-cloudflare-provider.mjs --out-dir .dev/cloudflare-provider
```

The runner signs 18 assertions with a disposable test-only key: three valid
Apple/Google cases and fifteen refusals for claims, headers, signature changes
and encryption-only keys. Native results are compared exactly with Node and
actual workerd/WASM, without provider network calls, credentials or deployment.
The full library build contains no test signing key. The native cache regression
and all-target strict Clippy also retain their separate checks.

The portable exports require trusted provider keys, a persisted challenge and
the host clock. They establish assertion verification only. A complete Cloudflare
HTTP adapter still needs origin/settings/rate policy, fixed-endpoint JWKS caching,
atomic one-use challenge/proof consumption and account/device/provider binding in
the Directory transaction domain before Apple/Google can be enabled publicly.
The existing native HTTP enrollment and secure new-device recovery boundaries
above remain authoritative.
