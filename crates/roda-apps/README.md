# Portable mini-app domain

`roda-apps` owns Zoen's built-in app catalog, MCP resource bundles, creation and
mutation rules, schedule conflict resolution, and viewer privacy projections.
Both `roda-ffi` and the browser core call this implementation. It has no database,
network connection, device keys, UniFFI, or platform UI dependency.

Set the host's language with `roda_i18n::set` before a domain call. Native engine
locks and browser worker requests provide that context. Pass the authenticated
member identity through `Actor` for `create_as` and `apply_as`; use `view_for` with
the viewer's member ID when producing UI state, especially for private schedule
bookings. The host still checks membership, grants, expected versions, and writes
signed events. `Applied::effect` describes effects the authorized host must apply
atomically with the mini-app version.

The public catalog entry points are `specs`, `spec`, `spec_for_start_tool`, and
`spec_for_uri`. Other shared projections include `headline`, `snapshot`,
`last_action`, and `call_tool_result`. Schedule integration uses `pin_client`,
`replayed`, `reconcile_concurrent`, and `owner_note_for`.

Build HTML apps with `node tools/miniapp-build/build.mjs miniapps/<app>` from the
repository root. Output and SHA-256 manifests go to this crate's `resources/`
directory. `cargo test -p roda-apps` checks catalog/manifest identity and all
schedule privacy and concurrent-booking regression scenarios.
