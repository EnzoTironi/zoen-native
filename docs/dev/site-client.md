# Private Site source and explicit publication

The iOS Site presentation uses an ordinary encrypted `text/html` FileDoc in its
conversation. Create posts the file and attachment atomically through the existing
file-send receipt. Save uses `file_new_version_checked`; a changed base cannot be
overwritten. Historical source remains readable without restoring it. The private
editor's unsaved text is transient and is never a publication source.

The main presentation is a preview. Source editing is an advanced option, not a
replacement for agent creation or a reusable visual editor. Those journeys remain
open in #319. Public publication uses the separate service described in
[site-publication.md](site-publication.md).

## Source and consent

- Core requires the current registered human/device, unbroken encrypted-space
  membership, the exact Item/version/hash and verified complete UTF-8 bytes.
  Reading does not grant writing; current writers can explicitly publish an older
  version even when that old base cannot be edited.
- HTML is limited to 2 MiB. Missing bytes are downloaded explicitly; invalid
  lengths, chunk hashes, complete-file hashes, UTF-8 or NUL content are refused.
- Configuration comes only from the authenticated relay, without redirects. Its
  audience must match the live session. Management/public origins are canonical
  HTTPS, with loopback HTTP allowed for local development; public hosting must
  use a separate host. HTML and agents cannot choose these destinations.
- Read/status/review never sign a publication. Status uses a distinct signed
  inspection domain. The native confirmation shows the selected saved version
  and public URL. Dirty drafts, stale dialogs and changed accounts cannot submit.
- Prepare/Publish revalidate configuration, account, membership and selected
  bytes. Only this explicit action signs the shared mutation, including public
  origin and expected public head. The API sends the exact verified source bytes.
- Site JSON uses the shared protocol's bounded numeric fields directly. Other
  existing JSON commands retain their original integer encoding.

## Delivery recovery

`protocol::sites::site_id` deterministically separates owner, space, Item and
public origin. It is public identity, not authorization. Before a signed request
leaves Core, the exact random request ID and reviewed mutation are sealed into a
device-bound journal using the existing local sealing primitives. The journal
stores intent/receipt metadata, not HTML or another document. Values and total
retention are bounded; corrupt or exhausted storage fails closed.

A timeout or lost response keeps this intent. Recovery rechecks current access
and renews the device signature without changing the request ID or mutation.
Inspection can reconcile an accepted receipt after closing and unlocking again.
A definitive HTTP conflict permits a new review; ambiguous failures do not mint a
new request. Receipts must match the exact intent. Private versions remain the
FileDoc's authority; public history and compare-and-swap belong to the service.

## Preview boundary and remaining acceptance

The first preview is intentionally static. It uses a disposable, nonpersistent
WKWebView without an authenticated bridge, JavaScript, external resources,
navigation, popups or file-upload panels. A restrictive CSP precedes untrusted
HTML. The navigation policy admits one initial blank document and closes on
retirement. Closing the presentation or changing its owner destroys the WebView.

Core tests cover source integrity, permissions, checked saves, exact signatures,
bounded JSON, unpublish and journal recovery. Frontend tests cover late saves,
cancelled/changed consent, account epochs, receipts and real attachment routing.
The pure native navigation policy executes on macOS; Linux CI explicitly skips
that Apple-only test. SDK typechecking does not prove WebKit runtime behavior.

Do not mark #322 or #319 complete from these checks alone. The next signed iOS
batch still needs keyboard, VoiceOver, rotation, WebView rendering, memory and
close/reopen QA. Static preview does not establish interactive HTML/React/Wasm
support. Agent creation, reusable visual editing, durable unsaved source drafts,
other-platform presentations and publication-history browsing remain follow-ups.
