# Managed image editor

PhotoCraft 0.11.1 is pinned to
`9b9643764f3fb77d9f0e48d7510983ae50621e11` in [upstream.json](upstream.json).
This implements the bounded image-editor integration tracked by #556 under
#180/#170. It does not establish parity with every upstream editing feature.

The actual `PhotocraftApp`, `Session`, native `.pcraft` reader/writer, brush preview
and PNG exporter run in a managed eframe/WebGL2 module. The headless worker runs
real Session commands in WASIp2 through Zoen's existing signed worker executor.
Neither imports upstream accounts, IndexedDB, collaboration servers, file pickers
or a durable second document store.

## One document authority

The existing App Item/FileDoc owns identity, scope, encryption, versions, mutation
receipts, CAS and access revocation. Manual edits and reviewed agent operations
use the same binary-document transport and existing approval journal. Source
hashes bind bytes; they never grant access. The standardized App card uses the
same host, full-screen presentation and portrait/landscape contract as other Apps.

The browser keeps an ephemeral real Session and Undo history. Snapshot retries
return the original proposal, while a newer stroke remains in the visible
Session after its earlier proposal is acknowledged. Export reads the explicitly
accepted head, including when newer pixels remain unsaved. A rejected transform
restores the entire prior DocState, including Undo, before returning an error.

The public pause retains FIFO admission and lets queued final input reach the UI
before deciding whether freezing is safe. Active gestures/composition are still
refused. A hidden viewport cannot acknowledge a pause while retaining raw input.
On phones and short landscape viewports the upstream right dock starts hidden;
its wide-layout choices are restored when returning to a larger viewport. The
renderer consumes host safe insets and the close-button exclusion once.

## Explicit candidate profile

Canonical save is the native `.pcraft` document. PNG is an explicit flattened
export with free-form text/XMP disabled; it does not replace the source document.
The native MIME is `application/x-photocraft`; PNG is `image/png` / `public.png`.
No invented native UTI is used.

The engine/bridge accepts one current RGB8 raster layer and performs a genuine
load/save round-trip with whole manifest and entry-set equality. Masks, effects,
video, patterns, channels, selection and unknown/future representations are
refused. Original bytes remain unchanged when any input is refused.

Bounds are 8 MiB source/output, 256 KiB manifest, 128 archive entries, 16 MiB
engine-decoded payload, 1,024 pixels per side and 1,048,576 canvas pixels. Tile
coordinates are restricted to -8…8. These are candidate bounds, not measured
peak iPhone memory. The existing isolated agent executor retains its smaller
512 KiB document capacity, 256 MiB memory, five-second execution window and
bounded fuel. Neither browser nor worker widens those existing host budgets.

The Core validator is deliberately structural and has no engine/UI/decoder
dependency. It reuses the bounded ZIP32 preflight, accepts only STORE entries,
rejects duplicate archive/JSON keys, and bounds names, sizes, dimensions, DPI,
64 raster tile references, coordinates and lowercase content-address names. The
container must contain exactly the referenced tiles and manifest. A native writer
seed supplies the closed structural template: only document/layer IDs and names,
canvas dimensions, DPI, raster default and validated tile references may vary.
All remaining properties must match the known template. Richer metadata and
features are rejected instead of normalized or flattened.

Core does **not** decompress, decode or verify the content hash of pixel blobs.
It preserves accepted source bytes, outer SHA-256, scope and versions. Complete
format semantics and pixel fidelity belong to the pinned engine/bridge and its
runtime proofs. A structural pass must never be reported as pixel validation.

Agent operations are five parameterless upstream transforms: horizontal/vertical
flip, clockwise/counterclockwise 90° rotation and 180° rotation. Commands require
an exact observed version/hash and owner review. No arbitrary commands, paths,
URLs or external assets are accepted. Browser brush input is bounded before live
preview and command execution: finite in-canvas points, 4,096 points at most,
round brushes up to 128 pixels, and no external texture/bitmap/dual brush.

## Reusable producers and deployment

`prepare-engine.py` requires a clean owned upstream checkout at the exact pin.
It copies the managed bridge and optionally the actual browser shell, applies
reviewed engine/UI patches, and records exact source inputs. It reuses the Grid /
Word input foundation and shared ABI/layout code; it never patches a Cargo cache.
Use Python 3.11+ and explicit official eframe/egui 0.36.2 crate archives for
`--browser`. Cargo generates the real lockfile in the private tree.

Build the browser crate for `wasm32-unknown-unknown` using the upstream
`wasm-release` profile, then wasm-bindgen 0.2.129 with `--target web` and
`--remove-name-section`. Build the headless host binary for `wasm32-wasip2` with
the same declared profile. Do not substitute a native bridge or metadata graph
for either actual runtime.

`build-bootstrap.mjs ABS_OUTPUT` uses the shared official MCP bootstrap producer.
`build-resource-manifest.mjs RUNTIME_JSON SEALED_ASSETS OUTPUT_JSON` delegates to
the common resource producer, verifies real asset bytes/digests and binds the
configured resource to those immutable files. Runtime limits remain 512 KiB JS,
48 MiB WASM and 8 MiB HTML. Registration additionally checks exact reviewed
module/WASM pins, engine revision, same-origin HTTPS paths and zero egress/device
permissions. The checked-in manifest has no runtime and leaves discovery off.

Deployments explicitly provide `ZOEN_PHOTOCRAFT_RESOURCE_MANIFEST_JSON`. Headless
execution separately requires all of `ZOEN_PHOTOCRAFT_WORKER_COMPONENT`,
`ZOEN_PHOTOCRAFT_WORKER_MANIFEST` and `ZOEN_PHOTOCRAFT_WORKER_PUBLISHER`; partial
configuration is refused. Test qualification URLs are not a live deployment.

## Evidence and remaining acceptance

Actual native qualification passed 12 bridge/engine cases and six PhotoApp/input
cases, with all-target pedantic/performance Clippy. Core qualification passed
1,028 client-core and 53 miniapps tests without skips, including configured real
resource creation, scoped approvals, retries, lost ACK, cold SQLite reopen,
selected history, encrypted chunks, rollback and revocation. Host TypeScript
checks passed separately.

The actual signed WASIp2 worker passed 12 cases: correct production dispatch,
foreign publisher/revision/code/engine refusal, native source inspection,
byte-for-byte native/WASI transform equality, fresh-worker retry, source hash and
scope substitution refusal, arbitrary-command refusal and fresh-component reopen.
Synthetic supplied scope in this component consumer is not a certified model turn.

The browser test runs the genuine MCP bootstrap, renderer, shared session and
export controller with a clearly synthetic Core read/CAS port. Independent PNG
checks decode the actual exports. It covers trusted pointer input, same-event
pause, newer input during delayed ACK, rotation without a document write, export,
owner retirement and fresh-renderer reopen. Chromium and WebKit evidence must be
reported separately; a browser run is not native iPhone acceptance.

#556 remains open for final package/browser and iPhone acceptance, shared-host
lifecycle qualification, measured memory/usability, broader lossless profiles and
reviewed PNG import. Do not claim complete upstream editing parity or a live
service from this integration.

## Provenance

Source is MIT OR Apache-2.0; trademarks are separately reserved. Retain upstream
NOTICE/ATTRIBUTION, engine/adapter licenses, bundled font/icon/dictionary/model
notices, and the official bootstrap dependency notices with sealed packages.
No external images, fonts, model weights or branding are added by the adapter.
The native raster fixtures were authored through the actual upstream writer;
their exact origin and limits are recorded beside them.
