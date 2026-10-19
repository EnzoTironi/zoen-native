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

The native catalog creates an Excalidraw App Item in the selected Space. The same
compact Rust projection supplies its chat and Home cards. The scene belongs to
that Item's version history; its full JSON, application state and embedded image
bytes use the existing encrypted, content-defined file chunks. The App envelope
contains the owned file manifest and small counts, without putting the scene or
encryption keys in `view_json` or a card snapshot.

The bundled editor uses the official MCP Apps bridge with an item-bound
`canonicalScene` capability. Six `app_scene_*` tools transfer a maximum 16 MiB
scene in 192 KiB chunks. The host checks the exact Item, order, UTF-8, size and
SHA before admitting a typed `app_document_save`. Rust independently validates
the scene and images, current membership/writer authority, app grant and observed
version/SHA. A checked save atomically publishes the owned chunks and one App
version. Its bounded durable receipts let a lost acknowledgment retry the exact
mutation; they do not adopt an unrelated newer scene as the editor's base.

The renderer captures all three Excalidraw `onChange` arguments. It retains the
unacknowledged attempt and newer edits, serializes writes, and reports only small
save-state metadata through `ui/update-model-context`. Back and Details freeze
editing and await the final acknowledged flush before retiring the frame.
Ordinary Save resumes editing. Failed saves leave the drawing open. An explicit
native Save Copy asks the app-side `prepare_scene_copy` tool for the current
canvas, verifies the staged chunks, and creates one new App Item/card atomically.
The original Item remains intact. An ambiguous copy retries the same mutation
with editing paused. Account replacement and external navigation cannot bypass
the active editor; owner retirement and explicit device erasure still fence its
authority immediately.

The canonical resource is self-contained and has no connection origins. Canvas
editing, images and bundled normal fonts do not need a CDN or a public provider
checkpoint. Native system fonts cover the optional CJK fallback. The unused
provider AI/Mermaid path is disabled through the public AI policy with two
explicit pinned library guards; provenance and bundled licenses record that
change. Provider export and unsupported browser file actions are absent from
this native editor. This is not a claim of every Excalidraw feature or platform
being accepted.

The older browser core retains received App-owned document manifests but rejects
creation and generic execution of this drawing until its typed document host is
implemented. The iPhone implementation's current source tests cover exact scene
and image bytes, cold reopen, stale saves, lost acknowledgments, copy rollback,
cross-chat fresh keys and current/historical blob demand. The signed v89 native
proof below covers the canonical editor; the v87 proof remains separate evidence
for the remote-provider viewer.

### Bounded canonical native proof (v89)

The preserving update on the isolated iOS Simulator opened a rectangle saved under
v88. Touch added a second horizontal rectangle, the iOS keyboard entered `Zoen`
and `MCP Apps`, and touch connected them with an arrow. Native saved status and
guarded Back returned a Home card showing `5 elementos`. After a full process
restart, the same card retained its count and opened the same five-element drawing.
No Metro or runtime JavaScript injection was used; native instrumentation supplied
accessibility inspection.

One uninterrupted four-step cold-reopen path passed once: restart, Home count,
same-card open and editor readiness. The recorded first take supplies the visible
diagram proof. The 32-step authoring recording retains two failed automation waits
and cross-tree warnings; it is not a successful full editing replay or two-pass QA.
The two trimmed recordings are 44.067 and 18.233 seconds. A
[combined 62.3-second proof](../../.argent/recordings/excalidraw-canonical-iphone-v89-proof.mp4)
joins them without new app actions or overlays.

Evidence: `acceptance.json` → `evidence.nativeCanonicalExcalidrawV89` and
`/private/tmp/zoen-excalidraw-canonical-iphone-qa-v89/native-proof.json`. The screenshot
is `.argent/screenshots/excalidraw-canonical-iphone-v89.png`. App 775/775,
TypeScript/Oxlint/Oxfmt, 15 renderer tests and all three exact-source Expo CI jobs
pass separately. Prior v88 full FFI and browser image-byte tests keep their own
attribution. This native take does not certify image insertion, an independent
recipient, conflict/copy-failure UI, physical iPhone, native surfaces or full parity.
The human preserving update retained four Home cards, recipe 6/8 and larger text;
public executable, JavaScript and strict signature checks passed. This is bounded
Home inspection, not full-app reacceptance. The accepted whole-app v85 artifacts
and the v87 remote readback remain unchanged.

### Native image import and linked-device readback (v90)

The native Photos picker imported a synthetic JPEG into the original five-element
drawing. Guarded Back saved it as six elements; after a full process restart, the
same Home card opened the drawing with its image still rendered. The previous
recovery copy also retained its six elements. Both simulators received preserving
signed Release updates, with executable, bundled JavaScript and strict signatures
verified. The main simulator retained its four Home cards, recipe 6/8 and larger
text in a bounded smoke check.

This walkthrough found a real image-save race: upstream Excalidraw emits an image
placeholder before its file bytes are ready. The editor now waits for the complete
scene before autosave, Copy or final flush. A bounded timeout keeps the editable
canvas; it never drops the image or claims an incomplete save succeeded. Rust's
strict scene validation is unchanged. Native request failures also present their
human-readable reason instead of a reflected Swift error type.

A fresh memory-only device linked through the normal six-digit native approval
and received 146 encrypted history events. Typed document reads verified both
owned scene manifests and embedded image bytes. Each drawing contained six live
elements and one 21,025-byte JPEG; the two JPEG hashes matched. A second check
retained both version/scene/image pins. This is another device of the same
synthetic identity, not a separate recipient or a cryptographic author-device
proof. The helper was stopped, its owned volatile vault cleared and its exact
temporary device revoked through native Devices UI. The chat retained both
six-element pinned cards and the copy's six-element inline card.

The [47.23-second native recording](../../.argent/recordings/excalidraw-photo-iphone-v90-proof.mp4)
joins the import and restart takes without recoding their video packets. The
originals retain their automatic static-frame trimming (122.042 seconds of wall
time). These are recorded walkthroughs; system-picker/DOM target warnings prevent
a claim of a full import replay or a two-pass regression.

Evidence: `acceptance.json` → `evidence.nativeExcalidrawImageV90` and
`/private/tmp/zoen-excalidraw-native-image-v90/`. Runtime commit `db626dce` has a
904-file signed-build input receipt; `6902ce6a` only fixes Linux admission of the
macOS Swift test. The macOS app check passed 776 tests and the renderer passed
24 tests, including delayed image preparation. All three Expo CI jobs passed on
`6902ce6a`. This does not promote the whole-app baseline or certify physical
iPhone behavior, all MCP providers or the later builtin-renderer integration.

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

The v87 native walkthrough opened the catalog and Excalidraw, started the actual
editor, drew a second rectangle, typed `Zoen + MCP` with the iOS keyboard and drew
a connecting arrow. The native status received the real server save ACK. The
53.4-second H.264 recording used an ordinary fresh Release opening, without Metro,
a debugger or runtime JavaScript injection. The app's 748 tests, four compiled
upstream UI regressions and all three exact-source Expo CI jobs passed.

A later observational native breakpoint identified the real `save_checkpoint`
request's checkpoint. A fresh connection then called only `read_checkpoint` and
verified all five element IDs, both labels, the newly drawn rectangle's geometry
and both arrow bindings. It did not create, save or export a diagram. This
independent server read is the persistence proof; leaving and reentering the
editor alone would only demonstrate local state retention.

Evidence: `/private/tmp/zoen-excalidraw-iphone-qa-v87/`, notably
`native-server-readback-proof.json`, the raw server response and signed-product
receipts. The recording is
`.argent/recordings/screen-recording-388CA707-A103-4845-B5FE-BAA9526D3D57-1792358439976.mp4`;
the saved screenshot is `.argent/screenshots/excalidraw-iphone-v87.png`.
The original live authoring flow retains selector warnings and is not claimed as
a two-pass replay regression. Both simulators received the exact signed v87 app
through preserving installs. The human simulator's Home cards and larger text
were smoke-checked; prior PDF/voice/history acceptance remains the separate v85
record rather than a new v87 acceptance claim.

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
- [Canonical encrypted drawing documents](../../crates/roda-ffi/src/app_documents.rs),
  [bounded scene transfer](../../apps/zoen/src/miniapps/canonical-scene.ts),
  [native drawing host](../../apps/zoen/src/miniapps/canonical-view.tsx)

Adding another provider requires endpoint and authorization policy, real
descriptor/resource discovery, supported CSP and permission handling, tool grants,
and native runtime acceptance. The existing public Excalidraw route does not
imply support for arbitrary remote URLs or authenticated providers.
