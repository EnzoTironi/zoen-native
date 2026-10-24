# GridCraft managed editor — integration in progress

Issue [#445](https://github.com/EnzoTironi/zoen-native/issues/445), under #180.
Pinned engine/UI: `storytold/gridcraft` **b8771d51fb4da60bd48c13cf313d6d6014dc5e66**
(version 0.11.1, MIT OR Apache-2.0). Upstream branding is excluded by its NOTICE;
no brand assets or upstream accounts, storage or collaboration are imported.

The concrete XLSX profile and Core use the existing encrypted FileDoc/chunk,
Item grant, version/CAS and exact receipt paths. `AppBinaryCreateFor` selects a
fixed trusted format; the existing Deck command/wire/resource remain compatible.
No second document database or local accepted-version counter is introduced.
The shared JavaScript host defaults to Deck and explicitly selects Grid only
through its trusted descriptor. Grid remains unavailable in the endpoint-neutral default manifest. Only a build
with the exact configured HTML/JS/WASM resource manifest may advertise it; native
asset delivery and iPhone acceptance remain separate requirements.

`host-bridge/` runs the actual pinned GridCraft `Session`, `cell.set`, calculation,
undo and native XLSX writer. It uses the existing Deck scope/head/proposal/accept
contract. A manual checkpoint keeps its original proposal immutable while newer
manual input/Undo stays in the same Session. Its exact next-version/hash receipt
advances only the accepted baseline; newer work requires a later checkpoint.
Pending agent proposals, history and explicit host pause still freeze input.
Export returns the selected accepted native XLSX bytes,
including after cold open, rather than a newer unsaved draft. No general command,
filesystem, URI or automation API is exposed to agents. The first reviewed
mutation is one bounded `cell.set`; generic upstream commands remain unavailable.
Browser CSP/WASI capabilities and Core authority are still required independently.
The adapter is never proof that an actor has a grant.

`prepare-engine.py` copies this adapter and the **single canonical XLSX profile**
from `crates/miniapps/src/gridcraft{.rs,/}` into a clean owned upstream checkout.
It applies the reviewed embedding command, host export and drag-history patches, checks the exact revision,
refuses dirty/adopted trees, records source hashes, and does not compile, fetch,
install, enable an App or claim a pass. Example preparation:

```sh
python3 miniapps/gridcraft-ui/prepare-engine.py --upstream /absolute/owned/gridcraft

# Browser preparation also applies pinned same-version egui/eframe input patches.
# Use Python 3.12+ and genuine registry archives (the recipe verifies their hashes).
python3.12 miniapps/gridcraft-ui/prepare-engine.py --upstream /absolute/owned/gridcraft-web \
  --browser --eframe-crate /absolute/cache/eframe-0.36.2.crate \
  --egui-crate /absolute/cache/egui-0.36.2.crate
```

After a coordinated compiler lease, generate the lockfile genuinely in that tree,
then qualify with one job, incremental disabled, debug info disabled and the
normal source/resource guard. Initial executable commands are:

```sh
cargo test --locked --offline -p gridcraft-zoen-host -- --nocapture
cargo test --locked --offline -p gridcraft-zoen-host --features ui-tests -- --nocapture
cargo clippy --locked --offline -p gridcraft-zoen-host --all-targets -- -D warnings
cargo run --locked --offline -p gridcraft-zoen-host -- --seed > /absolute/owned/empty.xlsx
```

Qualified on the pinned native engine: 7 profile cases, 14 Core binary cases
(including 10 existing Deck cases), 10 native engine/profile cases, 2 actual
egui Text+Enter input cases, all-target Clippy for the affected default/UI and
Core client-types graphs, and a genuinely compiled 126-file type exporter.
The UI test first failed with a hidden pre-ACK draft (9 became 99 after ACK);
`begin_edit` now refuses while the proposal is frozen, and the same test passes
with an additional no-editor assertion. These are native synthetic inputs, not
WebView/iPhone keyboard or certified-agent admission evidence.

The native tests exercise real engine calculation, human-scoped command and
manual checkpoint convergence, exact repeat, receipt refusal, undo, cold reopen
and accepted-version XLSX export. The independent standards-
shaped XLSX fixtures in Core tests are adversarial storage fixtures, not engine-
generated seeds. Generate any shipped seed with the actual pinned writer and
record its hash; do not substitute those fixtures for it.

Initial limits: 8 MiB compressed, 16 MiB inflated, 4 MiB per XML part, 128 ZIP
entries, 16 sheets, 40,000 stored cells, 10,000 rows and 256 columns. The headless
executor is deliberately smaller: 512 KiB native document/proposal and 64 KiB
metadata. Macro/external/unknown package parts, malformed references, XML DTDs,
external formulas and unsupported exports refuse explicitly. This is a restricted
managed profile, not a general Excel importer or a claim of upstream parity.

The upstream `?host=parent` bridge is replaced with a managed eframe/Glow shell
and the same official MCP App bootstrap/session/export code used by Deck. The
actual canvas is inert until Core opens a canonical snapshot; a pause, unresolved
proposal or retired owner blocks mutations. Share emits a host export intent;
it does not falsely mark an unacknowledged draft saved. No App storage is the
sole accepted copy. The host owns document versions and selected-version export.

Build the engine in the prepared owned checkout (and qualify the resulting bytes
under the source guard), then use `wasm-bindgen` 0.2.129 for the wasm32 release
module. Build the Root bootstrap with an absolute private output directory:

```sh
node miniapps/gridcraft-ui/build-bootstrap.mjs /absolute/owned/bootstrap --install-resource
```

The bootstrap builder emits exact HTML metadata and third-party JavaScript
notices. It deliberately writes an endpoint-neutral default manifest. Deployment
must supply `ZOEN_GRIDCRAFT_RESOURCE_MANIFEST_JSON` with the exact qualified HTML,
module, WASM hashes/byte lengths and HTTPS locations. A changed manifest is a
native compilation input; it must not reuse another deployment's native artifact.
`build-resource-manifest.mjs VERIFIED_RUNTIME_JSON SEALED_ASSET_DIRECTORY OUTPUT_JSON`
uses the shared runtime descriptor parser, verifies the actual sealed JS/WASM byte
lengths and digests, then binds them to this checkout's exact generated Grid HTML.
It refuses a missing, symlinked, wrong-sized or changed asset before writing output;
it does not read Rust pins, publish assets, rebuild WASM or enable the default profile.
Package the compiled graph's notices as described in `NOTICE.md`, including the
actual embedded font alternatives. Do not infer a license review from compilation.

Actual browser fixtures use the official generated bootstrap and pinned engine
inside an opaque sandbox with no guest network authority. They cover canvas
keyboard edits, calculation, pending proposal/ACK refusal, pause, exact cold
reopen, accepted-byte XLSX export and retirement. Their Core bridge is explicitly
synthetic; these results do not prove certified-agent or iPhone behavior. The
shared Deck path is independently checked with its original pinned engine and
actual PPTX/PDF exports.

The headless executable is a WASI-p2 component from `gridcraft-zoen-host`, selected
only by fixed App identity and an operator-configured trusted publisher signature.
The existing agent-host Gate stages the same typed owner review; the Core binds
its exact worker, tool, current actor/device/source, input version/hash and output
CAS receipt. Grid exposes only `read_gridcraft` and bounded `gridcraft_cell_set`.
No model-selected executable, URI, actor or arbitrary upstream command is exposed.
Its optional query returns one exact cell and bounded workbook metadata, not an
unbounded claim of complete workbook contents. The original Deck path is retained.

Operator configuration is explicit and all-or-none:
`ZOEN_GRIDCRAFT_WORKER_COMPONENT`, `ZOEN_GRIDCRAFT_WORKER_MANIFEST`, and
`ZOEN_GRIDCRAFT_WORKER_PUBLISHER`. The signature is verified before loading the
actual component. Network/filesystem/secrets capabilities are not granted. The
component has the existing 256 MiB / 2 billion fuel / 5 second envelope and the
smaller document/metadata limits above; an oversized document refuses explicitly.

Native iPhone manual/agent editing, keyboard/orientation/accessibility, memory,
restart recovery and chosen-version sharing remain pending until tested on the
actual built App. XLSX is the only exposed Grid export. Session undo is local to
the editor lifetime; durable Core version history does not promise a global
cross-device undo stack. Issue445 remains open until its complete acceptance.

A real pointer regression also confirmed that a refused column resize could
coalesce two already accepted cell undo entries. Drag paths now coalesce only
successful commands; the same actual egui keyboard/pointer regression and all
16 host/UI cases pass, with affected all-target Clippy. This does not enable
resizing or object mutation in the restricted profile.

The first browser package had a reproduced mobile-touch defect: focus was deferred
to RAF and could use the preceding editor geometry. The pinned successor applies
`mobile-input/` after the existing host sources and before the refused-drag undo
patch. Its current primary contact must still own the document, sheet, canonical
cell and field; focus reads actual rendered TextEdit geometry once. Canceled,
retired, frozen, wrong-owner or stale contacts cannot acquire focus. The same-version
eframe/egui source overrides use verified registry archives; no registry cache is
modified, no global focus bypass is enabled, and Cargo generates its lockfile normally.

The successor source `6d05c845` passed host/WASM Clippy, 13 real UI regressions,
release WASM and two actual trusted-touch browser runs. Both browser runs proved
one synchronous focus with current geometry, four refused focus inputs and exact
accepted XLSX values A1=11/B2=22. The public preparation recipe reconstructs all
163 qualified input files exactly. The retired first JS/WASM package no longer
satisfies the resource pins; it remains immutable at its old URL. These proofs
cover the source and Chromium touch contract, not the iPhone software keyboard.
The endpoint-neutral default still does not enable Grid; actual native resource
delivery, iPhone/agent/CAS interaction and complete issue445 acceptance stay open.

The short embedded editing viewport has a separate, transient render policy. It
compares the actual safe egui canvas against the requested ribbon/formula/status/
sheet chrome plus a worksheet interaction row. While a real cell editor is active
and that content cannot fit, the current TextEdit is rendered in the formula row
with its SAME widget ID, buffer, cursor and IME owner. Cancel/Enter keep their
existing paths. The Commands popup uses the existing catalog/run policy and the
explicit managed Share intent in a viewport-constrained virtualized list. It
does not auto-focus another search input, alter saved ribbon/formula settings,
create a different editor or infer a keyboard from orientation. The ordinary
layout returns when the real available space fits or the edit finishes.

`patches/0005-compact-retained-editor.patch` applies after0004 only in the browser/mobile
preparation; ordinary headless preparation keeps its original renderer. The real egui
regressions are in `host-bridge/src/compact_ui_tests.rs`. Real pinned egui tests and the genuine release WASM passed, with a byte-exact
fresh normal preparation recorded in [issue445](https://github.com/EnzoTironi/zoen-native/issues/445).
This is not iPhone landscape editing acceptance, nor proof of why the retained499
native host-height samples differ.
