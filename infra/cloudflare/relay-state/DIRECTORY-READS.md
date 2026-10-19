# Private Directory reads

The current source adds Person `Lookup`, `Profiles` and same-account `Devices`
operations to the existing Directory transaction domain. This is a local candidate
with native PostgreSQL/workerd differential evidence; it has not replaced the
private deployed version recorded in [DIRECTORY.md](DIRECTORY.md).

Shared Rust owns query preparation, profile ownership filtering and native binary
replies. Native PostgreSQL uses the same normalization, first-500-ID bound and
profile validation functions. The Worker performs SQL reads against the existing
identity/device tables and never creates a second account database.

`Lookup` trims whitespace and leading `@` characters and lowercases in Rust.
An empty handle returns no profiles. Exact lookup uses the unique stored handle.
Prefix lookup preserves the native behavior: remove percent signs and backslashes,
treat underscores literally, order by handle and return at most twenty rows.
`Profiles` considers only the first 500 supplied IDs. Storage returns each existing
identity once, ignores missing IDs and does not promise request order. The Worker
uses one SQLite JSON-table binding, preserving native `ANY` behavior without
exceeding the runtime's bound-variable ceiling.

Both paths apply the native ownership filter: a stored profile must match its
authoritative identity, kind and owner, and an agent requires its valid ownership
proof. This private Directory still admits Person registrations only; it supplies
that authoritative kind rather than trusting the profile payload to select it.
Agent enrollment remains unsupported. Signed/encrypted profile operations now
have separate [native SQL/AEAD and private host candidate coverage](DIRECTORY-PROFILES.md); these read
checks do not accept their public integration or profile-key recovery.

Selected rows are streamed from SQLite with a private aggregate facts budget of
768 KiB, including query, row fields and JSON punctuation. Oversized profile or
device inventories receive a retryable `private_directory_read_capacity` refusal;
no truncated success is returned. The shared Rust one-MiB complete trusted-context
limit remains the final backstop. This explicit private capacity bound is not
full native response-capacity parity.

`Devices` returns only the authenticated account's devices, including revoked rows.
Its two-field records remain `{device, revoked}`. Native PostgreSQL orders by
`created_at`. Existing private SQLite rows have no creation timestamps, so the
candidate preserves their enrollment order through stable rowids. It does not
invent historical wall-clock values or claim ordering equivalence after arbitrary
historical clock corrections.

Requests require current device authorization and registration. A preflight
transaction spends the native default account lookup budget, shared with
`DeliverLink` and `ClaimKeyPackages`: sixty requests of burst capacity, refilling one request per second.
Empty lookups and failed link delivery still spend admitted capacity. Reconnecting
or using another device does not reset the account's durable quota. Its wall-clock
basis is clamped to the last observed value; backward clock movement cannot refill
it. Native custom `ZOEN_LIMITS` settings and the complete global abuse-control
matrix remain separate integration work.

Shared Rust classifies cost before operation-specific validation. Active-device
authorization and registration precede the charge; invalid claim operation IDs,
noncanonical targets and oversized receipt-mode target sets still spend admitted
capacity. Only afterward does Rust construct the bounded target lookup. Invalid
claims never reach target SQL or consume stock/receipts. The earlier `e2a002cd`
checkpoint did not include claim charges; its evidence remains historical.

The actual directory mutation still runs in its own atomic transaction and
rechecks authorization. Failed durable work cannot refund already admitted lookup
capacity. This request admission is not permission to bypass the existing final
outbound device fence.

## Verification

With an isolated PostgreSQL test environment, run from the repository root:

```sh
cargo test --locked -p roda-relay-admission --test directory_reads
node scripts/verify-cloudflare-directory.mjs --reads --out-dir .dev/cloudflare-directory-reads
```

`--skip-build` reuses the generated shared WASM; the native oracle still runs against
real PostgreSQL in its own validated disposable schema. The original 71 enrollment,
linking and key-package scenarios remain unchanged. The additional timeline uses
25 normally registered synthetic accounts and check native lookup/profile/device
results, first-500 truncation, 500 distinct missing IDs, own-device revocation and
unregistered/revoked callers. Workerd also verifies restart, shared-device quota,
failed-link charging, rejected-claim charging and clock rollback. A separate trusted-storage fault test
checks aggregate large-glyph and device-inventory refusals; it does not claim real
native enrollment of fabricated inventory. Evidence retains exact fixture and WASM
bytes, source hashes and TAP output.

These checks do not accept a public relay, provider login, encrypted Space sync,
public signed-profile integration or the two separately tracked local socket-close gates.
