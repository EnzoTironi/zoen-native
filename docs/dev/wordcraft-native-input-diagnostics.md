# Word native input diagnostics

For the next iOS QA cohort, set `EXPO_PUBLIC_ZOEN_WORDCRAFT_RUNTIME_QA=1` in both normal build plans and preserve the public environment in their manifests. Production defaults off. `EXPO_PUBLIC_ZOEN_DECKCRAFT_INPUT_QA` may remain `0`. The gate additionally requires the pinned Word runtime/ABI. No WASM rebuild is required.

Read accessible nodes `wordcraft-input-order-qa` and `wordcraft-runtime-qa` before and after one real contact. These use the existing bounded all-frame WK telemetry channel, a separate per-document QA token and read-only `status`. Replaced/disposed documents lose authority; pending replies cannot publish. No document text, key strings, actor IDs or credentials are collected.

Runtime label begins `sample/eventSerial/ownerReady/engine/frozen`; engine 0 is unknown, 1 is a validated status reply, 2 is refused/unavailable. Frozen -1 is unknown. `head` is the real document version/hash. DOM and copied activation rows are captured together after the read-only status reply; the status itself precedes that observation. Event serials bind rows to that DOM sample. Ready-before-canvas waits through a cleaned-up DOM mount observer. At most 64 snapshots and 16 activation rows are retained.

`dom` scalar order:

1. Active role (0 absent, 1 exact TextAgent, 2 canvas, 3 other), document focus, visibility, composing, DPR, input pointer-events (0 none, 1 auto, 2 other).
2. Input x/y/width/height; canvas CSS x/y/width/height; canvas backing width/height; inner width/height.
3. User activation active/ever (-1 unavailable); visual viewport width/height/scale/offsetTop/offsetLeft.

Activation rows are serial/event-kind/trusted/activation-active/activation-ever. Event kinds, starting at zero: pointerdown, pointerup, pointercancel, touchstart, touchend, touchcancel, focus, blur, beforeinput, input, compositionstart, compositionend. Activation is captured synchronously in each listener.

`RNtouches` and `RNfocusRequests` separately count native touch callbacks and existing requestFocus attempts. They do not prove first-responder admission or keyboard presentation. DOM focus and desktop WebKit success also do not prove iOS software keyboard acceptance. This patch observes existing focus behavior without changing it.

Validation: 26 focused tests passed; targeted lint/format passed. One actual EMPTY Word / native-body / MCP / desktop-WebKit probe decoded the real head, frozen=false, active input and seven trusted events with zero writes or console errors. Native keyboard failure #557 remains open. Full application checks are delegated to the integration cohort. Rich Word #583 remains separate work in progress.
