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

## Native device custody

`inspect_native_device(agent, device)` and `repack_native_device(agent, device)`
accept locators only. The private loader authenticates the complete capsule,
checks the exact real SQL profiles/device certificate, and restores the native
facade's strict supported schema. The returned inspection contains generation
and byte counts; it grants no relay currentness or model permission.

The device-wide SQLite image is at most 4 MiB: at most 64 raw 64 KiB chunks, each
at most 65,576 encrypted bytes. A separately encrypted manifest stays below 16 KiB.
Authenticated context binds namespace, owner, Agent, device, certificate digest,
key version, parent manifest, generation, image size/digest and chunk index/count.
The capsule domain and key are separate from financial evidence custody.

One stage pin per device bounds unfinished staging. Each bounded transaction
checks the real device fence. Only a complete ready stage can atomically replace
the exact previous root; root and pin reads conflict with activation and GC.

Maintenance is admitted at a known successful SQL directory commit, before the
FDB root write. That commit returns a private consuming proof bound to the full
execution namespace, deployment witness, owner/Agent/device/certificate digest,
complete target root, exact prior root and device lease holder/token. Its
monotonic two-second window starts before the final SQL clock query and commit;
SQL query/ACK latency consumes that window. FDB checks it before opening the
activation transaction and consumes it immediately before the only commit.
A dead, timed-out or uncertain SQL transaction returns no proof and cannot
advance the root. A root read cannot recover the proof.

A revocation committed before maintenance admission denies it. A maintenance
already admitted at the known SQL commit can finish in its finite window even
if revocation commits afterward; subsequent calls are denied. This is admission
semantics, not cross-store atomicity or retroactive cancellation. The local
deadline bounds actor use; it cannot guarantee when an uncertain FDB commit
becomes durable. Unknown/failed commits return no live workspace or execution
permission, and have no automatic retry. This proof cannot authorize paid
dispatch, a Space action or a fabricated run. Expired abandoned stages can be collected.
Committed older generations are retained for a version-clock window and while
referenced; at most 16 retired generations may coexist. A full retention budget
refuses activation instead of evicting potentially needed evidence. These FDB
version windows are operational bounds, not a trusted wall clock or restore witness.

Managed custody construction remains absent: `RuntimeAuthority::open` installs
no native credential/key owner. Test-only provisioning exercises this protocol
with genuine certified profiles and retained OpenMLS packages. A fixture key/map
is not a production vault. Paid execution and guarded output still require
their separate integration gates.

## Retained reply discovery and loading

`relay_cell` optionally identifies the actual relay cell on the same FDB
cluster. The existing Execution owner pins that cell in its namespace and
checks it in every transaction. Selection requires an empty FDB namespace
apart from the deployment marker; existing unbound native/run state is not
adopted. Changed or removed configuration refuses open. This identifies the
source store; it is not a restore-continuity witness.

`sync_reply_runs(agent, device, space)` accepts locators only. It restores an
authenticated certified device image and reads the real relay-owned log schema
with conflicts, through a concrete bounded source accessor. Each call hydrates
at most 64 entries and 4 MiB of content. Missing positions, rollback/mismatched
cursors, unsupported history and image bounds refuse progress. Partial catch-up
can retain an image but creates no reply run until its native frontier matches
the actual source head. The head is checked again inside the activation write.

The native core retains discovery position inside that complete authenticated
image and scans at most 64 events/16 eligible replies per call. It selects actual
opened owner messages under C1's supported Direct/Trust profile and SQL clock
observation. The runtime freezes their original request, exact gateway preflight
digest, price/policy snapshot and ReplyIntent. These are inert originals, not a
live spending approval. The configured input bound is not a tokenizer estimate.

One FDB transaction activates the image with sealed immutable run, attempt
Binding, original-image reference, trigger-dedupe and durable wake records.
The trigger key is Agent/Space/actual source-event hash, independent of device,
generation or wake delivery. Another certified device finds the same original
run and cannot replace its request, device or Binding. The known SQL maintenance
proof additionally binds the complete proposed source/journal/ciphertext effect
batch. A root-only maintenance proof cannot authorize a journal batch. Unknown
activation returns no native workspace/capability; a later scheduling scan can
observe the retained wake without recreating execution permission.

`pending_reply_runs(1..=64)` returns inert durable scheduling identifiers.
`inspect_reply_run(run)` exercises a private retained loader and returns those
identifiers only. The loader authenticates the sealed original, exact current
SQL enrollment and active native image, atomically acquires actual run and
associated device leases, and refreshes the original intent. Original input,
policy and Binding stay frozen while current native frontier/grant facts are
derived separately. An actual source snapshot checks that frontier, active
root, immutable association, both leases and prepared Binding. Reads mint no
dispatch permit. The private Native reply step keeps these current facts separate
from the frozen original and repeats the actual source/root/pair checks inside
the final admission write. Seeded CoreCapsule/copied-frontier authority exists
only in the explicitly separate test Fixture variant.
Lease release preserves monotonic tokens and cannot release a newer holder.

No-op synchronization consumes no image generation. Runs protect their original
capsule through real GC references; terminal reference release and wake ACKs
await a verified run disposition protocol. Production paid entry remains
closed by absent managed custody and external continuity. The joined SQL/OpenMLS/FdbLog fixture validates this unit separately from
financial core-scope fixtures; it does not implement live WS provisioning,
managed credentials, external nonrollback continuity or guarded agent output.

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
Guard finish rechecks the minimum signed budget, original period-end and actual
native grant expiry using the SQL clock. The
consuming permit has a conservative monotonic deadline, at most two seconds
from that observation and no later than either authorization deadline. A paused worker
cannot consume an expired permit; the admitted hold remains pending.

`cancel_model(run)` first records an authenticated permanent stop request under
the original run/attempt/record/Binding scope. It requires continuity and exact
retained association, prepared Binding, source pairing, trigger index and image
reference. Recording the request does not acquire or steal a worker lease and
grants no refund. The actual final admission reads the same key with a conflict.
If the original worker still owns either valid lease, cancellation returns
`ModelCancellation::Requested`; cleanup awaits its release or real expiry.

Fenced cleanup is a separate private type without a current native credential,
grant, image readiness, gateway quote, latest policy or live enrollment dependency.
It rechecks both actual leases and the exact request, then releases only after a
known permanent FDB pre-dispatch tombstone defeats old admission and the SQL
closure/release commits successfully. Replayed closure also writes the tombstone;
a read-only snapshot cannot mint a fresh closure proof. `Released` is this known
outcome; observed admission returns `Retained { financial }`. Admitted and unknown outcomes retain holds. Restore
continuity starts closed and has no production setter; absence or a namespace
UUID is insufficient. The actual external nonrollback witness remains missing.

The original SQL period exists before retention: policy installation creates it,
and reply discovery requires `reply_budget` to find that installed signed policy.
Its FK preserves the period. Cleanup never creates a policy or period. A permanent
`runtime_predispatch_closures` row binds original deployment/owner/period/Binding
under that period lock, even before an attempt exists. Reserve and claim use the
same lock and SQL enforces the gate, preventing an old prepare from reserving
after completed cleanup. Cancellation before reservation returns logical Released
without fabricating an attempt, hold or posting. While valid worker leases delay
cleanup, an old prepare may reserve temporarily but cannot pass final admission;
later cleanup releases that one hold. Automatic worker wake/recovery and terminal
reference/receipt pruning remain open.

Successful fenced FDB admission followed by known still-valid SQL finish defines
the finite authorization point. Later revocation/cancellation does not retroactively
revoke that already admitted permit; its deadline still prevents delayed use.
Incurred late evidence can settle the original bill and grants no output progress.
This is not distributed atomicity or instantaneous remote provider cancellation.

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

The capsule fixture separately exercises genuine retained OpenMLS key packages,
full authenticated image restoration, real SQL revocation and FDB root/pin/lease
conflicts. It creates no paid run or provider send. It does not prove a live relay
join or managed key service. Forced version-clock retirement and known-reply
suppression are explicit fixture cuts. Native C1's joined encrypted reply journeys
remain separate from this maintenance slice.

Maintenance admission tests use real SQL/FDB and two isolated device scenarios:
15 target/scope substitutions, suppression/delay of a known SQL commit ACK,
actual post-ACK expiry, and both revocation orderings. The public repack pauses
past the original five-second SQL idle timeout; the revocation commits before
resume, and the failed SQL admission must leave the root unchanged. This
reproduced generation 3 -> 4 on the pre-fix source. The post-admission scenario
checks the explicitly supported finite completion followed by refusal of new
calls. The ACK cuts do not test database wire-level commit-unknown behavior.

On macOS, Cargo's runner may omit the external FoundationDB client from its
dynamic library path. Build with `--lib --no-run`, then run that exact test
executable with `DYLD_LIBRARY_PATH` pointing at the configured FDB client library.

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

The paid retained-step admission, managed custody, live relay provisioning,
worker/JetStream
recovery, typed tool approval/resume, signed output/usage, external continuity
witness, live billing reconciliation and deployment drills remain open. These
journeys do not complete runtime milestone 1 or Mastra/TextQL parity. See the
[capability ledger](../../docs/agent-runtime-capabilities.md).

## C3 branch validation

The real-service runtime suite completes 2 Rust tests, including 26 new named
native admission/cancellation outcomes, all 12 retained-discovery outcomes and
all 30 financial regressions. It uses actual Postgres, FoundationDB, OpenMLS,
relay FdbLog and loopback HTTP with explicit fixture custody. The real
60,000,000-version lease expiry is observed without changing that bound.
A separate forced-expiry takeover cut remains explicitly synthetic. Known
commit ACK suppression is not wire-level database commit-unknown testing.

Independent review found no blocker in the production delta; it did not execute
these services independently. One local fixture bootstrap returned Unavailable;
the identical binary passed on repetition, and the cause remains unproved.
The implementation PR binds its exact source, binary and raw-log hashes and
retains the unsuccessful attempts. Full integration CI on the refreshed stack,
managed custody, external nonrollback continuity, guarded publication and
automatic worker/wake cleanup remain open. This is not backend completion or
full Mastra/TextQL parity.
