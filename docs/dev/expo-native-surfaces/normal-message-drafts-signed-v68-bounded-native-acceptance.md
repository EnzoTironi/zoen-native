[Native surface index](../expo-native-surfaces.md).

## Normal message drafts — signed v68, bounded native acceptance

The v67 Rust implementation keeps ordinary text and reply targets in the existing
encrypted device-local store, scoped to exact account/device/chat/thread. CAS
revisions and tombstones fence stale writers; signed enqueue consumes only the
matching draft atomically and retains an idempotent accepted receipt. Drafts are
excluded from backup export/new-device restore. The application separates normal
compose from edit/search/caption/audio, flushes close and cancels erasure writes.
Swift `DraftRejected` is definitive only for the current unaccepted revision's
pre-append target/writer/empty checks after receipt lookup. Owner/storage/CAS/
response-encoding failures remain ambiguous and retain retry identity/text.
Those guards and backup/erase behavior have source proof, not induced native-failure
acceptance. Full FFI 441/441 and scoped strict Clippy retain the v67 freeze scope.

Signed **v67** source `c1d48a59` supplies the first diagnostic evidence: an exact
47-byte Unicode/newline main draft survives route return/cold, the ordinary MLS
peer observes zero native sends while unsent then one exact signed main send with
Delivered, and simultaneous main/thread unsent drafts stay isolated. The run also
exposes an empty field retaining its old multiline height after Send. Its partial
36-step diagnostic has 12 cross-tree warnings and one unmet paste-shortcut wait;
native Colar succeeds afterward, with one additional unrecorded middle-dot keyboard
tool error. The retired external one-use YAML is not a formal replay.

Signed **v68** source `90d67d87c0949d517be0d99df272f428f75c9bdd` fixes empty-field
height in two JavaScript files, reusing the unchanged Rust/native core. App 508/508,
TypeScript, Oxlint and Oxfmt pass. Exact public executable/JavaScript/strict signature
checks pass before and after UI on both devices; the post-runtime 788-input manifest
is stable, SHA256 `6ca95fec0992c4c704ed96d686d50907584b5f1a0f2fb0ebf66dedabed03b5c4`.
Actual bounded v68 results:

- The pending v67 thread draft restores cold and sends with keyboard focus and
  exact seed binding. The emptied native field is 52 points and retains focus.
- Quoted text and seed target survive cold. A separate search query opens/closes
  without changing that draft/quote, and entering Edit then Cancel restores them.
  The quoted send reaches the independent peer with the exact target/text.
- After sending a temporary target, staging a reply and deleting the target through
  native UI, Send rejects with a localized message and preserves the draft text.
  Clearing the quote permits one corrected send without a duplicate. The cached
  quote preview itself is still stale after deletion, as noted below.
- The ordinary MLS peer verifies cumulative five native posts, including the deleted
  target tombstone/four live posts, in a valid 15-event signed log. This count includes
  the earlier v67 main send. After one explicit Read, the peer exits successfully
  and discards volatile state. A cold sender launch retains Read labels and a
  consumed, empty 52-point field with closed keyboard and idle audio.

The partial v68 recording has 24 steps plus 27 direct actions and five cross-tree
warnings. Its YAML is retired externally and PNGs are copied with hash manifests;
no formal reusable replay is claimed. Human v66→v68 delivery uses the same accepted
archive, preserving data without uninstall/reset/erase. First ordinary cold Home
retains four cards and recipe 6/8; Chats opens existing Bilingual history at 100%,
with prior Delivered text/voice, empty 52-point field, closed keyboard and idle audio.
No human content or preference changes. Its partial recorder has four steps plus
two direct actions; its retired external YAML is also not a formal replay.

Three narrow visual follow-ups remain: keyboard-open search found two results but
initially placed matching bubbles behind the keyboard; multiline Edit→nonempty
one-line compose retained a taller field despite the fixed empty case; active quote
kept its cached preview after target deletion although the core safely rejected Send.
Kill before an unacknowledged write, induced native storage/owner/erase failures,
unsent attachment/audio/PDF process-death recovery, physical/provider acceptance,
full accessibility/performance and comparative parity remain unproven. All earlier
v66 objects, histories and gaps are preserved verbatim. Earlier v29 encrypted media
and v66 attachment/caption/video proofs are not attributed to this draft journey.

Build history is retained: the first v67 Swift compile failed (exit 65, missing
`DraftRejected` switch case); the first v68 attempt hit the disk guard (wrapper
exit 90, child receipt -15). Final v68 Xcode passes in 33.25 seconds, minimum free
9,639,485,440 bytes. Cache cleanup unlinked host binaries while relay/peer stayed
alive; the files were rebuilt without executing/restarting processes, with identical
helper hash and unchanged sources/locks. Six scoped tooling services were stopped
for both devices; the relay was untouched. These operational recoveries are not
blanket build-success, CI or product-parity claims.

Evidence: `/private/tmp/zoen-message-draft-qa-v68/evidence-manifest.json` contains
80 hashed files, including isolated `runtime-v68.json`, human preserving runtime/
install, final public-bundle checks and both screenshot manifests in that directory.
Artifact proof is `/private/tmp/zoen-iphone-release-v68-verification.json`; source
checks are `/private/tmp/zoen-v68-app-check.log` and
`/private/tmp/zoen-normal-message-drafts-v67/freeze.json`. Historical diagnostics:
`/private/tmp/zoen-message-draft-qa-v67/runtime-v67-diagnostic.json`,
`/private/tmp/zoen-v67-xcode-build-attempt1.{json,log}` and
`/private/tmp/zoen-v68-xcode-build-attempt1-disk.{json,log}`. Final build receipt is
`/private/tmp/zoen-v68-xcode-build.json`; binary recovery is
`/private/tmp/zoen-dev-binary-restore-v68/restore.json`. PR 136 stays open/draft.
