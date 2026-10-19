# Pinned Excalidraw MCP Apps renderer

This subtree contains the pinned upstream browser UI and explicit local patches,
not a second MCP server. `upstream.json` records upstream commit
`157aa23ceb1976008aadc89eb05e3444060f09d6`, PR #89 commit
`aafc247efb8ba0689e193557749784ea80ee4996`, exact source hashes and dependency
source pins. The original early-notification and initial-host-context patches
remain under `patches/`. The canonical scene adapter is separate readable source.

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

## Embedded assets and explicit exclusions

`library-assets.mjs` verifies pinned Excalidraw module hashes before applying
bounded source repairs. It embeds the standard Excalifont, Virgil, Cascadia and
other distributed non-Xiaolai font assets. Optional Xiaolai's large CJK font
subsets use the native `system-ui` fallback; Unicode scene strings are unchanged.
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
