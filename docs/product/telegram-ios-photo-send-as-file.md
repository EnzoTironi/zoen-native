# Photos: Send as File(s)

Photos More now exposes the pinned Telegram `Attachment_SendAsFile` /
`Attachment_SendAsFiles` action for a nonempty supported selection. It uses the
original Context Menu/File PDF and an explicit terminal send intent, as in
`MediaPickerScreen.swift:3174–3181`. Source paths, pin, hashes, license and
adaptations are in [the source inventory](telegram-ios-photo-send-as-file-sources.json).

## Canonical behavior

The native action captures its render revision and exact ordered selected IDs,
then reads the actual caption field when tapped. The host and JS adapter reject
stale, disabled, retired, foreign or malformed actions. The file intent is
recorded only after successful Photos export. Closing the native presenter still
must complete before the existing journal receives files; its accepted save
must precede Send. Canceling or failing export/save cannot carry file mode into
the next fresh ordinary selection. A rejected journal save retains its pending
draft and owned bytes for explicit retry/discard; it never silently drops that
draft to admit another selection.

This uses the existing typed `presentation: file` / `grouping: separate` draft
options, encrypted journal, CAS, signed message metadata and retry receipt.
There is no new draft store or transport. Actual CURRENT Photos bytes, MIME,
name, per-item captions, selected order, reply and destination remain governed
by the existing owners. File presentation does not relabel an image/video MIME
or promise an original/HD export. Ambiguous sends retry the accepted draft and
receipt without constructing a new selection.

The existing journal's presentation option applies to the whole batch. The
menu is therefore admitted only for a draft with no attachments or an existing
file-mode draft. Appending to an ordinary-media draft hides this action rather
than converting its previous items. The canonical draft revision is checked
again before accepting the file option. Ordinary selection retains the existing
media default; recovered file drafts retain their options through append,
reorder, caption edits and Send.

## Deliberate boundaries

This slice reuses the current bounded native UIMenu surface. It does not port
Telegram's ContextController menu animation. The upstream HEIC Keep/Convert to
JPEG chooser is not implemented; the existing Photos CURRENT export preserves
the real returned format. HD/original quality, grouping controls, spoilers,
scheduling, silent sends and paid media remain absent. Existing backend grouping
support does not imply that those UI paths are implemented.

The focused JS batch exercises the actual component/session/composer with native
ports, including final captions, ordered IDs, stale canonical revision, ordinary
draft rejection, recovery, failed/canceled requests and ambiguous-receipt retry.
The native tests exercise retained menu actions against actual content state.
An actual UIKit/glass/Lottie dependency-closure typecheck and Host parser check
are separate from executing those tests. The installed Release14 at `7b116e3f`
also passes a real Photos selection, final native caption, explicit More → Send
as File and confirmed JPEG message. Cold restart retains the caption and the
new signed file-mode recent row. The same artifact passes pending-PDF append,
repeated Add, editor cancellation and Gallery Add/File cancellation resumption.
The full Expo check passes 1,496 tests, types, lint and format; all 415 installed
files match the signed build. Evidence is under
`output/telegram-attachment-port-v242/presentation-proof/runtime-14-batch.json`.
This local single-account sender proof does not qualify independent recipient
delivery, HD/original conversion or Telegram menu animation/pixel parity.
