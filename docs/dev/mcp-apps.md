# MCP Apps in Zoen

Zoen now has an implementation of the standard MCP Apps host and a public remote
Excalidraw connection. The actual server discovery and synthetic checkpoint
roundtrip passed. The signed v87 iPhone Simulator build also passed actual editor
interaction, native server-save acknowledgment and independent readback of that
same drawing. This bounded proof does not certify full host conformance,
accessibility or whole-application parity.

A separate signed v89 proof covers a canonical drawing owned by a Zoen chat and
its full-process reopen. The v90 proof adds actual native Photos import, cold
reopen and encrypted readback on a new linked device of the same test identity.
The whole-application acceptance baseline remains v85.

## Two independent connections

The server advertises its UI through a tool's `_meta.ui.resourceUri`. Zoen reads
that exact `ui://` resource using `resources/read`, requires
`text/html;profile=mcp-app`, and validates the resource's `_meta.ui.csp` and
requested permissions. Tool visibility determines which declared tools the app
may call. These are standard MCP Apps descriptors, without a required Zoen
manifest. See the [MCP Apps specification](https://github.com/modelcontextprotocol/ext-apps/blob/main/specification/2026-01-26/apps.mdx).

- **App ↔ host:** the provider's standard `App` communicates with the official
  `AppBridge` through JSON-RPC over `postMessage`. Zoen uses
  `@modelcontextprotocol/ext-apps` 2.0.3 and the Apps protocol `2026-01-26`.
- **Host ↔ server:** `@modelcontextprotocol/client` 2.3.1 handles Streamable HTTP.
  Its automatic server-profile negotiation is separate from the Apps handshake.
  The real Excalidraw endpoint negotiated core MCP `2025-11-25`; the connection
  does not assume the newest core wire format. See the [official SDK version
  negotiation guide](https://ts.sdk.modelcontextprotocol.io/v2/protocol-versions).

On iOS, a trusted parent document inside WKWebView owns an actual sandboxed child
iframe. It validates the child window, opaque origin, per-frame nonce and message
bounds before forwarding to React Native. The provider continues using its real
parent-window transport. CSP precedes provider bytes, and requested browser
permissions are not automatically granted. Resource-declared origins constrain
network and asset access; the current remote-session parser supports bounded
exact HTTPS origins, a subset of the standard's possible policies.

## Pinned Excalidraw presentation

The deployed Excalidraw resource registers initial tool handlers after its
handshake. A normal iPhone opening lost those events and showed a blank preview;
slower diagnostic delivery rendered the initial rectangle. Its editor also
assumes `inline` on first mount even when the host's initial context is fullscreen.
The failed v86 walkthrough is retained separately from acceptance evidence.

Zoen therefore uses an explicitly pinned, patched upstream UI build, rather than
rewriting downloaded HTML or delaying host notifications. The source, dependency
lock, patches, MIT license and reproducible build live in
[`miniapps/excalidraw-ui`](../../miniapps/excalidraw-ui/). The initial-event fix is
based on [upstream PR 89](https://github.com/excalidraw/excalidraw-mcp/pull/89),
with initial host context handling corrected as well. This is a patched build,
not the unmodified HTML currently served by Excalidraw.

`remote-session.readApp()` still returns the actual remote resource and its actual
SHA. A provider-specific presentation adapter binds the known server, launch tool
and upstream resource URI to `ui://zoen/excalidraw.html`. The presented bytes have
their own SHA and provenance manifest. They cannot add an origin beyond the
remote resource's policy and receive no browser permissions. Discovery, tool
calls, checkpoint ownership and saving continue through the same live official
Excalidraw MCP connection. The generic host has no timing workaround or launch
data replay on layout changes.

## Bundled miniapp views

All nine bundled AppSpecs now provide MCP Apps resources with integrity manifests.
Pet, Poll, List, MapTap, Recipe and Countdown use the shared React UI and official
App SDK alongside Hike, Schedule and Excalidraw. Their former native mutation
fallbacks are removed; historical snapshots retain read-only state. Rust tools,
grants and native compact cards remain authoritative. See
[`miniapps/builtins`](../../miniapps/builtins/) for the controls and visual limits.

List shares through explicit `ui/message` or `ui/open-link` actions; opening a
prepared WhatsApp link never claims delivery. The simulated core operation and
obsolete caller are removed. Repeated poll closure rejects the already-closed
state before producing another plan effect. The shared resource verifier requires
a matching manifest, and the single builder checks physical source boundaries
including symlinks before admitting the selected source graph.

Source checkpoint `1ea24776` passes the actual packaged HTML/official bridge
tests, scoped Rust checks and exact rebuilds of eight resource pairs. Its source
receipt remains indexed by `acceptance.json` → `evidence.bundledMcpViewsV91`.

### Bounded built-in native checks (v91/v92)

Signed v91 native checks cover Pet care, sleep/wake, a tab-paused Dash run and its
saved score; Recipe servings, checklist and a shared cooking step; Countdown date
and ticking; and one MapTap guess/result. Reopening retained scores, servings,
checklist and date; persistence of the cooking step was not separately checked.
These v91 checks cover four scoped views. The same walkthrough retains a real
failure: dragging the globe horizontally navigated back to Home.

Signed v92 source `11ce6ac3` fixes that fullscreen gesture conflict. Rightward,
opposite horizontal and vertical drags now rotate the globe without leaving the
editor. Details and native Back work, retaining the prior 48-point result and
1/5 state without another guess. Management body/edge back-drag did not complete;
only the native Back buttons are accepted. One extracted four-action globe-drag
fragment replay passed once in 2.4 seconds; the original fifteen-step walkthrough
remains diagnostic, not a full replay or two-pass acceptance.

On v92, ordinary catalog creation retained form text while changing the compact
conversation picker. Poll voting changed 1/0 to 0/1; one close chose B - Fruta,
and the same item reopened closed at version 4 without a close button. The exact
Plan line count was not inspected natively; once-only Plan effects retain their
separate Rust/FFI attribution. List add/check reopened at 1/1 with its author.
One explicit native chat confirmation produced message
`01M58REC4HMS3Z7HHHNGH07EE3`, visibly Enviada. The WhatsApp link confirmation was
canceled; no external opening or delivery is claimed.

The v92 app check passes 777 tests plus TypeScript, Oxlint and Oxfmt; all three
[exact-source Expo CI jobs](https://github.com/EnzoTironi/zoen-native/actions/runs/39179002573)
pass. Its 941-input signed build reuses the exact v91 native/core and guest bytes
(533 selected inputs, closure SHA-256 `8b4e9a97a0299a6e032145e1b36b70f867e5d8e765679decc70be5b0e0758699`),
without a new Rust build/test claim. Evidence is under
`/private/tmp/zoen-builtins-native-iphone-v91/native-acceptance.json` and
`/private/tmp/zoen-builtins-native-iphone-v92/root-native-map-acceptance.json`,
indexed by `evidence.nativeBundledMcpViewsV92`.

Poll/List proof is in
`/private/tmp/zoen-builtins-native-iphone-v92/native-poll-list-acceptance.json`.
Its retired 99-step recording retains eleven cross-tree warnings and two
corrected unmet waits; same-item reopening is not a cold-process proof. Both v92
public installed bundles and strict signatures pass. The human preserving update
and native restart retain its four original Home cards, recipe 6/8 and larger
text, with no data changes, modal or keyboard. The Home-only receipt is
`/private/tmp/zoen-builtins-native-iphone-v92/human-home-smoke.json`.
These bounded observations across v91/v92 do not accept every feature of all six
views. Accessibility, reduced motion, physical-device and broader comparative
checks remain open; the earlier
Excalidraw proofs and whole-app v85 baseline are unchanged.

## Drawings owned by a Zoen chat

See the [complete section](mcp-apps-drawings.md#drawings-owned-by-a-zoen-chat).

## Current remote-session authority

See the [complete section](mcp-apps-drawings.md#current-remote-session-authority).

## Actual Excalidraw proof

See the [complete section](mcp-apps-drawings.md#actual-excalidraw-proof).

## Provider boundaries and native cards

Notion's [official MCP client integration](https://developers.notion.com/guides/mcp/build-mcp-client)
provides tools with OAuth authorization. That is separate from Excalidraw's
interactive MCP App. Zoen has not implemented Notion OAuth or verified a Notion
`ui://` resource; this work does not embed the entire Notion application. Future
OAuth credentials must stay in private account/device-bound storage, outside
iframes and shared chat state.

Native cards, widgets, Shortcuts and Live Activities consume compact canonical
Rust snapshots. The `native_surface_items` batch validates the owner/device and
current Space access, reads at most 64 requested IDs consistently, and omits
missing or incompatible items. It carries no HTML, version history or keys.
The v86 source tests passed; OS-level surface acceptance remains pending.

Standard MCP Apps does not require a proprietary native-card protocol. A future
third-party card can use a supported projection of declared structured results,
with an open-app fallback. External tool results must not become encrypted shared
state or native snapshots merely because the HTML can render them; canonical
publication needs an explicit product action and validated projection.

## Implementation map

- [Remote connection and bounds](../../apps/zoen/src/miniapps/remote-session.ts)
- [Viewer and session consent](../../apps/zoen/src/miniapps/remote-view.tsx),
  [Excalidraw action policy](../../apps/zoen/src/miniapps/remote-grants.ts)
- [Official Apps bridge](../../apps/zoen/src/miniapps/protocol.ts),
  [native parent transport](../../apps/zoen/src/miniapps/host-transport.ts),
  [remote CSP](../../apps/zoen/src/miniapps/remote-sandbox.ts)
- [Canonical native-surface batch](../../crates/client-core/src/native_surfaces.rs)
- [Canonical encrypted drawing documents](../../crates/client-core/src/app_documents.rs),
  [bounded scene transfer](../../apps/zoen/src/miniapps/canonical-scene.ts),
  [native drawing host](../../apps/zoen/src/miniapps/canonical-view.tsx)

Adding another provider requires endpoint and authorization policy, real
descriptor/resource discovery, supported CSP and permission handling, tool grants,
and native runtime acceptance. The existing public Excalidraw route does not
imply support for arbitrary remote URLs or authenticated providers.
