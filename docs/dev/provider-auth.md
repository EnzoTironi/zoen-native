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

The assertion exports require trusted provider keys, a persisted challenge and
the host clock. Their assertion-only result does not authorize account enrollment.
The existing native HTTP enrollment and secure new-device recovery boundaries
above remain authoritative.

## Private Cloudflare enrollment host

`094a5bb0` adds the private host to the existing Directory Durable Object, sharing
its Person/device tables and SQLite transaction boundary. Shared Rust owns strict
challenge/bind validation, signatures and enrollment decisions. The host owns
entropy, fixed-endpoint HTTPS/cache I/O and atomic SQL writes. No second account
store or provider-subject injection API is introduced.

Original request text reaches Rust before JavaScript parses fields, so duplicate
JSON fields are refused. Request bodies are bounded to 32 KiB, JWKS streams to 64 KiB,
provider fetches to 5 seconds and operations to 10 seconds. Workers fetch cannot
separately enforce the native three-second connect timeout. Only proof hashes are
durable; assertions and authorization codes are not retained. Provider confirmation
still cannot enroll a new device into an existing account or recover its keys.

With an isolated test database configured, run:

```sh
node scripts/verify-cloudflare-provider.mjs --host --out-dir .dev/cloudflare-provider-host
```

The exact command passes 18 native/18 Node/18 workerd assertion cases, 20 actual
PostgreSQL issue/bind scenarios and 35 workerd host cases, with zero skips. Existing
Directory 18 workerd regressions also pass with the same generated WASM. Coverage
includes concurrent verification/binding, retry after expiry with the original
receipt, exact current certificates, unchanged existing profiles, late SQL rollback,
fixed URL/redirect refusal, body limits, cache refresh and malformed requests.
Receipt: `/private/tmp/zoen-cloudflare-admission-v96/validation/provider-host-final-acceptance/acceptance.json`
(SHA256 `52ef524c34eb50a2787645190bcd2817441d96dfd1b5b1af02b486b6d5a9e8e1`).

This host is not deployed. Outer Worker HTTP remains 404 and missing provider/client
configuration remains disabled. Public gateway IP limits, actual Apple/Google
confirmation, recovery integration and complete public synchronization remain
separate work. These private checks do not establish public login acceptance.
