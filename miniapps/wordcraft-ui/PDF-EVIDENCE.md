# Accepted Word PDF export

Continuation of [#502](https://github.com/EnzoTironi/zoen-native/issues/502).
The closed worker and BrowserBridge expose `docx` and `pdf` for the exact
accepted `Scope`/`Head`. Export never reads the visible pending draft. Historical
export opens the selected original DOCX under its own version/hash. Existing
Core transport, current permission checks, transfer retirement and OS sharing
remain the authority; this adapter adds no Store, grant or commit.

PDF uses the pinned WordCraft `d1b0bfc3f9dd83126b162a42b667facecb003b7f`
layout, display/font selection and `wordcraft_pdf::export_layout`. The actual
layout is reused rather than reconstructed. It refuses an upstream `.notdef`
glyph before returning PDF bytes. Accepted DOCX, pending input and cold reopening
remain intact when export fails. This is still the declared `plain-text-v1`
DOCX profile, not rich DOCX import or full font coverage.

The single worker operation is `export:{scope,head,format:"pdf"}` alongside
`open:{scope,head}`. A simultaneous `command`, unknown format, foreign scope or
wrong head is refused. Response metadata binds the source head, actual output
hash/length, PDF MIME and engine revision. The output stays within the existing
1-MiB WasmTier frame, reserving its four-byte prefix and maximum 64-KiB metadata;
no execution, memory, output or permission limit was increased.

## Executed evidence

Receipts and sealed outputs are external under
`/Users/enzotironi/.codex/builds/zoen-word-pdf-export-v549/`.
All compilation used private source/targets, jobs1, debug0, incremental0,
offline/locked source guards and the existing 8-GiB disk floor. No shared v410,
SDK, device, maintained service or provider was used.

- Original `pdf-red-03`: the real unmodified bridge refused both PDF operations
  (0/2 PASS). Initial native export tests and real WordApp/BrowserBridge suite
  subsequently passed 9/9 and 31/31 respectively.
- First actual signed WASIp2 execution `actual-wasi-15` passed its frame/metadata
  checks, but independent PDF extraction and rendering found missing emoji
  glyphs (NULs and visible boxes). That RED and both original PDFs remain kept;
  successful framing was not treated as fidelity.
- Corrected native `pdf-glyph-tests-22`: 4/4 PASS, including exact before-ACK,
  accepted and selected cold history, foreign scope/head, unknown format,
  unchanged pending input/DOCX on missing glyph, and the real BrowserBridge
  canvas with newer manual input. Actual FontDb probing distinguished native
  Apple emoji fallback from static WASI fonts; the native absent-glyph test
  uses a verified unsupported scalar, while WASI retains the original emoji
  refusal. The earlier native corpus-assumption RED remains recorded.
- Current host/browser all-target Clippy, WASIp2 bins Clippy and browser-target
  all-target Clippy pass with `-D warnings`. Genuine release components and
  browser WASM were rebuilt with the actual pinned fonts.
- Final `actual-wasi-26` passes all 17 real framed calls plus the three clocks
  in 347.977s, using the signed actual worker and unchanged limits.
  This includes the original Unicode DOCX/edit/retry/cold cases, explicit emoji
  PDF refusal with no bytes, and supported exact current/historical PDFs.
  Independent pypdf parsing verifies their exact distinct text, embedded font
  mappings and tags; Poppler renders were visually inspected. Runtime receipt
  SHA256 `36197ef32a5f2e780860f536c53369c82b987f05d1f4553e7c3132bf8ebe9429`.
- Public preparation was rerun normally at the same pin: all 653 source files
  and the exact 32-input/archive receipt match the qualified prepared browser.
  Final Node31, whole TypeScript, focal Oxlint/Oxfmt and all three normal
  bootstrap `--check` commands pass. These transport/source checks do not
  substitute for an installed app.

Final browser package02 uses normal wasm-bindgen 0.2.129 web packaging with the
unchanged 512-KiB JS/48-MiB WASM caps: JS SHA256
`89a1449b395211e425cb1d53bcacf637b35bec6f8e7edf7b75995357eb01ef59`
(80,346 bytes), WASM
`9931c692606084d9d54951e20343585503788ed4ac9484ab6cd1b96cebf6ae43`
(46,350,798 bytes). Normal combined Word HTML is
`b33797470cbc0f370606020d580e18d42ac640d11fac187184ac1aabf0fc9452`
(624,193 bytes); Deck/Grid companion metadata was regenerated normally from
these shared sources without rebuilding their unchanged engines.

The sealed package retains the actual engine/adapter/font notices, with every
copied notice checked against its source hash; this is not a new transitive
license audit. The default manifest still has no runtime package. External
Word qualification uses an explicitly undeployed reserved HTTPS URL, not a
claim that assets are served. `appAvailable` and `nativeAccepted` remain false
in the package evidence. Deployment, actual browser-to-canonical-host export,
iPhone/share/keyboard/orientation/accessibility and rich DOCX acceptance remain
open. The native Word routing correction in #558 is included as its own
ancestral source; its native acceptance is not inferred from this PDF evidence.
