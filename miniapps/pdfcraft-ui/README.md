# PdfCraft managed integration

Integration candidate for #581; resumed from the preserved native/worker baseline.
Not registered or enabled in Zoen. No PdfCraft PR or production availability is
claimed. Browser and worker qualification precede canonical integration.

Tracks #581 under #180/#170. Upstream is fixed to PdfCraft 0.13.0,
`d445410807e7eefbe5b96f9662ab963ae8dd2a3a`; source/dependency hashes are in
[upstream.json](upstream.json).

The actual upstream `PdfCraftApp`, Session, COS model, paragraph editor, PDF
writer, renderer and Undo are retained. The standalone parent-message bridge is
not used: posting a save message is not a durable Core ACK. No upstream URL
loader, account, collaboration service or document storage is imported.

The original engine/App have executed on original contributor-authored A4 PDFs:
paragraph replacement, real pixel renders, Undo/Redo, a delayed-save ACK,
stable-save Undo, cold reopen, 32 pages and 1600×1200 PNG/JPEG image PDFs.
Independent pypdf checks confirmed exact replacement text, an unknown catalog
value, page count/rotation and JPEG encoded bytes preserved through rotation and
reopen. The expanded native Mac probe took 1.18 s / 88.6 MB peak RSS. These are
specific fixture measurements, not an iPhone or worst-case memory guarantee.

The first native host qualification passed three real tests: exact proposal/
receipt/retry and accepted-version export; stale paragraph/forged artifact
refusal; malformed source and disabled document JavaScript. The real WASIp2
baseline also executed in production WasmTier limits: a two-page
read used 16.36M fuel, and a page rotation generated a PDF using 28.75M fuel. Its
proposal was independently read by pypdf; an approved-command envelope mismatch
was deliberately retained as a consumer RED before qualifying the corrected
adapter. It does not prove Core authorization or persistence.

The retained adapter host tests now exercised 70 consecutive actual edits, periodic
full-writer compaction with opaque data preserved, false JPEG dimensions and
bounded stream-decode refusal. Core structural/intent tests passed 3/3, and its
Clippy all-targets run passed with pedantic/perf and warnings denied. The native
host/App batch subsequently passed 11 cases, including real
pointer/text events, a newer paragraph draft during ACK, native Undo/Redo,
orientation/short viewport and 70 retained-session checkpoints with compaction.
The successor qualifies twelve host tests: five bridge checks and seven actual
retained-App checks, including TouchEnd/PointerGone opening the paragraph and
consuming text without prematurely writing the COS document. Native and WASM
all-targets Clippy pass with pedantic/perf and warnings denied. Every earlier
failed receipt is preserved. Platform cfg matches the original worker,
installed-font, save-dialog and update-thread paths; no lint was suppressed.

The actual release WebKit package now passes trusted touch → text edit → save v2
→ native Undo/save v3 → Redo/save v4 → fresh-renderer cold recovery and exact
selected-version export. Independent pypdf confirms text, opaque publisher data,
two pages and unchanged geometry. The initial package is a retained causal RED:
upstream paragraph input required hover after touch release had cleared it. The
patch uses only that response's admitted click position inside the same page.
These are browser/component proofs with a synthetic Core CAS port, not Rust
persistence, agent authorization or iPhone acceptance.

The corrected WASIp2 worker also executes in the real signed WasmTier: seven
cases pass, including the exact approved-command operation envelope, immediate
fresh-store retry, cold reopen, hash/scope/unknown-operation refusal. Read uses
16.39M fuel and rotation 29.39M under unchanged 256MiB/2Bfuel/5s run limits.
The one-thread cold compilation took 225.97s, separate from execution (16ms read,
2ms rotation); deployment should reuse the existing operator-owned compilation
cache. Its publisher and scope are synthetic test fixtures. Independent pypdf
confirms exact text/opaque data and the requested rotation. Canonical CLI/Core
integration, live agent refresh preserving Undo, and iPhone acceptance remain
pending. No fifth App resource or worker is enabled on main. GitHub #581 is the
status authority.

The initial structural limits are 8 MiB, 32 declared pages, 8,192 current objects,
64 internal PDF revisions and image dimensions ≤2,048 / 2,000,000 pixels. The
shared Core helper uses the pinned upstream COS parser with a 1 MiB object/xref
stream inspection limit. It checks headers, exact xref locations, repair/
encryption refusal, declared page Count and image scalars. It does not walk the
complete page semantics, render, decode image pixels, edit or normalize bytes.
The engine bridge independently checks actual page count, geometry, warnings,
representation preservation and bounded stream/JPEG admission. Internal revision
compaction through the real full writer passed native checks; Core retains exact
chosen versions independently of PDF's internal revision chain.

An older manual ACK must not rebase newer COS edits or a newer unfinished UI
draft. Refused reviewed edits restore the real native Editor/Undo/Redo. JavaScript
is disabled before open and cannot be re-enabled by the managed Session. Time and
authorship are content: retain exact accepted output rather than expecting a
fresh execution on another runtime to produce the same bytes.

Preparation refuses changed pins/manifests, dirty or symlinked source and generated
adapter inputs. The input foundation replay compared all 1,564 original tracked
files; only Cargo.toml changed and every existing renderer/platform vendor patch
remained intact. Use `prepare-engine.py --upstream OWNED_CLEAN_PIN` for inspection,
or `--managed` for the native binary adapter. Preparation never means execution.

After a source-qualified inspection build, generate an original JPEG with
`test/generate-jpeg.py NEW_JPEG_PATH`, then run:

```sh
python3 test/native-probe.py --binary QUALIFIED_INSPECTION_BINARY \
  --receipt SUCCESSFUL_CARGO_GUARD_JSON --jpeg GENERATED_JPEG \
  --output NEW_RUNTIME_DIRECTORY
```

The verifier requires pypdf and the generator requires Pillow (both are available
in the bundled workspace Python). It checks the binary's successful source receipt,
runs the actual engine/editor, retains results outside the repository and reads
the generated PDFs independently. It does not skip failed or unavailable paths.

Source is MIT OR Apache-2.0. Retain source/dependency/font notices. The actual
`docs/brand/LICENSE-brand.txt` requires forks to remove ArtCraft trademark
artwork; the managed patch excludes those graphics and retains source attribution.
See [NOTICE.md](NOTICE.md). No optional OCR models or external document/media assets
have been imported.
