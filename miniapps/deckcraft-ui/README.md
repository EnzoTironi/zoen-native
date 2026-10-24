# DeckCraft managed code package

This directory preserves a reproducible source boundary for DeckCraft 0.9.0,
revision `a31b990d32f2c8562f1588562f922960c3fde48b`. The exact three patches add a
bounded host bridge over the real engine Session and an explicit `zoen-host`
startup ABI, with owner-queue freeze during final snapshot/commit. Upstream code is MIT OR Apache-2.0; see NOTICE.md and licenses/.

The code package is separate from the small `text/html;profile=mcp-app` resource.
The existing 8 MiB HTML/document allowances are unchanged. It is **not listed
as a usable App**: a managed release, the canonical binary adapter, an actual
anonymous code-asset endpoint and iOS runtime qualification must all exist.
No deployment URL is checked into the source and this build does not publish.

## Build

Use Node 26+ for the build audit and Rust/Cargo 1.99.1 with the `wasm32-unknown-unknown` target, Trunk 0.22.0,
wasm-bindgen 0.2.129 and Binaryen 133. Supply an existing local upstream Git
checkout containing the pinned object and the local craft-fonts inputs from
revision `43913056ce82a3fdad0ef69f6b88d1dc922a8104`. Font hashes and sizes are
verified; only the selected fonts and their licenses enter the build.

```
python3 miniapps/deckcraft-ui/build.py \
  --upstream /absolute/pinned-deckcraft-checkout \
  --fonts /absolute/craft-fonts \
  --work /absolute/new-private-build-directory \
  --asset-base-url "$EXPLICIT_CODE_ASSET_BASE_URL" \
  --trunk /absolute/trunk --wasm-opt /absolute/wasm-opt
```

The optional base URL must end in `/`; the generated metadata appends the pinned
revision and the actual generated asset filename. HTTPS is required, including local production-like development. No account,
artifact or query data belongs in code URLs. The caller must serve the assets
at those exact URLs with `text/javascript` / `application/wasm`, anonymously,
without redirects and with CORS permitting the trusted embedding parent.
A successful build does not establish endpoint availability. Omit `--asset-base-url` to produce `assets.json` without any URLs; create deployment metadata only after the exact endpoint exists.

`--prepare-only` performs source/patch/lock/font preparation without compiling
or requiring a URL. All builds use a fresh private directory, private target,
one Cargo job, one Binaryen core, offline locked dependencies and a deadline
(default 1800 seconds; maximum 3600). Timeout terminates the process group and
preserves the log/receipt. No shared checkout or target is mutated. An optional `--cache-target` copies an inactive cache into the private target first (APFS clone on macOS); the source cache stays unchanged. Toolchain
binaries and dependency caches must already be installed; the builder performs
no downloads. Tool versions are fixed; clean-machine byte reproducibility is
not established by this entry point alone.

Trunk's documented `data-cargo-features` option enables `zoen-host` only in the
private archived input. [Trunk Rust asset options](https://trunk-rs.github.io/trunk/guide/assets/index.html).
The builder checks actual JS exports/imports and WASM metadata after release,
then writes `assets.json` (or `runtime-package.json` with an explicit URL), notices and a receipt. Trunk's generated
standalone HTML/favicon are not the MCP resource or a production Zoen asset.

## Host contract

The verified HTML manifest may contain `runtimePackage` with schema 1, engine
`deckcraft`, revision, ABI `zoen-host-v1`, and exact JS/WASM URL, bytes, SHA-256
and MIME. Only `ui://zoen/deckcraft/index.html` may opt in. Its network/device
capabilities and link domains must be empty. JS is at most 512 KiB, WASM at
most 48 MiB; these limits apply only to this optional code package.

Pass the already verified metadata to both
`sandboxHtml(html, [], manifest.runtimePackage)` and
`<MiniappFrame runtimePackage={manifest.runtimePackage}>`. The generic host
propagates it; a DeckCraft view must additionally use the dedicated canonical
binary tools, never generic `state_json` mutation as a document authority.

The trusted parent performs two fixed anonymous GETs and verifies exact bytes
before delivery. No child URL, headers, body, credentials or document data can
be supplied to that fetcher. Redirects, MIME/length/hash changes, untrusted
sources and late owner deliveries fail closed. Raw WASM is transferred as an
ArrayBuffer inside the WebView/browser, never base64 or a React Native message.
The opaque child retains `connect-src 'none'`; only this opt-in receives blob
module and `wasm-unsafe-eval` script authority. No unchecked remote or data JS
module is enabled. CSP path allowlists are deliberately not used for fetch,
because they do not constrain an attacker-controlled query string.

The small official MCP App client calls `window.__zoenRuntime.load()` lazily.
It imports the verified glue, initializes it with the verified Uint8Array,
checks the managed exports, and calls `zoenHostStart()` exactly once. Managed
Rust `main()` schedules nothing. Real requests remain the bounded host queue;
start is not an engine-ready/render acceptance promise. Page retirement
aborts fetch, rejects pending load, revokes object URLs and retires bindings.
A late WASM initialization can never start a retired engine. There is no
account-global module cache, durable document store or alternate tool transport.

`node miniapps/deckcraft-ui/build-runtime.mjs` regenerates the small trusted
parent/child support script; `--check` verifies its exact committed bytes and
source manifest. This uses the existing app esbuild/noble dependencies. The
normal app tests exercise streamed verification, actual generated native
shell delivery, the real module-init lifecycle, and absent-metadata consumers.

## Qualification limits

The older standalone v147 release proves that the actual release is a
multiasset package (~35.8 MB WASM), not that the managed ABI is shipped. Do not
label that older binary managed or reuse its hash as a newer build result.
Node/module tests and a wasm32 check cannot establish WKWebView WebGL support,
peak memory, native close/rotation behavior, accepted document durability or
canonical export. Those require the final managed package and real runtime
proof, with passed/failed/pending reported separately.

## Opt-in Shift experiment

The default builder and worker recipe stay unchanged. An explicitly requested
`--experiment eframe-shift --experiment-crate /absolute/eframe-0.36.2.crate` stages
the complete checksum-pinned published crate inside the new private source tree,
applies 0015 through the normal checked patch mechanism, and verifies the exact
root lock and dependency source trees. It never patches the shared Cargo cache,
downloads code, changes the pinned upstream revision, or relaxes locked/offline
builds. `receipt.json` records the experiment and before/after tree manifests.

This is an unqualified WebKit-input hypothesis, not a shipping correction.
The native before/after title/Notes, mixed-case/lowercase, modifier/deletion/IME
and canonical save/reopen/export proofs must pass before promotion. Keep the
original assets and manifests for comparison. The independent headless worker
source/lock is not an input for this experiment. Current dependency license audit
limits remain; preserve eframe's actual pinned licenses before distribution.

## Opt-in iOS text activation

`--experiment eframe-ios-input` uses the same exact local crate archive and
includes the Shift correction, one primary Pointer/Touch stream, synchronous
activation after an accepted textual gesture, and the existing DOM input agent.
Its six dependency paths and four source paths have separate closed allowlists
and complete before/after hashes. The original Shift experiment and default
dependency remain available as controls. No shared Cargo source is modified.

The input recipe applies five ordered, separately hashed patches. Primary-contact
routing reuses the exact shared Craft mobile-input patch consumed by the other
editors. Its admitted trusted press/release focus leaves a focused text agent
and composition intact; drag, cancellation and secondary-contact gates remain.

| Part    | Responsibility                                          |
| ------- | ------------------------------------------------------- |
| `0017a` | Local eframe dependency and lock binding                |
| `0017b` | Activation state, DOM text agent and runner handshake   |
| shared  | Primary pointer/touch routing and admitted canvas focus |
| `0017d` | DeckCraft canvas text admission                         |
| `0017e` | Existing input contract tests                           |

The builder permits only that exact sibling patch path; other external paths,
missing, repeated or reordered parts are refused. It checks every patch hash
before applying the first, then verifies the complete dependency tree and managed
source hashes. Other experiments keep their single-patch receipt shape. Historical
receipts retain the previous local patch identity in Git history. Updated recipes
require their own compilation, browser and iPhone qualification; they do not
change installed runtime asset pins.

The normal managed engine patch handles surrounding IME deletion through the
existing Session, composition snapshot and undo/journal. It preserves selected
text, Unicode scalar bounds, inherited run properties and paragraph marks.
Commit or navigation produces one canonical replay record; cancellation restores
the original document. The headless worker keeps its separate pinned recipe.

The real egui harness covers textual admission, Notes focus, cancellation,
multiple contacts and owner changes. These tests and WASM compilation do not
prove that WKWebView opens the software keyboard on the first gesture. Keep the
experiment unqualified until the final iPhone artifact passes input, correction,
recovery and export. Building this package does not change the runtime asset pins.

## First-touch cursor correction

The reviewed successor recipe repeats the dependency arguments in this order:

```sh
--experiment eframe-ios-input --experiment-crate /absolute/eframe-0.36.2.crate \
--experiment egui-touch-input --experiment-crate /absolute/egui-0.36.2.crate
```

The second stage uses the same checked archive/patch mechanism for egui 0.36.2.
It requires the exact first-stage lock and source hashes, and records both
dependency proofs. Other chains are refused before creating the output tree.
The original recipes and their pins remain available.

An initially unfocused touch TextEdit senses clicks so its parent can scroll.
The pinned egui cursor code handled drag-capable responses but did not initialize
the cursor for that click-only tap. Focus arrived on release with no cursor or
IME output; the later frame missed the synchronous mobile activation opportunity.
Patch 0018 initializes the cursor at a genuine click through the shared text
selection path. It applies to ordinary eframe text fields across Craft apps.
The existing eframe admission, drag thresholds and Notes save commands are retained.

The focused regressions exercise the actual widget, first-release cursor/IME,
pointer and touch selection, multiclicks, cancellation, disabled fields, and the
actual DeckCraft Notes release callback with its existing admission gate. These
source tests do not establish that the final WKWebView artifact opens the iOS
keyboard or saves real user input. Candidate assets need separate native input,
canonical save/reopen and export qualification before promotion.

## First-touch admission before DOM focus

The successor input recipe keeps the same eframe/egui revisions and opt-in
chain. A real first Notes click can already own the egui TextEdit and publish
IME while the DOM viewport is still unfocused. `Response::has_focus` includes
that viewport state, so it previously refused this admitted click. The bounded
host regression reproduces refusal with `RawInput.focused = false` and passes
with `true` as the control.

Patch family 0017 requires the current TextEdit memory owner for that real clicked,
enabled response, together with current IME. It retains the synchronous outer
contact/release eligibility, focus-loss cancellation, multitouch, drag and custom
canvas ownership gates. It does not change global egui focus behavior. Patch
0018 carries both real Notes focus cases, no-IME refusal and later-frame refusal.
The recipe hashes bind these source changes; retained candidate04/05 packages
and their receipts remain unchanged. These host tests do not establish the
actual WebKit focus transition or software-keyboard acceptance. Candidate06
still needs iPhone manual edit/save/reopen/export qualification (issue #174).
