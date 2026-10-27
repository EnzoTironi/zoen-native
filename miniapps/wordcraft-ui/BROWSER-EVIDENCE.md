# Managed Word browser qualification

This continuation uses the real pinned WordApp/Session/canvas and shared official
MCP App host. It does not register an available App or prove iPhone behavior.
The existing narrow plain-DOCX profile, canonical binary host and worker remain
separate from full rich Word feature acceptance (#502/#180).

## Sources and reproducibility

- Engine: `d1b0bfc3f9dd83126b162a42b667facecb003b7f`.
- Final producer: Zoen `b0c9886cb3477a1a5a702892dfc1d9a4b4396f11`;
  prepared `fc7d45f946dd20e6d2708d597a4a17c450496f48`.
- Normal public preparation reproduced 784 crate/app/vendor files exactly from
  clean pinned source; `.git`, the genuinely generated Cargo.lock and the source
  receipt were excluded from that byte comparison. The receipt binds 29 authored
  inputs plus both pinned crate archives. `browser-public-replay-06.json` records
  this preparation, not execution.
- Shared Grid current-owner/primary-contact/TextAgent patches are reused with
  verified egui/eframe 0.36.2 archives, in the owned vendor directory only. The
  registry cache and other editors' sealed assets were not changed.

All paths below are relative to the retained external build directory
`zoen-artcraft-next-v500/wordcraft/`.

## Executed qualification

| Scope | Actual result |
| --- | --- |
| Native producer `4da4ab89`, prepared `52308b10` | All-target host/browser Clippy PASS (pedantic/perf and unwrap/expect deny); 28 engine/actual WordApp/input tests PASS, zero ignored; shared layout 2 PASS |
| Native result | `browser-check-12/result.json`, SHA256 `53f4fe943494d2c21bd4af6baebe37c4267410fef0268828ab05c1adf5d7f3c4` |
| wasm32 final producer | All-target Clippy PASS 17.221s; genuine release build PASS 210.727s, jobs1/debug0/incremental0; no flags, caps or timeouts widened |
| WASM result | `browser-wasm-04/result.json`, SHA256 `f48d583b1da234425403db391c9929808beaf0388e878eb37d84e6c6f2e39efb` |
| Actual official MCP browser | Real canvas touchscreen tap and keys, held ordinary autosave receipt, second save, accepted DOCX export, Close and fresh renderer reopen PASS |
| Browser result | `browser-mcp-04/result.json`, SHA256 `0aa77d8777045688fc1ece77f764948abc6310422dace5810dbcd85dca59dc2b` |
| Source consumers | 34 Word/runtime-client/shared-session/export Node tests PASS; whole app TypeScript and focal lint PASS; no repeated full app suite |

The final change after native qualification only addresses seven browser-target
pedantic diagnostics in `api.rs`/`shell.rs`; the native host/WordApp test inputs
are byte-identical. Original compiler failures are retained without suppression.

The browser used a synthetic canonical read/CAS port. It held the first ordinary
autosave while real keyboard events continued, then accepted two distinct saves
through head3. The genuine pinned native reader inspected the actual export as
`Word browser\nSecond line`. Close/reload returned the same 6,437-byte DOCX,
SHA256 `47ac291cc53bc1e864a8a7243ed9da6cdc722666c9751fd2418f5c26247d39ef`,
head3 and zero writes after reopen. This tests the real shared host consumer,
not Core persistence, a certified agent, physical process survival or iPhone IME.
DOM focus already existed before the recorded primary tap; that observation is
not presented as proof that a user gesture opened a mobile software keyboard.

## Immutable package

`browser-package-03/` uses normal wasm-bindgen 0.2.129 web release packaging with
`--remove-name-section`. The prior oversized default package remains retained;
no executable section or runtime limit was deliberately bypassed.

| Asset | Bytes | SHA256 |
| --- | ---: | --- |
| `wordcraft.js` | 80,346 | `ceb462889b8be05859df91f829d0976896b2512dc10f3c4d689eb55eac1914d0` |
| `wordcraft_bg.wasm` | 46,342,731 | `93fe072224eb097f204baf7778d11883362c3235b05021c42338575321e42a41` |
| `browser-bootstrap-03/wordcraft.html` | 624,158 | `a84383c0e5e568ea3d67ae4685ce5116a2b720805bb60f49ba73f56e8f108095` |

The JS/module remain below the unchanged 512KiB/48MiB loader caps. Genuine engine
and font notices accompany the package; this is not a complete transitive license
audit. No deployment URL or existing service route was overwritten.

## Retained failures and contract

- First real WordApp manual/soft-break mismatch: corrected the test to exercise
  upstream PasteText for paragraph LF while retaining typed-soft-break refusal.
- Actual first-touch predicate RED, then a real nested egui lock deadlock RED;
  current-owner admission/geometry and frame access were corrected. Primary,
  multitouch/cancel, stale document and historical/pause negatives remain.
- Browser01 refused Word because the generated shared loader was stale. The
  genuine generator fixed it; actual generated-script tests retain legacy editor
  admission and closed rejection behavior.
- Browser02 exported only `Word b`. Native held-receipt regression independently
  received only `Checkpoint` instead of the full new input (0 PASS/1 FAIL).
  Snapshot had frozen all manual input during normal autosave.
- Manual checkpoints now retain immutable original proposal/mutation/base while
  the same live Session/Undo continues editing. Exact ACK advances only the
  accepted baseline. Wrong or late ACK cannot consume a newer proposal. Explicit
  pause/history and pending agent proposals remain fenced. No debounce increase,
  event buffer, separate journal or authority was introduced.
- Browser03 was an invocation/setup error (wrong seed filename), before browser
  execution. Browser04 used the real retained `wordcraft-empty.docx` seed.

Grid has the same source-level freeze and is tracked independently in #546;
Deck's separate bridge already retains newer manual drafts. These Word receipts
are not attributed to Grid's existing package or native acceptance.

Unsaved WebView keystrokes are memory, not durable state. Ordinary Close must
flush the existing journal/host first. Rich formatting/images/tables, browser
resource registration, canonical creation plus certified-agent review, native
open/edit/save/cold recovery/export and accessibility still require completion.
