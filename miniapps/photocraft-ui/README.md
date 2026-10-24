# PhotoCraft integration foundation

Source: PhotoCraft 0.11.1, revision
`9b9643764f3fb77d9f0e48d7510983ae50621e11`, as recorded in
[upstream.json](upstream.json). Tracks #556 under #180/#170. No App registration or
availability change is included.

The source contains a real eframe application at `apps/photocraft-web`: its target
is `wasm32-unknown-unknown`, WebGPU with WebGL2 fallback. It is a standalone app,
not an existing managed MCP App ABI. Its file pickers, downloads, IndexedDB and
external font loader are not Zoen authority. Browser startup, touch/IME, memory,
managed Session, binary transfer and immutable asset packaging still need work.
This foundation does not establish WASIp2 compatibility.

## Current byte adapter

`host-bridge` uses the actual `photocraft-engine::Session::execute`, native format
reader/writer and PNG exporter. No UI, GPU or automation-server dependency is
added to the adapter's direct roots. Genuine Cargo metadata resolved 459 packages
across platforms with no eframe/egui/wgpu/UI/automation package reachable from
this bridge; native arm64 execution was qualified separately below. This is not
a sandbox or a complete audit of transitive code capabilities.

The upstream desktop association names `.pcraft` as `application/x-photocraft`;
PNG export is `image/png`. No Core Format/catalog is registered by this patch.

The candidate `raster-canvas-v1` profile accepts a current native `.pcraft` bundle
with one RGB/8 raster layer. Masks, video, effects, patterns, channels and selection
are refused. Unknown fields, unsupported archive entries, duplicate names, format
migration or a changed manifest/entry set after loading and saving are refused.
This deliberately rejects valid richer upstream documents instead of silently
flattening them. The original caller-owned source is never rewritten on refusal.

Limits: 8 MiB source/output, 256 KiB manifest, 128 archive entries, 16 MiB decoded
payloads, 1,024 pixels per canvas side and 1,048,576 canvas pixels. Raster tile
coordinates must be within -8…8 before decoding or command execution. These are
conservative candidate bounds, not measurements of peak iPhone memory or CPU.

Only five parameterless upstream canvas operations are exposed: horizontal and
vertical flip, clockwise/counterclockwise 90° and 180° rotation. The Session also
has a closed authorization callback; arbitrary commands, paths, batch, plugins,
network, fonts and filesystem commands cannot be requested through this bridge.
PNG is a separate, explicit flattened export with XMP/free-form text disabled;
canonical save remains `.pcraft` with its original native identity and model.

The expected SHA-256 only binds bytes to a selected source. It does not confer
permission. Integration must supply it from the existing authorized App/FileDoc
projection and persist results with the existing scoped CAS/mutation receipt.
This library adds no store, account, draft journal, approval or collaboration
service. Durable history/undo remains owned by Zoen; an ephemeral engine Session
must not be advertised as the durable copy.

## Preparation and checks

Use Python 3.11+ and a separate clean owned checkout at the exact upstream pin:

```sh
python3 miniapps/photocraft-ui/prepare-engine.py --upstream /absolute/owned/photocraft
python3 -m unittest discover -s miniapps/photocraft-ui/tests
```

Preparation is offline and records exact source/adapter hashes. It adds only
`crates/zoen-host` and `zoen-photocraft-source.json`; it does not modify upstream
engine/UI/format, compile, generate a fabricated lockfile or activate an App.
Cargo must generate the genuine lockfile in that private tree when a compiler
window is available. Run Clippy pedantic/performance and actual host tests with
normal resource limits before claiming the adapter executes. The Rust tests call
the real writer, Session operations, saved-byte reopen and codec pixel reader;
they do not substitute a fake engine.

Native arm64 qualification on the pinned source passed eight actual in-memory
adapter regressions and all-target Clippy with pedantic/performance and
`-D warnings`. Six Python preparation/refusal tests passed separately. The native
cases cover renderer PNG pixels, sparse tiles, original source immutability,
reopening saved bytes, repeated rotation, fractional metadata, closed commands
and bounded/unknown/duplicate archive refusal. This proves neither a process
restart nor canonical persistence/permission enforcement. No browser or WASI
execution was performed.

A real regression found that converting the typed manifest directly to a JSON
Value widened f32 metadata and refused the native writer's fractional DPI. The
schema gate now reserializes through the same JSON path as that writer, while
retaining whole-model equality and the unknown-field refusals. Earlier compiler,
Clippy and fixture failures remain in the external qualification receipts.

Next: genuine WASM qualification, PNG import as an explicitly reviewed copy,
shared MCP browser Session, approved agent edit on the same canonical item,
manual save/ACK recovery, selected-version preview/export and native acceptance.
The full checklist remains in #556; source preparation is not its completion.
