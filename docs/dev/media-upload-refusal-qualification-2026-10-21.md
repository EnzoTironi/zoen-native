# Attachment publication after actual upload

Base: `b5bec14489ffaea63e79c5b82044c46dd362e911`. Executed negative:
`0b07872c`. Initial implementation: `35a8a158`. Final production/test
follow-ups: `b5179cbb` and `f7f2e6b8619713ad3c283c5b6b912c3c1d2822bc`.
Receipts and preserved diagnostics:
`/Users/enzotironi/.codex/builds/zoen-media-upload-refusal-v262/`.

## Confirmed defect and bounded repair

Public `save_media_draft` / `send_media_draft`, the actual HTTP media worker,
the actual authenticated WebSocket session, relay `Accepted` / ordered `Event`,
and independently reopened sender/receiver reproduced both HTTP 400 and 413.
The original sender displayed `Sent`; the cold receiver retained the message;
the only pending encrypted upload had been deleted. The assertion refusing
confirmed remote delivery without recoverable bytes failed in `red-05.log`.
Earlier logs are explicitly fixture setup/compile diagnostics, not additional
functional negative controls.

`blob_uploads` remains the encrypted upload journal. Its additive `failed` column
retains refused ciphertext while excluding that row from automatic retries.
Absence is not a newly invented remote receipt. Existing rows migrate with
`failed = 0`; already deleted historical rows cannot be reconstructed or reliably
classified by this migration.

The existing outbox now waits for local queued uploads before offering a File
creation/version or a message referencing that File. Transient failures preserve
the exact signed intent, CID and encrypted upload. Real HTTP success uses the
existing `upload_done` and network wake. Other Spaces still publish independently;
same-Space ordering preserves the existing MLS generation window.

Terminal upload refusal and dependent outbox failures commit in one SQLite
transaction. The existing signed group matching fails the affected messages and
their exact group. An unconfirmed, unused File creation in that aborted group is
also failed if its upload is still pending, so it cannot strand later ordinary
heads. A canonical File, a File whose upload has already completed (and can be in
flight), or a File referenced by another pending intent retains its own fate.
The normal failed-message projection keeps captions, options, group identity and
local attachment bytes readable after cold reopen, including default messages
without explicit media metadata. Their signed local source is reserved through
the same display-only recovery fence; new legacy claims, versions and reverts
remain refused before any SQLite write. Listener invalidation follows
the durable transition. MLS recovery retains its separate existing requirement:
a refusal does not acknowledge its context upload or release its commit.

The public send result still means local durable acceptance, not remote delivery.
Replaying its accepted receipt returns the same failed message IDs; it does not
silently regenerate successful sends. An explicitly created new draft can copy
the retained bytes into a fresh authorized File/key. No recovery-only File gains
publication authority and no retry/discard API is added here.

## Executed qualification

- The original 400 and 413 cases failed on the stated remote-delivery assertion.
- Seven new production tests cover terminal 400, terminal 413 with a two-member
  group, transient 503 with cold restart, injected SQLite refusal failure,
  default-media retention without explicit options, preservation of a refused
  canonical App card, and a public voice draft refused over actual HTTP.
- Both terminal journeys retain exact ciphertext and readable failed messages,
  expose no matching remote messages, and allow a later ordinary message through.
- The transient journey observes a second PUT from the same worker after 503,
  proving that its transient branch actually executed. That request is held and
  interrupted; the cold restart makes a third identical PUT. It replays the
  original public receipt without duplicate intent, verifies no publication
  before HTTP 200 using an ordered wire marker,
  retries the same ciphertext, and confirms one message. A cold receiver then
  retrieves the ciphertext via real HTTP GET, verifies/decrypts it through
  `media_arrived`, and reconstructs the exact File bytes.
- The rollback test fails outbox storage inside the refusal transaction; upload
  state, signatures, attempts and message state remain unchanged. Its successful
  retry deliberately leaves the second group blob unacknowledged and proves that
  the orphan File does not block the later ordinary head.
- An additive old-schema migration/transaction test and existing MLS recovery
  upload regression pass. Existing media ordering fixtures now explicitly
  acknowledge their upload prerequisite; their substantive ordered/cold replay
  assertions remain intact, and real HTTP is tested separately.
- Independent review identified a second executed negative: without explicit
  media options the cold failed message retained its caption but lost its card
  (`red-default-01.log`, test commit `d4a83695`). The final display-only repair
  preserves the original card and refuses new claims/version/revert without writes.
- A canonical non-File App-message compatibility test exposed a follow-up routing
  regression (`red-nonfile-01.log`): terminal rejection made that message disappear.
  If File recovery does not retain a default message, it again takes the original
  checked projection path. Explicit metadata admission remains unchanged.
- Public voice `send_media_draft` with a valid 250ms PCM WAV, actual HTTP 400 and
  cold reopen preserved the exact voice DTO (transcript, duration, waveform,
  hash/item), local bytes and Failed state. No additional voice production change
  was needed. This does not qualify hardware playback. The first 100ms fixture
  was correctly refused by the existing 250ms minimum before any upload;
  `voice-01.log` is fixture admission evidence, not a product failure.
- Scoped `media_` tests: 55 passed after the final production routing correction.
  All seven final upload/compatibility cases passed after the voice fixture was
  added. These counts overlap; they are not separate totals. Store sync tests: 4 passed. Explicit MLS recovery
  prerequisite test: 1 passed. Strict Clippy for `store` and `client-core`, all
  targets, `--no-deps -D warnings`: passed. Final logs and source hashes are sealed
  separately. One complete client-core library run passed 629/629 in 168 seconds
  on the initial implementation `35a8a158`; it is not relabeled as a full-suite
  run of the default-recovery follow-up. Final follow-up qualification is the
  scoped 55-case run, seven final upload/compatibility cases and strict Clippy. No npm,
  native build, device or production service was used here.

## Reference and limits

Read the assertions in [Signal iOS attachment upload tests at the pinned revision](https://github.com/signalapp/Signal-iOS/blob/d13b9da9e4622e16297b111243755c4c2972091a/SignalServiceKit/tests/Network/Upload/AttachmentUploadManagerTests.swift#L320-L436).
They distinguish reported local progress from successful uploaded state and cover
restart/resume and reuse. No Signal implementation, retry count or delay was
copied; its tests use mocked transport and were not executed here.

The HTTP/WS relay is a bounded local test fixture, not the deployed
PostgreSQL/FDB/NATS/blob stack. The main combined fixture uses a Closed Space;
the retained existing WS ordering cases exercise real MLS. HTTP worker and WS
session are actual production components with separate fixture endpoints; the
fixture installs the same upload-to-flush wake used by `Net::start`. A successful
relay HTTP response is trusted according to the existing blob contract: crash,
power-loss durability, object-store loss and post-upload remote deletion are not
qualified. The successful cold GET does not execute the separate download-worker
retry policy. Ordinary success without a preceding transient fault is covered by
the same HTTP 200 completion path, not a separate extra journey.

The publication gate here resolves File content, including legacy File messages
and voice File creation. It is not a complete audit of Page/App/background/profile
blob lifecycle or of File-revert publication. Terminal upload/failed outbox bytes
remain retained; physical deletion/GC and user recovery/discard affordances are
separate work. Shared orphan blobs can still have active upload work, but the
aborted File event no longer blocks later ordinary publication. Queue size,
per-offer chunk lookups and terminal confirmed-history replay are unbenchmarked.

The next two audited contracts remain open: explicit user recovery/discard of a
failed signed send, and an explicit download retry clearing the same worker's
existing backoff without changing account/version/hash authority. They are not
implemented or implied by this batch. Historical Signal matrix classifications
and product readiness flags remain unchanged.
