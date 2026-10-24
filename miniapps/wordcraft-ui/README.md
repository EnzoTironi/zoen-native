# Managed WordCraft: bounded DOCX worker

Tracks [#502](https://github.com/EnzoTironi/zoen-native/issues/502), under ArtCraft
[#180](https://github.com/EnzoTironi/zoen-native/issues/180). Default builds do not enable
an available App, a native editor, or a PDF exporter.

The adapter uses the real pinned WordCraft Session and DOCX writer. Its binary
framing is the existing Deck/Grid ABI: little-endian 4-byte metadata length,
bounded JSON, then raw document bytes. The response has the same framing and
`{ok, metadata}` or `{ok:false,error}`. No paths, base64, host storage or accounts.

`open` carries the existing `Scope` and `Head`. The single command is
`{command:"text.replace",params:{text}}`, under the existing `Mutation` envelope.
It translates to upstream `select.all` and raw `text.insert`; it is not a generic
command dispatcher. Human and agent callers follow the same proposal boundary.
The accepted document stays unchanged until an exact scope/mutation/next-version/
hash ACK. Retries in the same bridge return the same proposal; the canonical host
still owns authorization, durable idempotence, CAS, FileDoc and cold recovery.
The standalone process emits a proposal and cannot commit it. The normal Zoen worker
resolver recognizes an explicitly configured signed Word component. The reviewed
creation/agent continuation is documented below; default builds still do not
advertise a browser App.

## Deliberately narrow DOCX profile

Only plain-text packages produced by this pinned writer are editable. The ZIP is
bounded to 512 KiB compressed, 32 entries, 1 MiB per entry and 4 MiB total actual
inflated bytes. Names cannot traverse; duplicate entries are refused. Count and
declared sizes are checked before inflation; actual reads are bounded too. Text
is at most 32 KiB UTF-8; LF and tab are supported, other controls are refused.

The upstream DOCX reader is lenient and can retain macros and opaque parts. This
adapter compares **every uncompressed package part** with the actual writer's
`Document::from_text` package. Rich formatting, metadata, images, macros, extra
parts and ignored XML are refused, including benign imports outside this profile.
Opening never replaces original bytes with a best-effort reserialization.
After the actual Session edit, plain text is encoded into the same declared
plain profile; Session editing metadata is not part of this format. Export of an
accepted head returns its exact original DOCX bytes, including before ACK.

The manual checkpoint captures an immutable proposal while the actual canvas
may continue editing. Only its exact receipt advances the accepted baseline;
newer local input and Undo remain in the same Session for the next checkpoint.
Retries preserve the first proposal's bytes, mutation and base, not the newer
draft. Agent proposals, historical viewing and explicit host pauses still block
manual input. Ordinary close must flush through the existing host before it
destroys the renderer. Keystrokes held only in the WebView are not durable, and
reopening an accepted DOCX does not recover unsaved memory after process death.

## Reproduction and evidence boundaries

Use a separate clean checkout at the existing pin:
`d1b0bfc3f9dd83126b162a42b667facecb003b7f`. Run
`python3 miniapps/wordcraft-ui/prepare-engine.py --upstream ABS_OWNED_CHECKOUT`.
This copies sources and records every input SHA; it neither builds nor downloads.
Run `python3 -m unittest discover -s miniapps/wordcraft-ui/test -p '*_test.py'` for
recipe provenance/refusal checks. In the prepared upstream workspace, genuine
Cargo resolution must update Cargo.lock before a locked source-guarded build.
The package to qualify is `wordcraft-zoen-host` (Clippy all targets, then lib
tests and an actual binary frame run). `--seed` emits the actual upstream writer's
blank DOCX. Never substitute a fabricated ZIP for an executed writer fixture.

Official WordCraft 0.9.0 has a real browser WASM release. Its release commit is
`2e6db0bd88795e80375176c01665cff7dbe2b057`; the selected pin differs only in README
and `.coderabbit.yaml`. Inspected release ZIP SHA-256:
`710d2f4dd95ac6234a043d15d8e82232d20330199dbdbc94159324eeb02631c1`.
Actual WASM: 43,967,642 bytes, SHA-256
`fe8e18258a33eebd4b32d311e1651ebd817bae18469599a6f16feb3403389ad5`.
This proves availability, not execution or a managed package. That release is
not copied into Zoen resources. Its size and full fonts/assets need a managed
package audit. Browser WASM is not evidence of WASIp2 support or native keyboard
behavior. The WASIp2 qualification below uses separately built components.

## Genuine WASIp2 components

For a separate clean pinned upstream checkout, add `--wasi` to the preparation
command. The two recorded patches select the existing native clock bodies on
`target_os="wasi"` in engine, formats, layout and PDF. Browser clock/dependency
branches keep their `js-sys` implementation. No clock value is fabricated and no
rendering, thread, file or import permission is enabled. Native-only unused
helpers are cfg-gated rather than suppressed; existing helper tests remain.

Generate the real lockfile in the prepared tree, then qualify
`wordcraft-zoen-host` with native all-target Clippy/lib tests and WASIp2 bin
Clippy. Build both `wordcraft-zoen-host` and `wordcraft-clock-probe` with
`--target wasm32-wasip2 --release --features qualification`. The optional probe
runs the actual engine clock, layout monotonic clock and ODT writer's revision
clock branch. It is a separate qualification binary, not a product export or
worker command. The linked WASIp2 graph has no `js-sys`, `wasm-bindgen`, egui,
eframe, wgpu, glow, winit or rfd. CPU layout/font/PDF dependencies remain.

The real Zoen consumer is `crates/zoen-agentd/examples/wordcraft_component_check.rs`:

```sh
cargo clippy -p zoen-agentd --example wordcraft_component_check -- -D warnings
cargo build -p zoen-agentd --example wordcraft_component_check
ABS_CHECKER ABS_WORKER_COMPONENT ABS_CLOCK_COMPONENT ABS_WRITER_SEED ABS_NEW_OUTPUT
```

All four paths are absolute. The seed must come from the upstream writer's real
`--seed` execution. The consumer checks signed publisher/hash refusal, uses the
existing `WasmTier`, and supplies no filesystem, environment or network to the
components. It compares wall timestamps against actual host UTC, requires the
monotonic clock to progress, then exercises inspect, edit, exact retry, cold
reopen and unknown-command/foreign-scope/stale-head/bad-hash refusals. Each run
retains the existing 5-second, 2-billion-fuel, 256-MiB limits. Its signing key and
Scope are explicitly synthetic; they do not establish canonical authorization.
The new output directory contains the actual proposal DOCX and receipt.

Executed evidence (external artifacts, not repository binaries):

| Qualification | Exact source / result |
| --- | --- |
| WASIp2 producer | Zoen `f1c9c8d13a08ce05591882813927a7a2e3f18f02`, prepared `5b374c574ef8a57b136de55c30a6c1f6f502f00b`; native and target Clippy, 6 real engine regressions, release components all PASS |
| Real WasmTier consumer | Zoen `917e5ee057e2c1caf5f25bc17cc1b41f58dc95fe`; all 11 producer inputs unchanged; Clippy/build and 3 clocks + 8 framed calls PASS |
| Worker component | 30,959,770 bytes; SHA-256 `72b2235b7d794fea72b3eb4589e18b67e19a6cdd271d2bcf2eeef8b658ec85d5` |
| Probe component | 30,331,861 bytes; SHA-256 `3737f8d10921f580834260a5fda5c5c66368ff65d205d2b43bdcfb02cc8ba14e` |
| Runtime receipt | SHA-256 `763f6f46b3b9c266d3565d5dd20b4fdbb2a9921a3c2a5b39960173f46c8e6bbe` |

The first WASI Clippy attempt failed on four upstream target-specific unused/
return diagnostics; the second patch addresses them without `allow`. That RED
and the native first-slice diagnostics remain retained. Qualification did not
execute PDF export or prove every upstream command compatible.

Cold JIT is a material limitation: the normal development-built WasmTier consumer
with one compiler thread took **196.400 seconds** to load the worker; both loads
and all execution took 392.284 seconds. Per-call receipts report 0–6 ms and at
most 112,733,774 fuel, excluding component compilation. This is not a startup
latency guarantee. Retain and prewarm a verified `WasmTool` in the existing tier
for a deployment, reusing it for fresh per-request stores; do not compile for each
request. No cache/prewarm implementation or unverified AOT deserialization is
introduced here. App availability still requires a canonical host/resource and
browser integration with deployment readiness, permissions and recovery proof.

The canonical plain-DOCX Format/Core/worker continuation and its actual signed
WASIp2 -> encrypted FileDoc -> CAS -> cold original ACK/history proof are recorded
in [CANONICAL-EVIDENCE.md](CANONICAL-EVIDENCE.md). This includes a serializer-loss
RED and its strict XML/round-trip correction; the old sealed components above
remain preserved.

The real managed canvas/browser continuation and its retained REDs are recorded
in [BROWSER-EVIDENCE.md](BROWSER-EVIDENCE.md). Browser resource registration,
canonical creation/certified-agent action approval, native
open/edit/save/cold recovery/export, accessibility and orientation. Keep #502 and
#180 open until their actual journey criteria pass.

## Creation and reviewed agent operations

The normal binary factory/agent consumer now recognizes `create_wordcraft`,
`read_wordcraft` and `wordcraft_text_replace`. Creation uses the existing signed
review of the exact title/seed and editing binds the observed version/hash to the
existing approved-turn worker and FileDoc CAS. Replacement text is capped at
16 KiB UTF-8 to stay within the original signed tool argument envelope; the
managed DOCX profile still permits up to 32 KiB of text for ordinary host saves.
No native availability is implied: the default manifest has no runtime package.
An explicit `ZOEN_WORDCRAFT_RESOURCE_MANIFEST_JSON` must match package03's exact
engine/ABI/JS/WASM and generated HTML. `build-resource-manifest.mjs` verifies
actual sealed local assets independently of Rust pins. It does not deploy them.

Actual qualification for this consumer is recorded in [AGENT-EVIDENCE.md](AGENT-EVIDENCE.md).
Accepted-head DOCX/PDF export, the actual pinned font/layout path, retained
missing-glyph RED and independently inspected current/historical outputs are
recorded in [PDF-EVIDENCE.md](PDF-EVIDENCE.md). The configured package exposes
both formats; the default manifest remains unavailable. Unsupported font glyphs
refuse PDF without changing the DOCX or pending draft.
