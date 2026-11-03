# Conversation attachment tabs

Implementation and remaining native acceptance are tracked in
[#173](https://github.com/EnzoTironi/zoen-native/issues/173). Message rendering is
tracked separately in [#172](https://github.com/EnzoTironi/zoen-native/issues/172).

The reference is Telegram iOS
`f1dd7a2dbd02cbbf513e75d5695d8d36d1cf5838`:
`AttachmentController.AttachmentButtonType`, `AttachmentPanel` and
`ChatControllerOpenAttachmentMenu` select the real child controller. In that pin,
`richText` is labeled Article, uses the original `TabArticle` animation, and opens
[`RichTextAttachmentScreen`](https://github.com/TelegramMessenger/Telegram-iOS/blob/f1dd7a2dbd02cbbf513e75d5695d8d36d1cf5838/submodules/TelegramUI/Components/RichTextAttachmentScreen/Sources/RichTextAttachmentScreen.swift).
That is a rich document editor, not a Markdown file picker. It can convert a
simple document to text/entities or send a richer InstantPage document.

Zoen retains its existing native gallery, Files/Audio providers and one attachment
presentation owner. Additional categories are offered only when an empty media
draft can yield safely. A selected Photo/recent file cannot be silently discarded
by changing categories. The route handoff waits for the exact native close;
background, cancellation, stale models and account/destination loss revoke it.
The original Article artwork is pinned in `TelegramAttachmentPresentation` with
its exact source and decompressed JSON hashes under the existing selected license.

- **Article:** the existing native Enriched Markdown editor provides real bold,
  italic, strike, heading, list and link actions plus its native format menu.
  Markdown/source mode preserves its serialized content. The conversation's same
  normal-message draft owns text, reply and thread. Typing does not send; Close
  captures and flushes, while Send captures the actual native value and consumes
  the canonical draft. A failed capture/save remains visible and retryable; leaving
  without that final input requires an explicit confirmation. The route retires
  before navigation so old native callbacks cannot repopulate an accepted draft.
- **Location:** the user explicitly requests a single foreground position or
  searches/selects a place, reviews its map and caption, and explicitly stages a
  GeoJSON file in the same encrypted media journal. Its frozen reply context is
  captured before GPS/search and survives a newer normal composer reply. Review
  and Send use the existing attachment owner; rendering never fetches location.
- **Contact:** the system picker requests only the selected contact's name and
  phones. Those exact editable fields are reviewed, then encoded as a bounded,
  escaped UTF-8 vCard and staged as a normal file through the existing encrypted
  media journal. Its exact cache copy is retained by the existing deferred-release
  owner until ingress reconciliation; retry flushes the same write. Review/Send
  and the received file use the existing attachment and file-viewer paths.
- **Poll:** the sheet edits one encrypted private draft in the existing media
  journal. Incomplete question/options survive close and cold reopen; the
  conversation offers Edit/Discard. Private stable option IDs prevent a retained
  text callback from changing another field after removal. An owned local,
  selected or cross-chat reply remains captured through editing and recovery;
  threads use the canonical reply contract. Explicit Create atomically installs
  the existing canonical single-choice `poll` App, its grants and one signed
  message with an acceptance receipt. A failed write retains private work, a
  refused source/writer can be revised, and an ambiguous attempt freezes the
  exact payload for explicit retry without duplicate installation. Neither
  restore nor render retries a send. Voting authority stays in the existing App.
  Inline Telegram poll presentation, anonymous/quiz/multiple votes and canonical
  voter identity remain gaps; current public voting uses display names, which
  may collide. These are not claimed as complete by this delivery (#741).

These are functional ports onto existing Zoen text/file/App contracts. Full
InstantPage blocks, embedded media/tables, live location, Telegram contact-message
bubbles, scanner, online audio/search and other conditional Telegram categories
remain unchecked in #173. Article input is bounded to64KiB UTF-8 without silent
truncation. A captured/saved normal draft can cold-reopen; this does not assert
crash durability for a native keystroke whose change/capture has not reached the
canonical draft. Poll publication reuses the existing media journal's durable
receipt and transaction rather than adding a new idempotency API. Contact/location
forms are transient until explicitly staged.

Source tests exercise the actual production React Compiler output and native
formatting method dispatch, canonical text/media controllers, cold draft reads,
reply/thread, lost ingress acknowledgments, retained callbacks, and real native
close-event handoff. They do not establish iPhone keyboard, picker, VoiceOver,
visual/animation parity or a complete editor acceptance journey. Those require the
final manual iPhone acceptance by Enzo. Current implementation proceeds through
automated tests, builds and CI only; manual/device QA is deferred by his request.
