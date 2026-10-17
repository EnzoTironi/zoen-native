# Browser authority and encrypted device history

The Expo Web Devices screen can authorize a device when this browser created the
account and restored its protected primary authority. A browser linked by another
device cannot acquire that authority by opening the screen. It directs the person
to their principal device instead.

## Authorization

The person pastes the new device's existing `zoen-link:1:` code, compares the six
check digits on both screens, then confirms authorization. `BrowserPrimary` stays
inside the worker. Rust signs the recipient device certificate and seals the
identity box with the shared `roda-link` HPKE/PSK codec. Temporary encoded identity
secrets are zeroized. The root seed never enters a client snapshot or a plaintext
worker result.

Before any delivery frame leaves the browser, the existing encrypted IndexedDB
client snapshot saves the ciphertext, certificate, peer keys and stable request ID.
A cold retry of the same code reuses that ciphertext. A late acknowledgement from
an earlier request cannot authorize a subsequent recipient. Once the relay
acknowledges delivery, the journal records the certified recipient and queues the
normal signed `DeviceJoining` events for encrypted groups. Existing MLS membership
reconciliation supplies the recipient's Welcome messages.

## Initial history

After the groups are ready, the person chooses **Enviar histórico**. Rust selects
the recent opened window (up to 500 events per Space and a 64 MiB total), encrypts
1 MiB chunks with a fresh transfer key and retains the exact ciphertext in the
same encrypted client journal. The host performs device-signed HTTP PUTs only
after that journal is durable, with a 15-second timeout per PUT. It yields the
canonical queue between chunks. Network failures retry retained bytes rather than
regenerating a key/nonce pair. Cold startup resumes the retained transfer.

Each successful PUT is journaled. Only after every chunk is acknowledged does
Rust publish the HPKE-encrypted history manifest through the existing relay link
mailbox. The sender releases its retained chunks after the manifest acknowledgement and
emits the existing authenticated `HistoryReady` wake hint. The receiver's pending
marker and foreground/reconnect resume remain fallback when that hint is lost.
Both chunk work and manifest publication reject a recipient whose device has been
revoked. The receiving device still verifies/imports the manifest and signed outer
history records through its existing history receiver. The sender's successful
result means the relay accepted the manifest, not that the destination displayed
or read the messages.

## Older pages

A linked browser requests an older page from its pinned principal device using
`roda-link::DeviceMsg`. The pending request ID and exact sequence boundary survive
a restart. Replies must match the principal, recipient, request and Space. Every
opened record must match the already verified signed relay record before the
atomic import replaces any ciphertext. Completion is retained in the encrypted
client snapshot. An offline principal produces a recoverable UI message.

Device messages are bounded to the relay's 1 MiB budget. When a single opened
event cannot fit after serialization, the browser surfaces an explicit page-size
failure and stops automatic retries. It does not mark that Space complete. A
chunked protocol for such individual older events remains a future extension; the
native older-page receiver currently has its existing behavior for an empty
`Page { more: true }` response.

## Validation boundary

`zoen-web-core` tests exercise stable identity ciphertext across reload, certificate
acknowledgements, stale acknowledgement rejection, exact transfer ciphertext and
signed PUT headers, native-codec manifest/chunk opening, revocation before manifest
publication, peer/request/Space correlation, verified opened imports and bounded
oversized-page failure. Browser controller tests check persistence before PUT and
that a failed encrypted save never uploads.

A real browser-primary to native-device UI walkthrough, multi-chunk interruption
across a real browser restart, and visual confirmation on both devices are still
required before claiming this journey fully validated. Provider login, backup
recovery, release signing, and production relay deployment are separate acceptance
items.
