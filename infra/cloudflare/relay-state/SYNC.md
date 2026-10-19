# Shared catch-up visibility

The native relay and Workers/WASM now use the same Rust catch-up policy in
`roda_relay_admission::sync`. This implements planning only. It does not expose a
public socket or make the private Cloudflare log a complete encrypted relay.

For each requested cursor, current members read from that sequence to the head.
A removed member reads only up to and including the stored removal sequence. A
cursor after removal and an unknown Space produce no read. Rejoining restores
current-member visibility. `all` appends current Spaces absent from the requested
cursors, from sequence zero. Requested order and repeated cursors are preserved.
Membership lookup and all-Space inclusion use ordered sets instead of scanning
the entire membership list for every cursor.

The native session delegates those decisions to `cursor_read` and
`include_current`. Its database deadlines, failure handling, paginated reads,
receipt replay, final device revocation fence and `SyncDone` behavior remain in
the native session.

## Portable boundary

`planSync(frame, contextJson)` takes a bounded canonical protocol-4 `Sync` frame
and trusted facts for the authenticated session. The context has this shape:

```json
{
  "session": {
    "authenticated_identity": "<64-character identity key>",
    "authenticated_device": "<64-character device key>"
  },
  "facts": {
    "identity": "<same authenticated identity>",
    "current_spaces": ["sp_current"],
    "removed_at": {"sp_removed": "8"}
  }
}
```

The host must derive the session from Directory-owned authentication and read
these membership facts from authoritative committed Space state. Public clients
must never supply the context. Rust rejects an identity mismatch, unknown fields,
noncanonical binary frames and invalid or rounded UInt64 removal values.
The result is an array of `{space, from, through}` reads. `from` and non-null
`through` are exact decimal UInt64 strings. `through: null` requests current
member history through the head.

Every outgoing event still needs the current durable device-authorization check.
Planning does not grant a delivery lease or waive Space membership rules. The
Cloudflare identity-to-Space index, atomic membership projection updates, MLS
projection, bounded log-read orchestration, receipt replay and socket fanout
remain integration work.

## Verification

From the repository root, run:

```sh
cargo test --locked -p roda-relay-admission --test sync
node scripts/verify-cloudflare-sync.mjs
```

The verifier builds shared Rust/WASM, generates eight native protobuf vectors
and compares their plans in Node and local workerd. It also checks that another
identity's membership facts are refused. An isolated output directory retains
the exact fixture, JavaScript, WASM and an acceptance receipt with their hashes.
`--skip-build` uses the existing generated WASM while retaining the tested bytes.
These checks do not establish public authentication or encrypted-history sync.
