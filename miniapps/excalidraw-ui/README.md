# Pinned Excalidraw MCP UI

This subtree builds an explicitly patched Excalidraw MCP UI. It contains only the
upstream browser sources, not a second MCP server. The live official server still
supplies discovery, diagram creation and checkpoint tools through standard MCP.
Its original resource identity remains separate from this presentation resource.

`upstream.json` pins upstream commit `157aa23ceb1976008aadc89eb05e3444060f09d6`
and PR #89 commit `aafc247efb8ba0689e193557749784ea80ee4996`. The downloaded
PR diff is retained in `patches/89-buffer-initial-events.diff`. The small second
source patch records consumption of the negotiated initial display mode and
buffering of host-context changes before the editor React effect mounts.
No host timers, repeated tool-input injection, HTML response rewriting or
relaxed sandbox/storage permissions are used.

## Build

From the repository root, install the app tooling and this isolated UI lock:

```sh
npm ci --prefix apps/zoen
npm ci --ignore-scripts --prefix miniapps/excalidraw-ui
node miniapps/excalidraw-ui/build.mjs
node miniapps/excalidraw-ui/build.mjs --check
npm test --prefix miniapps/excalidraw-ui
```

The builder uses pinned esbuild and the app's pinned Oxfmt. It verifies recorded
source hashes, bundles the upstream browser entry and official App SDK, and
retains upstream's exact versioned React/Excalidraw/morphdom esm.sh imports.
No server dependencies or tools are copied into the app.

Outputs are `apps/zoen/src/miniapps/vendor/excalidraw-ui.ts` (default HTML plus
named `manifest`) and its JSON integrity manifest. The resource URI is
`ui://zoen/excalidraw.html`. `--check` regenerates in memory and compares exact
artifacts and third-party notices without writing. The manifest records the
presented bytes/SHA, exact source and lock hashes, upstream/patch provenance,
bundled dependency licenses and esm.sh-only resource policy.

## Verification boundary

The test compiles the actual patched upstream React UI and actual official App
SDK, then connects it to Zoen's production AppBridge. Host launch notifications
are delivered while the Core component is still unmounted. Assertions cover the
initial rectangle, checkpoint result, negotiated fullscreen and early context
changes. Independent controls compile the same source with each repair removed
and reproduce the missing editor. Only the expensive graphics library is a
presentation fixture; React, useApp, message transport, AppBridge, buffering and
upstream UI event handlers are real. This is not a claim of native WebKit,
Excalidraw canvas rendering, remote checkpoint persistence or iPhone acceptance;
those require the separate signed-app runtime journey.

See NOTICE.md, LICENSE and THIRD-PARTY-LICENSES.txt for attribution. Updating
upstream requires reviewing both source patches, refreshing exact source hashes,
rebuilding, and rerunning the early-delivery controls and native acceptance.
