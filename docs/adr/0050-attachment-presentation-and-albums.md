# 0050 — Signed attachment presentation and explicit albums

Status: implementation for review; native UI enablement is a separate integration step.

An attachment remains one canonical File Item and one MessagePosted event. MIME,
name, SHA-256, encrypted chunks and file versions describe the actual bytes. A
message may additionally carry signed `media` metadata: `presentation` is `media`
or `file`; optional `group` contains an album id, zero-based index and count.
The absent field retains the exact legacy event encoding and automatic rendering.

Album identity is derived by Rust from a domain tag, author, space, reply/thread
scope and the entire ordered list of immutable attachment mutation ids. The group
is not inferred from adjacent messages or timestamps. Each original message id
continues to own its replies, receipts, reactions, edits and deletion. Forwarding
one member preserves presentation while clearing album membership. A UI groups
only a complete, consistent, authorized set; partial arrival remains individual
messages and never hides canonical content.

The existing encrypted media draft journal stores optional `options` containing
both `presentation` and `grouping` (`separate` or `grouped`). Both default values
are normalized by omitting options, preserving old save receipts. Grouped sends
require 2–10 items. The same revision/CAS, attempted flag, authenticated source
hashes, mutation receipts and atomic rollback apply to options and bytes. The
native send transaction writes every member together; failures cannot accept a
partial album or change a frozen retry into another presentation.

The browser uses the existing encrypted snapshot/outbox and producer leases. A
batch request carries bounded JSON metadata plus binary length-prefixed sealed
frames. It validates the full selection and every lease before one existing domain
batch commit. Each receipt is bound to the full ordered batch intent, and all events
share the same upload barrier. A fresh encryption of identical bytes can recover
an accepted retry; reordered items, changed captions, changed options, mixed old/new
leases and foreign destinations cannot reuse accepted intent.

No quality selector is enabled by this contract. The Photos adapter currently
exports CURRENT (including Photos edits); slow-motion compositions may be exported
by the SDK using HighestQuality MOV. The bounded image editor renders up to 4096px
and emits PNG. None of these imply bit-exact original bytes or an HD preset. Real
quality options require a measured transform and validated output committed through
the same canonical journal before they can be advertised.

Compatibility: new optional event metadata is omitted for legacy sends. Existing
single-file fingerprints remain unchanged when options are default. New metadata
uses a domain-separated fingerprint. No extra draft store or durable UI authority
is introduced. Signed membership is scoped by author and conversation; group
validation rejects invalid counts/indices, duplicate membership, incompatible
presentation/targets and foreign files.

Failed local sends remain visible after membership revocation through a separate
recovery projection of the signed local outbox row and its matching signed file
source. This path grants neither writer authority nor album membership and cannot
make recovered-only files eligible for a new attachment claim. Received metadata
uses the same ordered group cursor on native and Web. A shared signed fixture
covers reversed indices, duplicates, count/presentation/reply drift and forwarded
album claims; invalid events remain in history without gaining a message view.

Metadata-bearing outbox messages wait for an authoritative same-Space non-voice
File in the verified, relay-confirmed log. Grouped messages also wait for the
same-author File creation and exact previous group membership. Optimistic state,
a socket write and `Accepted` do not release those dependencies. A disposable
per-offer snapshot uses the existing canonical `PlanReplay`/`State.apply_checked`
fold and dependency predicate; signature-valid but projection-refused history
cannot replace a File or advance the group cursor. Voice and legacy messages
without media metadata keep their existing transport behavior.

This replays each relevant Space once per offer pass, with temporary state
proportional to that confirmed history. Existing merged `state` and
`space_projections` contain the pending overlay and cannot safely replace this
confirmed-only fold. There is no persistent cache or additional durable queue;
large-history latency and memory have not been performance-qualified. A held
message also holds later heads in that Space, so sealed generations cannot
overtake it beyond the existing MLS window. Other Spaces remain independent.
Retrying after a transient refusal or cold reopen preserves the original
ciphertext, client ids, order and journal receipt.

A terminal refusal atomically marks the exact signed matching group's pending
members failed. A refused unconfirmed File creation also fails its same-author,
same-Space metadata-bearing references and their matching groups. Matching binds
group id/count, presentation, reply, author and Space; invalid signatures and
forwarded group claims cannot expand it. A refused duplicate creation cannot
invalidate references to an already confirmed Item. Failed signed intents and
readable private bytes remain available through the existing recovery projection,
while newer ordinary heads can progress. The actual network refusal path flushes
those newly released heads immediately, without waiting for incidental traffic.
Source bytes/File creation rows unrelated
to the failed message group are retained, not deleted. This adds no public retry
or discard API; a user-facing failed-send recovery affordance remains separate
work, and confirmed-message deletion semantics are unchanged.

Recovery-only identities are reserved from authenticated failed outbox sources
before confirmed history is folded. This also rejects legacy messages without
`media` metadata and signed version/revert events, including after reopening;
later insertion of readable recovery bytes cannot grant them authority. File
editing affordances and mutations reject those targets before writing chunks or
keys. A validated File creation in the canonical projection clears the derived
reservation. An explicit copy of readable bytes creates a fresh File through the
existing writer checks and remains supported.
