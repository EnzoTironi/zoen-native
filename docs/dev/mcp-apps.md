# MCP Apps in Zoen

Zoen now has an implementation of the standard MCP Apps host and a public remote
Excalidraw connection. The actual server discovery and synthetic checkpoint
roundtrip passed. **Native UI proof is pending**; these server receipts do not
certify rendering, interaction, accessibility or full host conformance on iOS.

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

## Current remote-session authority

The registry currently allows only `https://mcp.excalidraw.com/mcp`. Redirects,
credentials and unsupported authorization are rejected. This public connection
has no OAuth implementation or token storage. The viewer explicitly uses Expo's
streaming fetch; bounded response readers preserve the original response because
React Native's global `Response` constructor cannot wrap a stream.

The session binds requests and late responses to the unlocked account, device
and frame lifetime. Blur, account changes and disposal close the frame and abort
the connection. Initial launch requires the user's action and calls the
discovered UI tool. App-originated calls must be declared by that same server and
app-visible; resource reads are restricted to the bound UI resource.

For Excalidraw, starting a drawing grants checkpoint reads and autosaves only for
the exact checkpoint returned by that launch. Another checkpoint ID is rejected.
Destructive or otherwise unknown writes require confirmation. Export has a
separate explicit publication confirmation because it uploads the drawing and
returns a shareable URL. Server hints alone do not authorize publication.

The external app receives presentation context such as theme, dimensions, locale
and time zone. Zoen does not automatically send account identifiers, chat history,
members or keys. Model-context updates remain local to this session, with no AI
submission. The drawing is a provider-backed session draft, not a canonical Zoen
chat item. Sharing into chat and native device capabilities are unavailable on
this remote viewer. Closing the session does not claim to delete the provider's
checkpoint or provide durable reopen history in Zoen.

## Actual Excalidraw proof

The [official Excalidraw MCP App](https://github.com/excalidraw/excalidraw-mcp)
exposed `read_me`, `create_view`, `export_to_excalidraw`, `save_checkpoint` and
`read_checkpoint`. Discovery returned `ui://excalidraw/mcp-app.html`, the standard
App MIME type, and a 450,212-byte HTML resource with SHA-256
`441650e7bc844a43511b906281d11589e8d24a34a83d657c1d7a56f907c21a63`.
Its declared connection and asset origin was `https://esm.sh`; clipboard-write
was requested, not granted.

The live proof created one identifiable synthetic rectangle, read its checkpoint,
moved its x-coordinate from 120 to 160, saved and reread the same checkpoint.
Both reads contained exactly that one element. It invoked no export, created no
public share link and used no Zoen content. The session closed; the server exposes
no deletion tool, so the synthetic checkpoint remains subject to provider
retention. This was Node HTTPS execution, including a discovery pass with the
installed React Native `Response` constructor, rather than native UI execution.

Local evidence is under `/private/tmp/zoen-remote-mcp-excalidraw-v86/`:
`discovery.json`, `checkpoint-roundtrip.json`, `final-discovery.json` and
`source-freeze.json`. The session's nine focused tests include actual official SDK
JSON/SSE handling with the installed React Native response constructor, tool
binding, owner teardown, bounds and unsupported authentication.

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
- [Canonical native-surface batch](../../crates/roda-ffi/src/native_surfaces.rs)

Adding another provider requires endpoint and authorization policy, real
descriptor/resource discovery, supported CSP and permission handling, tool grants,
and native runtime acceptance. The existing public Excalidraw route does not
imply support for arbitrary remote URLs or authenticated providers.
