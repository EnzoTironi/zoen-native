# Pinned Excalidraw MCP Apps renderer

This subtree contains the pinned upstream browser UI and explicit local patches,
not a second MCP server. `upstream.json` records upstream commit
`157aa23ceb1976008aadc89eb05e3444060f09d6`, PR #89 commit
`aafc247efb8ba0689e193557749784ea80ee4996`, exact source hashes and dependency
source pins. The original early-notification and initial-host-context patches
remain under `patches/`. The canonical scene adapter is separate readable source. The maintained legacy
adaptation keeps host lifecycle in `mcp-app.tsx`, streaming SVG rendering in
`mcp-diagram.tsx`, scene conversion in `mcp-diagram-scene.ts`, and editor chrome
in `mcp-chrome.tsx`; each remains covered by the pinned build source inventory.

## Build

```sh
npm ci --prefix apps/zoen
npm ci --ignore-scripts --prefix miniapps/excalidraw-ui
node miniapps/excalidraw-ui/build.mjs
node miniapps/excalidraw-ui/build.mjs --check
npm test --prefix miniapps/excalidraw-ui
```

The pinned builder bundles React, Excalidraw 0.18.0, the official MCP Apps SDK
2.0.3 and every runtime stylesheet/font/image dependency. SDK 2.0.3 preserves the
negotiated experimental capability marker; upstream's older SDK stripped it.
The generated HTML has no external imports or network origins and must fit the
existing 8 MiB host verifier. `--check` compares exact output bytes and notices.

Outputs are `apps/zoen/src/miniapps/vendor/excalidraw-ui.ts` (default HTML and named
`manifest`), its JSON integrity manifest, and the same HTML/manifest under
`crates/miniapps/resources/excalidraw.*`. Resource identity is
`ui://zoen/excalidraw.html`. Provenance distinguishes this patched presentation
from the original remote provider resource; it does not attest a vendor signature.

For an independently admitted external release, use an isolated destination:

```sh
node miniapps/excalidraw-ui/build.mjs --package-output /tmp/zoen-excalidraw-package
node miniapps/excalidraw-ui/build.mjs --package-output /tmp/zoen-excalidraw-package --check
```

This writes HTML, manifest, provenance, license and third-party notices only to that
directory. It leaves the built-in renderer and Rust resources unchanged. Upload and
admission still use the authenticated File/App commands; this builder grants no
permissions and does not change an existing App's captured resource pin.

## Canonical drawing behavior

The negotiated `experimental.zoen.canonicalScene` capability selects the canonical
editor. Its full scene contains the database serializer's elements and selected
persistable appState plus exactly the owned BinaryFiles referenced by live images.
Excalidraw's database serializer alone intentionally omits files. Images remain
embedded bytes; the guest never uploads them to an external service.

Standard `tools/call` uses `app_scene_read`, `app_scene_read_chunk`,
`app_scene_begin`, `app_scene_append`, `app_scene_commit`, and `app_scene_abort`.
UTF-8 scenes are bounded to 16 MiB, chunks to 196608 bytes, and transfer hashes are
verified. The host binds authority/CAS/mutation receipts to its canonical item.
Failed saves retain their exact attempt before newer edits can advance; metadata
status uses `ui/update-model-context` without scene text or an AI message.

`ui/resource-teardown` freezes the real editor, waits for its view-only callback,
captures the latest API scene and awaits save acknowledgment. Success remains
view-only through navigation. Host-called `resume_scene` restores editing after
an ordinary Save Now/background flush; a prepared copy cannot be resumed that way.
Host-called `prepare_scene_copy` drains an admitted save without retrying a failed
CAS, stages the latest frozen scene without committing it, and retains the same
staged result for lost-ACK retries. The native host owns visible save/retry/copy
controls. Account privacy retirement can interrupt this in-memory guest; this
renderer does not claim process-death recovery of unsaved browser edits.

The canonical marker remains version 1. This renderer additionally reports
`sceneSave.liveRefresh: 1`, readiness, clean status and the exact read/save head.
Readiness starts after the canvas callback and MCP handlers are installed. The host
only enables owned-scene agent access after that observation. Older releases keep
their existing manual editing behavior and cannot falsely advertise agent support.

Host-called `refresh_scene` pauses a clean editor, waits for view-only acknowledgment,
reads a separately staged candidate through the same chunk tools, and applies it
using Excalidraw's public `restore`, `addFiles` and `updateScene` APIs. Its exact
normalized scene must return through `onChange` before acknowledging the new head.
The host retains its previous save base until that acknowledgment, then calls
`resume_scene` to restore editing. Lost application/context/resume acknowledgments
can be retried in the same frame without creating another document version. Dirty input,
image import, release/owner changes and missing acknowledgments refuse the update;
there is no automatic reload that drops a draft. Agent prompts and review use the
same native sheet/session, and its head is checked by a pinned Core read before
publishing a request. These are controller/React guarantees; iPhone WebKit acceptance
remains a separate native journey.

## Embedded assets and explicit exclusions

`library-assets.mjs` verifies pinned Excalidraw module hashes before applying
bounded source repairs. It embeds the standard Excalifont, Virgil, Cascadia and
other distributed non-Xiaolai font assets. Optional Xiaolai's large CJK font
subsets use the native `system-ui` fallback; Unicode scene strings are unchanged.
The host touch stylesheet hides only Excalidraw’s keyboard hint row for a coarse
pointer; toolbar controls and accessible names remain available.
The canonical editor has no CDN fallback or browser-local scene persistence.

Public `aiEnabled=false` hides provider AI. Two pinned repairs also guard the
manual Mermaid menu and automatic Mermaid-paste conversion, which upstream 0.18
otherwise leaves reachable with that flag. Only that optional converter branch
is excluded. Ordinary text/Unicode paste, vectors, images and full scenes remain.
Canonical load/save/export menu actions requiring unavailable browser or remote
upload authority are disabled using public `UIOptions`. Link opening is prevented;
embeddable content is disabled. The authoritative Rust parser also rejects
unsupported elements and external image assets.

## Verification boundary

Tests execute actual React handlers and save/transport controllers with only the
expensive drawing component substituted for lifecycle probes. They cover frozen
copy/navigation, latest full-scene capture, ACK replay, conflicts and image bytes.
The original initial-event tests use the real official App and production host
AppBridge; independent repair removals reproduce lost early events.

The optional owned Chrome test executes the full generated production renderer,
production AppBridge and unchanged opaque sandbox/CSP. It checks real canvas boot,
image bytes, ordinary Unicode paste, canonical save and the absence of a Mermaid
entry. Independent menu/paste guard removals fail those same assertions. The
library makes a caught script-evaluation probe denied by CSP; no unsafe-eval or
network authority is granted. This headless proof does not establish iOS WebKit,
physical device performance, native navigation or multiplayer behavior. Those
remain separate signed-app acceptance journeys.

See NOTICE.md, LICENSE and THIRD-PARTY-LICENSES.txt. Updating dependencies requires
reviewing source repairs, refreshing pins, rebuilding and repeating these checks.
