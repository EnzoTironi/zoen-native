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
is not a production vault. Paid execution, source synchronization, trigger/run
dedupe and guarded output still require their separate integration gates.

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

The paid retained-step loader, managed custody, actual relay synchronization,
stable trigger/run creation, worker/JetStream
recovery, typed tool approval/resume, signed output/usage, external continuity
witness, live billing reconciliation and deployment drills remain open. These
journeys do not complete runtime milestone 1 or Mastra/TextQL parity. See the
[capability ledger](../../docs/agent-runtime-capabilities.md).
