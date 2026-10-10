# Zoen runtime authority

This library implements the Postgres financial ledger and fenced FoundationDB
dispatch protocol. Public `run_model` and `cancel_model` remain closed with
`CoreAuthorityUnavailable` until they load a genuine retained certified MLS/core
step. Verified steps have no public constructor, deserializer or injected verifier.
The host owns the process-wide FoundationDB network. Apply the relay migrations,
including `0025_model_financial_authority.sql` and
`0026_runtime_deployment_binding.sql`, before opening the library.

One Postgres financial database is paired with one execution namespace and
evidence-key fingerprint. An immutable SQL singleton and an independent FDB
marker retain that pairing. Open refuses another namespace/key, a missing or
different FDB marker, or any unbound retained attempts. Every execution
transaction reads the pinned marker with conflicts. Initial pairing requires
empty stores and known bounded commits; an orphan after an uncertain commit
needs explicit reconciliation and is never silently adopted. Matching workers
can reopen concurrently. This marker identifies stores only: it does not prove
nonrollback continuity or permit paid work/refunds after restore.

## Dispatch and finance

`roda-log::owner_budget` signs bounded owner-period policy bytes with a certified
device. SQL installation requires a live Person owner and active exact device.
An Agent cannot issue a budget. Policies approve specific profile digests and
USD micro-unit limits, expiry and a contiguous version chain. They do not convert
legacy proposal cents or enrollment into spending authority.

A price profile pins endpoint, model, credential reference, integer rates and
finite input/output/transport bounds. Gateway preflight supplies actual encoded
request size and configured bounds; mismatches deny before reservation or HTTP.
The input bound is host supplied, not a token estimate or a live tariff proof.
Cache/reasoning differentials, currency conversion and catalog fallback are unsupported.

The dispatch sequence prepares a sealed FDB binding, reserves under the SQL
owner-period lock, commits one unique SQL claim, holds a short shared directory
guard during one manual FDB admission, then consumes that fresh admission by
committing its SQL witness and ending the guard. Only this known-fresh sequence
produces a transient permit for one gateway transport. No SQL directory lock
spans HTTP latency. Reads, replay, reopen and uncertain commits cannot recreate
claims or permits. Both run and certified-device fences guard execution writes.
Guard finish rechecks signed budget expiry using the actual SQL clock. The
consuming permit has a conservative monotonic deadline, at most two seconds
from that observation and no later than the policy deadline. A paused worker
cannot consume an expired permit; the admitted hold remains pending.

Cancellation releases only after a permanent fenced FDB pre-dispatch tombstone
defeats old admission. Admitted and unknown outcomes retain holds. Restore
continuity starts closed and has no production setter; absence or a namespace
UUID is insufficient. The actual external nonrollback witness remains missing.

Immutable exact balanced postings record reservation and exclusive terminal
release/settlement. Deferred SQL constraints enforce journal shape, reference
binding and aggregate/posting equality. Lowered limits preserve existing
obligations. Supported usage overruns post the whole charge rather than hiding
expense at the hold ceiling.

## Evidence and recovery

Only the actual gateway collector supplies bill evidence. Invalid output does
not discard it. This profile settles complete successful responses with the
configured reported model and directly present consistent input/output usage.
Missing, invalid or unsupported usage stays uncertain; only directly reported
zero becomes zero. Captured evidence is not a provider-signed invoice.

XChaCha20-Poly1305 seals retained content with namespace/subject authentication.
This slice caps requests at 32 KiB and responses at 16 KiB; sealed captures must
fit 96 KiB. SQL and FDB independently retain evidence. Late capture may settle
after revocation or lease loss, but cannot authorize progress or publication.
`reconcile(1..=64)` transfers evidence and financial references through missing
per-entry acknowledgements. It never calls providers or publishes agent output.

## Verification and open integration

Run `cargo test -p zoen-runtime --lib -- --nocapture` with `ZOEN_TEST_PG` and
`FDB_CLUSTER_FILE` pointing at isolated real services. One Rust test owns the FDB
network and executes named journeys on fresh Postgres databases, isolated FDB
prefixes and actual local TCP/HTTP. Directory profiles, certificates and owner
proofs are real. Core scopes and continuity are explicitly seeded test authority.

Journeys cover owner concurrency, policy/directory refusal, balanced immutable
postings, profile mismatch, changed limits, direct zero/missing usage, overruns,
invalid output, replay/reopen, both forced admission/closure orderings and races,
cancellation, revocation during paused HTTP, stale device fences, SQL pool
unavailability, reconciliation, out-of-order acknowledgements, declared restore
refusal and sealed-store canaries. Suppressed acknowledgements follow actual
commits; they do not prove DB wire-level commit-unknown behavior. Declared restore
tests do not detect rollback automatically.

Store-pairing journeys reproduce the unsafe cross-namespace refund with real
paused HTTP, then verify the rejection and complete charge. Concurrent initial
opens and reopens, changed keys/markers, immutable SQL pairing, unbound legacy
attempts, and an actual deferred SQL commit failure are exercised. That failure
leaves the independently committed FDB marker orphaned and closed. It is a real
commit rejection, not DB wire-level commit-unknown testing.

The real core/MLS loader, chunked device capsule and custody, worker/JetStream
recovery, typed tool approval/resume, signed output/usage, external continuity
witness, live billing reconciliation and deployment drills remain open. These
journeys do not complete runtime milestone 1 or Mastra/TextQL parity. See the
[capability ledger](../../docs/agent-runtime-capabilities.md).
