# Owned external App packages

This is the first canonical package contract under
[#180](https://github.com/EnzoTironi/zoen-native/issues/180). It reuses the
existing MCP Apps SDK, AppBridge, App card, fullscreen presentation and
Excalidraw scene protocol. Its first document format is **Excalidraw**.
It is not a generic document engine or a registry for arbitrary remote tools.
Native acceptance of this new admission path requires its own integrated build;
earlier bundled-editor and remote-viewer evidence keeps its original scope.

The external package owns its renderer and independent release. Zoen owns the
Item, authorization, encrypted bytes, versions, synchronization and receipts.
Opening a document does not create a remote provider document or send its
contents to a SaaS service. Internal ArtCraft development and extraction remain
stopped; this contract does not announce their packages as already external.

## Publish through the existing File API

A producer uses the normal public `Client.file_add` API with the authorized
Space, name/path, explicit MIME `text/html;profile=mcp-app`, UTF-8 HTML bytes and
optional thumbnail. The same API already has a native UniFFI binding. Do not
insert a File row, write the local database, inject WebView storage or fabricate
an App Item to prepare a package.

The first package must be self-contained and speak the existing Excalidraw
`canonicalScene` contract through the official MCP Apps SDK. The pinned upstream
adaptation and reproducible producer are in `miniapps/excalidraw-ui`; its current
bundled output is `crates/miniapps/resources/excalidraw.html`. A separate package
release can supply those bytes without rebuilding an editor as part of a Zoen
UI change. Keep source, revision, licenses, lockfile and release SHA attributable.

This admission accepts at most 8 MiB. Its host manifest admits no connection or
asset origins, browser permissions or arbitrary capabilities. External scripts,
fonts and network dependencies therefore cannot be assumed to work. A file with
the MIME is an explicit installation candidate, not evidence that its renderer
has passed the protocol, usability or document-format tests.

The CLI exposes the same ordinary publication path:

```sh
zoen file add CHAT editor.html --mime 'text/html;profile=mcp-app'
```

It passes the exact MIME to `Client.file_add`; omitted `--mime` keeps the existing
extension inference, including ordinary `.html` as `application/octet-stream`.
Empty, control-containing or duplicate MIME arguments refuse before opening a
home. This command creates a normal File; it does not install an App, issue a
grant or run its HTML. A model-produced URL or filename does not admit a release.

## Admit and open one canonical Item

The File Item detail offers **Open App** only for the exact MIME. The frontend
captures the current identity/device, Space, File Item/version/SHA and title.
It obtains a mutation id from Core, asks `app_resource_find` for the canonical
existing Item, and calls `app_resource_install` only after an unambiguous miss.
Lookup failures and multiple matches are refusals, not a reason to install again.
Lookup includes authenticated historical release pins on the same App, so a cold
retry of an older package still finds the document after a renderer update.

Both commands accept an `input` shaped as:

```json
{
  "expected_identity_id": "<current identity>",
  "expected_device_id": "<current device>",
  "space_id": "<authorized Space>",
  "package_item_id": "<File Item>",
  "package_version": 1,
  "package_sha256": "<64 lowercase hex characters>",
  "title": "<document title>",
  "mutation_id": "<Core mutation id>"
}
```

Core validates the current owner/device, writer authority, source Space, exact
authenticated File version, MIME, hash, byte bounds and availability. Installation
creates the App, its encrypted document/UI chunks, its existing App grant and one
card message atomically. The App id derives from owner/device/Space/mutation;
retrying the same intent recovers that Item. No second installation ledger or
frontend list scan is an authority.

The owned resource URI is
`ui://zoen/package/<source-file-item>/<source-version>/<sha256>`. It is immutable
release provenance. The App retains its own encrypted UI FileDoc alongside its
document FileDoc; the source File need not remain the sole durable copy.

The frontend captures the Item id and resource URI for one focused editor
lifetime. These operations carry `item_id` and `expected_resource_uri`:

- `app_document_resource`: verify and read that Item's admitted UI bytes.
- `app_document_read_pinned`: read the document under the same UI pin.
- `app_document_save_pinned`: save with the observed `expected_version`,
  `expected_sha256`, `scene_json` and stable `mutation_id`.
- `app_document_copy_pinned`: copy the current scene with `scene_json` and a
  stable `mutation_id`, preserving the source and rekeying the new Item.

Core checks the pin and performs the read/write under the same lock. The guest's
six `app_scene_*` tools still use the existing bounded chunk and hash protocol;
they do not obtain an unscoped generic Core RPC capability. The scene validator,
membership, grant, version/SHA checks and durable receipt rules remain active.

## Update the renderer independently

The owner chooses another authorized package File from the same Space in
**Update App**. `app_resource_update` receives:

```json
{
  "input": {
    "admission": {
      "expected_identity_id": "<current identity>",
      "expected_device_id": "<current device>",
      "space_id": "<authorized Space>",
      "package_item_id": "<new package File Item>",
      "package_version": 1,
      "package_sha256": "<new UI SHA>",
      "title": "<current document title>",
      "mutation_id": "<fresh Core mutation id>"
    },
    "item_id": "<existing App Item>",
    "expected_version": 2,
    "expected_sha256": "<observed document SHA>",
    "expected_resource_uri": "<captured previous UI URI>"
  }
}
```

Core requires the original owner, current writer/App grant, exact title and
observed base. It versions the UI on the same Item while preserving the document
bytes, title and scene metadata. A changed base refuses the update. Lost response
recovery uses the existing bounded durable document receipts.

An already open frame retains its captured renderer and draft; a remote release
change does not reload it. Stale pinned reads and new save/copy mutations refuse
instead of silently switching to the newer renderer. An already accepted mutation
can still recover its durable receipt without a new effect after a release or
App-grant change; current owner/account and readable-scope checks still apply.
The editor's existing recovery choices
remain explicit. Background, focus or owner retirement closes the old authority
and prevents a late result from navigating in a new scope.

## Share and recover

The existing current-version sharing protocol carries the owned UI and document
with the same artifact consent and encrypted key distribution. A newly admitted
participant does not need historic MLS keys or access to the original package
File. Missing chunks remain unavailable until received; missing bytes alone must
not quarantine an otherwise valid authenticated event. Copying to another Space
rekeys both the UI and document through the existing file authority.

## Request and approve an agent edit

The owned Excalidraw tool family uses the existing signed request and approval
pipeline, with proposal schema 6. Its closed commands are `read_scene` and
`replace_scene`; the latter proposes a complete scene of at most 32 KiB against
the version, SHA and renderer pin returned by the read. It never updates the
renderer or admits arbitrary guest tools.

The normal CLI agent host discovers only the App authorized by the signed source.
An offered tool, a successful read or a model's `requires_review: false` does not
authorize a write. The native gate stages the exact held call as an ordinary
Request. The existing RequestCard and `decide_tool_request` record the owner's
`ApproveOnce` decision; Suggest access remains Suggest. The signed review detail
contains the frozen proposed scene, and cannot describe different proposed bytes.
The owned App's existing Agent sheet can show visual before/after previews when
its admitted renderer advertises `reviewPreview: 1`. Core's read-only
`scene_tool_review` returns the canonical base and exact signed replacement only
to the current owner/device, with the request hash, Space and renderer pin held.
It refuses changed bases, pins, grants, expired or resolved proposals.

The host sends those bytes to the same frame's read-only `render_scene_review`
MCP tool. The pinned SDK uses upstream Excalidraw's offscreen `exportToCanvas`;
it does not touch the open canvas, draft, save queue or observed head. Each PNG
is limited to 128 KiB and 896 pixels per side. The host checks the echoed
request/hash/pin/base, scene and image hashes, dimensions and a second Core read.
The native images use no persistent image cache. Approval becomes available
only after both images load, and retires on owner, focus or canonical changes.
Renderer output remains presentation; it does not grant an effect or replace
Core's exact-byte approval and publication checks.

Older renderer releases remain manually usable but cannot qualify this visual
approval. Close and reopen the App after an explicit renderer update. Native
WebKit acceptance of the new review UI is still separate from Core, React
Compiler, SDK and headless Chrome pixel evidence.

Execution rechecks the owner/device, source, membership, writer rights, App grant,
renderer pin and document base. An approved effect uses the same App document and
durable mutation receipt. Retry and cold continuation recover the accepted result
without a second version or effect. An expired review needs a current renewed
review; an older predecessor cannot shadow its approved successor.

The Core/CLI tests use real clients, signatures and the ordinary host with a
controlled model. They do not prove a real provider or native approval journey.
The earlier native632 build covers the preceding manual-editor contract and does
not contain this schema-6 rollout. Clients/runtime must be upgraded together;
older clients' handling of these requests remains unqualified.

For a real-provider run, use the normal
[`agent-host --source`](agent-source-hosting.md) selector with the exact new signed
edit-request message, never the App card or an unrelated backlog entry. Scene
commands execute in Core and require no binary editor worker. Omit all optional
binary-worker triplets and their cache directory from that process configuration;
configured workers otherwise initialize and compile their WASM at startup, even
when the selected source only needs a drawing. This does not skip provider, Gate,
permission or approval configuration. Do not start the unrestricted host to test
one App when its retained profile contains older unrelated work.

Generic package creation by an agent, additional document formats, native visual-review acceptance,
remote-tool credentials, distribution/registry and independent deployment remain
separate acceptance work under #180.
