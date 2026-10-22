[Native surface index](../expo-native-surfaces.md).

## Attachment batches and captions — signed v66, bounded native acceptance

The native attachment dropdown opens the ordinary Photos/document picker. Up to ten
assets, within a 64 MiB aggregate bound, return to a compact inline draft above the real
message field. Selecting a thumbnail changes that asset's caption; the normal text
draft remains separate while the same composer is mounted. File and caption commit
atomically in the core, using a stable mutation intent per item. Accepted items are
skipped on retry and an attempted caption, bytes and destination remain frozen after
an ambiguous failure. This does not add an album event or durable unsent-draft recovery.

Signed **v65** source `7e8b0e5b` first proved the main-chat picker/send journey: ordered
photo and video, independent Unicode captions, restoration of the prior text draft,
and a nonempty caption edit. A normally registered encrypted peer verified the exact
captions, signed manifests and decrypted bytes; both received files fully decoded.
That same run exposed two failures: clearing a caption was rejected by the core's
empty-message guard, and video extraction ran before its asynchronous source loaded.
These are preserved diagnostic failures, rather than v65 acceptance claims.

Signed **v66** fixes both. Only an own canonical ordinary File message in the same
conversation may accept a blank caption, under the existing author/deleted/member
guards and 16 KiB UTF-8/no-NUL bound. Plain, voice, miniapp, deleted and foreign-chat
empty edits remain refused. Draft video extraction waits for public SDK58 readiness,
with post-subscription state sampling, a 15-second deadline, cancellation cleanup and
one serial decoder. Small selections now use the shared localized B/KB/MB formatter.
The actual previous core fails the new empty-caption regression; the final source
passes 17 focused file-message tests within the complete 427-test FFI suite. Six
thumbnail lifecycle checks are included in the 482-test application suite.

On the exact signed v66 app, native Edit → clear → Save reaches the independent peer
without changing the original file/message identities, version or bytes. A cold
restart retains the empty caption and both media. Swiping right on the fixed seed
opens its thread. Photos selects photo then video; both appear inline, including a
real video thumbnail, readable `8,3 KB` size and the captured root. Separate captions
survive selection changes and Send creates exactly two seed-bound thread file posts.
The peer verifies all four cumulative attachments, exact captions/targets/order,
valid signed log, immutable version-one manifests and matching bounded decrypted
bytes. All four explicitly exported synthetic files fully decode. An explicit Read
request is observed on the sender and retained by a further cold main-chat launch
after the peer exits and discards its volatile keys. Its namespace is consumed.

The 90-step isolated diagnostic has 15 cross-tree warnings and no unmet wait. The
raw one-use recording is retired; no formal reusable replay is claimed. The same
archive is then preserving-installed on the human QA simulator without uninstall,
reset or erase. Its first cold Home retains four cards and recipe 6/8; Chats opens
existing Bilingual history at 100%, with the card rail, existing Delivered voice,
empty composer, closed keyboard and idle audio. That nine-step diagnostic has three
cross-tree warnings and no unmet wait. No human content or preference is changed.

Native induced send/cleanup failures, stale picker/account changes, ten-item stress,
durable draft recovery, complete album/media parity, physical/provider acceptance,
full accessibility and measured motion/performance remain outside this proof.
Web and Android runtime acceptance remain deferred. Exact v64 artifact objects and
all prior evidence/gaps are preserved in the acceptance ledger.

Evidence: `/private/tmp/zoen-attachment-batches-v66/{runtime,human-runtime}.json`,
its evidence manifest and full-decode receipt,
`/private/tmp/zoen-iphone-release-v66-verification.json`,
`/private/tmp/zoen-v66-app-check.log`,
`/private/tmp/zoen-file-caption-clear-v66/freeze.json`,
`/private/tmp/zoen-draft-thumbnail-v66/checks.json`, and the retained v65 diagnostic
at `/private/tmp/zoen-attachment-caption-v65/native/runtime-diagnostic.json`.
