# Private encrypted Directory profiles

The existing Directory transaction domain now handles `PublishAgreementKey`,
`AgreementKeys`, `PutProfile` and `GetProfiles`. It stores only agreement public
keys, encrypted profile bytes and their certified-device proofs. Person display
names and biographies stay encrypted; the separate public handle registration
contract is unchanged.

The shared policy lives in
`crates/roda-relay-admission/src/directory/sealed_profiles.rs`. Native Session
validation delegates to the same policy. The host never treats caller-supplied
identity, registration state or device authorization as authoritative.

## Admission and persistence

- Resolve the Directory-owned session and freshly authorize its current device.
  Provisional sessions must register before reading or writing encrypted profiles.
- Before SQL target selection, verify an upload's account, exact session device,
  certified signature, ciphertext limit and version range. The final shared plan
  validates again against the transaction's trusted rows before any write.
- Agreement publication replaces that identity's signed public key. Reads retain
  the native first-500-ID, duplicate-coalescing and missing-row semantics. There is
  no promised input order and no added membership requirement for ciphertext reads.
- A profile may replace the stored row at a greater version. Equal version and
  identical ciphertext is an accepted retry; another active certified device of
  the same account may replace its proof. Equal version with changed ciphertext
  and every older version receive `stale profile version`.
- Versions cross WASM/JavaScript as canonical decimal text and remain SQLite
  `TEXT`; comparisons happen in Rust within the same transaction. No floating
  point or lexicographic version comparison is used.

The native PostgreSQL column is signed `BIGINT`. Both request paths now reject
signed versions greater than `9223372036854775807`, and native persistence also
uses a checked conversion. This prevents a UInt64 upload from wrapping negative
and breaking monotonic replacement. Zero remains allowed, preserving the native
contract. Existing invalid negative database rows, if any, require an explicit
migration decision; this change does not silently reinterpret or repair them.

Ciphertext is bounded at 8 KiB per profile. SQL selection uses `json_each` with one
binding for all requested IDs. Rows are streamed and counted with Base64 and JSON
overhead against the existing 768 KiB private fact budget. The complete Rust
context retains its 1 MiB bound. Oversized aggregates fail retryably with no
partial successful response. Malformed trusted versions/Base64 or mismatched rows
fail closed. These are private adapter capacity bounds, not production scale.

## Reproducible qualification

With a disposable PostgreSQL database, run:

```sh
ZOEN_TEST_PG=postgres://... node scripts/verify-cloudflare-directory.mjs \
  --profiles --out-dir /absolute/fresh/empty/evidence-directory
```

The oracle calls actual native SQL APIs and the existing profile AEAD/signature
implementation. It checks opening ciphertext after SQL roundtrip, rejecting the
wrong key/version, exact versions above JavaScript's safe integer range, changed
retries, device/account binding, size/range boundaries, key replacement, reads and
revocation. The workerd tests compare native protobuf replies and encrypted SQL
state, persist across restarts, race versions and inject late SQL failures before
an exact retry. No private profile keys leave the synthetic oracle.

The verifier binds source files, native fixtures and actual WASM/glue bytes. The
CI has a dedicated profile lane before the existing complete Directory/socket
gate; no failing socket deadline is skipped or weakened.

## Remaining composition

The outer Worker still returns HTTP 404. This does not expose a public profile
service or establish public login/synchronization acceptance. Profile-key sharing,
`ProfileChanged` fanout, Space discovery and per-delivery authorization must still
be composed with public encrypted synchronization. Account recovery, profile
photo/media UI, real provider enrollment, production migrations and final client
journeys retain their separate acceptance requirements. Login with Apple or
Google alone never grants access to previous encrypted profile/history keys.
