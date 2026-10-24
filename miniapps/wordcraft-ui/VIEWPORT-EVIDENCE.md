# Managed Word viewport and first touch

## Scope

The real pinned Word renderer now consumes the host safe content rectangle. The
shared eframe contact adapter restores browser focus to the canvas on an admitted
trusted primary touch when the app does not already have focus. It does not
force the hidden text input, synthesize document focus, bypass IME ownership or
change the document ABI. The real engine, upstream commands and canonical host
remain authoritative.

Native iPhone keyboard, physical input, OS sharing and durable Core acceptance
remain pending. Desktop WebKit with an iPhone descriptor is browser evidence.

## Causal failures and fix

The root eframe Ui covers the viewport even when RawInput carries safe insets.
Word's managed Web/focus mode places an empty page in roughly the first 98 points.
At the retained iPhone geometry that page was behind the host status/close area.
The new wrapper intersects the current parent rectangle with ctx.content_rect()
and clips the actual WordApp there, following the existing Grid canvas boundary.
Already-safe parents do not receive duplicate padding.

A native egui regression uses the actual managed open, writer, Session, WordApp,
canvas/page rectangles and real caret across portrait, landscape, keyboard-height
and zero-inset layouts. Without the wrapper, the causal RED was:

    canvas [[0,0]-[402,874]] escaped host content [[0,118]-[402,840]]

The first RED attempt failed before compilation; a later attempt hit the fixture's
unapplied texture disposal guard. Neither is counted as geometry evidence. The
texture fixture correction explicitly disposes real TextureDelta values because
this geometry test has no GPU. The preserved causal RED is viewport-red-05.

After the geometry fix, actual WebKit trusted taps still targeted the canvas with
document focus false, leaving the input inactive. A real mouse contact focused the
same canvas and input. The existing contact patch had removed upstream canvas
focus. Restoring focus_without_scroll(canvas) only for an admitted trusted primary
touch fixes the first-touch browser path. Existing focus, composition and contact
ownership are preserved. Grid and Photo must independently qualify their consumers.

## Qualified package

Source: 18a8a3da43351ebb5d0f768380ec69fefe0d387d. Prepared source:
4f2e5296bf476c595d78901b2f5c13ba348de69f.

| Asset | Bytes | SHA-256 |
| --- | ---: | --- |
| wordcraft.js | 80480 | da14faca74ed3ffae283ffdac1c40674ac049ccae80e631ed9be60502d360016 |
| wordcraft_bg.wasm | 46355404 | 4aefe20f62636e7f725f6877feaf71cf7901e077fa53c9ccd04ffd16623d6e5f |
| wordcraft.html | 624193 | b33797470cbc0f370606020d580e18d42ac640d11fac187184ac1aabf0fc9452 |

Normal producer replay compared 792 real prepared files. Its only successor
changes were the shared eframe event implementation and its provenance receipt.
WASM all-target Clippy and the normal release build passed. wasm-bindgen 0.2.129
used the existing packaging options. The 512 KiB JS / 48 MiB WASM caps are unchanged.
Bootstrap HTML, its combined engine/font notices, worker component and ABI are
unchanged. Current registration pins bind the real successor assets.

## Results and remaining RED

External receipts are under
`/Users/enzotironi/.codex/builds/zoen-word-native-viewport-v568`.

| Evidence | Result |
| --- | --- |
| viewport-red-05 | Causal actual canvas escape |
| viewport-final-07 | Native actual Word geometry/layout: 3 tests passed |
| viewport-clippy-08 | Native all-target browser consumer Clippy passed |
| viewport-wasm-clippy-09 | WASM all-target Clippy passed |
| primary-touch-wasm-clippy-17 | Final shared focus consumer Clippy passed |
| primary-touch-wasm-build-18 | Final normal WASM release passed |
| miniapps-word-resource-23 | Configured real resource: 5 tests passed, 0 ignored |
| webkit-observe-close-21 | Capability assertions below passed; teardown remains RED |

The browser run uses actual module/WASM, normal bootstrap, shared SDK/session and
export consumer, with a synthetic Core CAS. It proves these assertions:

- Real painted empty paper begins at y118, below the host close exclusion.
- First trusted touchscreen contact changes the real input from inactive to
  focused/pointer-events auto, with a caret at x24.5/y152.59375.
- Real browser keys produce exactly `Word browser\nSecond line`; actual engine
  DOCX inspection and independent PDF text extraction agree.
- Paper remains visible after landscape, keyboard-height and portrait changes;
  layout does not add document writes.
- A fresh renderer reloads the exact accepted DOCX/head, 6437 bytes, version 2,
  SHA-256 47ac291cc53bc1e864a8a7243ed9da6cdc722666c9751fd2418f5c26247d39ef.
  Reopening performs zero writes.

Browser key dispatch precedes the engine UI frame. The original immediate-flush
fixture captured a pending-input prefix (`Word browser\nSeco`); receipt
webkit-fixed-primary-19 is retained. The rendered-input proof waits for real child
animation frames before taking its snapshot. It does not claim that a close
racing an unprocessed input event is qualified.

The final test is deliberately not marked PASS: during reload, runtime-browser's
pagehide listener retires the engine before managed-binary-app's pagehide listener
clears its callback. Word then rejects zoenHostOnChange(undefined) with
host_unavailable_or_retired. The exact stack and complete prior assertions remain
in webkit-observe-close-21/result.json. The shared host lifecycle fix is a separate
integration change; it does not require rebuilding this engine package.
