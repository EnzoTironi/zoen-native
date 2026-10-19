This private adapter keeps one directory transaction domain per relay cell. Identities,
unique handles, globally unique devices, sealed one-time link boxes, handshake challenges
and host sessions belong to the same SQLite Durable Object. A caller cannot choose an
account shard or supply `registered`, an enrollment row, or an authenticated session.

`hello({frame})` accepts the canonical native binary Hello, verifies the identity's device
certificate in shared Rust, and returns a native protocol-4 Challenge plus an opaque
challenge ID. Its nonce comes from the host's secure random generator. `authenticate`
accepts that ID and the native Auth bytes, consumes the challenge once (including failed
attempts), verifies the device signature over the unchanged `zoen-sync/2:auth` domain,
and derives the session from current directory rows. Challenges expire after ten seconds;
the private adapter caps pending challenges and sessions at 256 each. Its sessions expire
after fifteen minutes; this bound is not a proposed public WebSocket lifetime.

`request({sessionId,frame})` supports Person Register, DeliverLink, FetchLink and Unlink.
Each request re-reads its active device in the same synchronous SQL transaction as its
writes. Root certification alone cannot link a new device to an existing account. A
registered active sponsor is required; exact active enrollment retries are read-only,
while revoked/conflicting devices cannot be revived or reassigned. Enrollment and its
sealed link box commit together. Boxes preserve the native fifteen-minute expiry,
bounded cleanup and one-time consumption. Successful Register transitions only that
session; other preopened provisional sessions cannot overwrite the new identity.

The Rust policy reuses the native protocol codecs, certificate/auth signature domains,
Person profile normalization/ownership rules and request authorization predicate. The
native PostgreSQL relay calls the same extracted pure functions. The pending self-key
FetchLink exception remains valid after sponsor enrollment assigns the key to an account.
First registration still cannot claim that globally enrolled device for its self identity.

`authorize({sessionId})` returns a request-admission snapshot. It is not an outbound
delivery lease. Returned binary reply bytes are private operation results, not permission
to send them to an external socket. Native delivery holds a durable device fence through
the actual send; this adapter has no sockets and does not implement that fence. An already
admitted native Space append may finish after revocation, while fresh admission and every
future public delivery must reject the revoked device.

The Worker has no HTTP routes, workers.dev or preview URLs. It is not connected to the
closed-Space service. Provider proofs/JWTs, backup recovery, agent registration, profile
fanout, global rate limits, key-package publication/claims and their revocation deletion,
and the complete public session/MLS protocol remain separate gates. In particular this
namespace cannot contain key packages, so its Unlink is not yet a complete replacement
for native device revocation across those tables. No caller may use this slice as a
public authentication facade.

With an existing PostgreSQL test environment, run
`node scripts/verify-cloudflare-directory.mjs --out-dir .dev/cloudflare-directory` from
the repository root. `--skip-build` skips only shared WASM generation; the native oracle
still creates its own fresh, validated PostgreSQL schema, runs real native directory
transactions and removes only that schema on success or ordinary errors. The test bridge
and SQL failure triggers are local fixtures and are absent from the deployment config.
The shared WASM build command is reused from `scripts/build-cloudflare-closed.mjs`; it
does not require PostgreSQL or deploy either Worker.

The private `zoen-relay-directory` Worker was provisioned on 2026-10-19 at version
`759ffee3-0ef9-4ff5-a53b-1c971a3e2197`, from source commit `46128a65`. It has no
public route, workers.dev or preview URL. The 33 PostgreSQL scenarios and nine
workerd cases are local validation; no remote Directory journey was run in this
checkpoint.
