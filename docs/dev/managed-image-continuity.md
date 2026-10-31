# Managed Images continuity

Tracked by #380, under #366/#325. Images provider dispatch and original
publication use independent checkpoints. General reply output and global
continuity remain closed. The separate [Images-only output constructor](managed-image-output-scope.md)
can qualify a fixed certified managed device and route; its combined actual
host journey is not yet accepted. Construction of an anchor alone grants no
output authority.

The deployment is initially one cooperating host on a local Unix filesystem.
The owned anchor directory must be explicitly excluded from PostgreSQL/FDB
backup and restore. Restoring all three together defeats this detector. NFS,
distributed workers, privileged modification of the directory, and replacement
of stable lock inodes are outside this contract. There is no operator Boolean
that turns these assumptions into permission.

## Construction

`RuntimeAuthority::open_with_native_device` remains the certified device/vault
boundary. `initialize_managed_image_continuity` then accepts only an unused
paired execution namespace and zero financial attempts. Initialize it before
`initialize_native_device`, key packages, invitations and source catch-up.
The pair and device are derived from existing private SQL/FDB/native authority.
The private pairing witness is stored in the mode-0600 anchor and redacted from
Debug. It must never be serialized into a network request.

An interrupted genesis can be retried only with the exact original existing
anchor and still-unused stores. A new directory is refused if either store
already contains a checkpoint, including genesis. An existing directory with missing/invalid state is never
recreated. `open_managed_image_continuity` reopens the retained anchor; it does
not regenerate a missing anchor or reset an old checkpoint.

## Dispatch transition

The existing claim remains the only financial one-shot admission. Checkpoint
records contain no amount, period balance, refund, provider receipt or new bill.

1. Before reservation, compare the actual SQL/FDB checkpoints with the external
   anchor. Resolve only an exact known interrupted transition.
2. Consume the existing original SQL claim. Persist/fsync external PREPARED for
   the digest of that exact attempt, claim nonce and immutable binding.
3. Write the next FDB checkpoint in the transaction that writes the original
   admitted nonce. Carry this annotation through the existing linear
   `FreshClaim` and `FreshAdmission` types.
4. Write the next SQL checkpoint in `DirectoryGuard::finish`, in the same
   transaction as `runtime_admission_witnesses` and its current authorization
   checks. CAS against the exact previous checkpoint in both stores.
5. Observe both exact next checkpoints, fsync external COMMITTED, acquire the
   shared generation guard, and only then consume the original short-lived
   dispatch permit. Keep the guard in the admitted binding through the actual
   gateway response and incurred-cost retention.

Checkpoint failure grants no dispatch capability. A later invocation cannot
repeat a consumed claim. The existing incurred evidence/settlement path remains
responsible for money even if the worker, directory or generation later changes.

| External state | SQL / FDB | Recovery |
| --- | --- | --- |
| COMMITTED | same committed checkpoint | Observe clean state |
| PREPARED | both previous | Clear only PREPARED; no effect was permitted |
| PREPARED | both exact next | Commit external observation; never return Fresh |
| PREPARED | SQL previous, FDB next | Reconcile only from original admitted FDB/SQL claim evidence |
| PREPARED | SQL next, FDB previous | Retain PREPARED; refuse new effects |
| Any | other/divergent/missing pair | Refuse new effects |

The normal FDB-next/SQL-before crash retains an exact dispatch reference in the
same FDB transaction as its original admitted nonce and checkpoint. Recovery
checks that reference against external PREPARED, the authenticated original
prepared binding, the exact FDB nonce, and the original SQL claim/binding. It
CAS-advances only the SQL detector checkpoint and reconciles COMMITTED. It does
not insert an admission witness, return Fresh, resend the consumed original,
release its hold or fabricate a bill. That obligation remains unknown; another
distinct task can proceed with normal current authorization and budget checks.
A missing, altered or unrelated reference fails closed. Older records without
the retained reference cannot use this recovery path.

SQL-next/FDB-before cannot result from this dispatch ordering's ordinary crash;
it indicates missing/rolled-back execution evidence. An operator must recover
the original store evidence from its retained/WAL recovery source until both
exact observations are present; reopening then reconciles it. The anchor exposes no
API that overwrites a database, fabricates an admission witness or mints a send
permit. If the original evidence is unavailable, this deployment stays closed.

## Retirement and remaining delivery contract

`writer.lock` admits one host for the anchor lifetime. Never unlink it after a
crash. `generation.lock` is shared through each effect; retirement takes its
exclusive OS lock before PREPARED and holds it until reconciliation. The next
generation cannot take ownership while a previous bounded effect retains the
lock. These are cooperative local locks, not proof that arbitrary old binaries
or stolen provider credentials stopped working.

Before opening production, the registered managed device must be required to
bind every WS publish and HTTP upload to the independently observed generation
and exact checkpoint. Relay must read the same independently retained anchor
and hold its shared effect lock, rather than trust a restored SQL generation or
an echoed caller value. The shared filesystem primitive is in `relay-admission`
to avoid a Runtime/Relay dependency cycle. Upload/publish need corresponding
capsule/publication checkpoints; dispatch alone does not protect these effects.
Authorization/policy mutation recovery and provider credential isolation also
need qualification. A startup epoch alone proves none of these properties.

## Evidence boundaries

Publication consumption is a separate source increment tracked by #394/#395,
stacked after the still-unqualified recovery increment #391. A checkpoint before
Publish is insufficient: restoring a snapshot taken after that checkpoint but
before append can erase the original event, dedupe index and Runtime receipt.

Anchor layout 2 therefore retains immutable `checkpoints/<sequence>.json`
Prepared records and `effects/<original-publish-digest>.json` consumption outside
the store restore unit. The effect digest identifies the exact canonical signed
Publish wire and namespace, independent of retry checkpoints. `consume_publish`
fsyncs a create-new record before returning a private non-clonable permit. Any
existing record returns only Consumed observation; partial/corrupt records cannot
be deleted to recreate Fresh. Readers refuse old layouts or missing directories.
The pairing witness remains local and is absent from publication/network facts.

Review and FileDoc Publication retain their original checkpoint alongside their
original wire. SQL checkpoint CAS shares the existing native directory admission
transaction; FDB historical Prepared and current checkpoint share the same
capsule/Publication activation transaction. Append must compare conflict-tracked
current FDB CP with the live external tip (committed or an exact prepared-next
transition), verify the original historical record, and CAS original consumption
in the same transaction as the original event/dedupe. Later unrelated progress
does not invalidate the original historical permit. A matching old original CP
cannot stand in for an advanced live tip. The queue must retain the permit and
its generation lock through actual commit, even after caller cancellation.

An already consumed effect can recover only by observing the exact original
Accepted/event. Missing original evidence remains Unknown, never another send.
The required real restore regression includes original source/dedupe and Runtime
capsule/Publication, preserves the external consumed record, and must distinguish
this refusal from ordinary lost Accepted recovery. Root owns complementary relay
protocol/conditional-append wiring in #386/#387. Nothing in this source increment
opens production continuity or output admission. Its new tests are not yet run.

For native activation, SQL-next/FDB-before is also an ordinary crash cut because
the existing directory authorization commits first. It remains closed pending
original-evidence/operator recovery; the dispatch-only recovery above must not
be applied to it or presented as complete activation recovery.

Source includes filesystem crash-state and actual child-process lock tests;
the existing unpaid native Images HTTP journey additionally exercises the
dispatch checkpoint and cold file reopen. Its restore case rewinds matching
checkpoint rows in actual SQL and FDB and verifies refusal against the advanced
independent file. It does not perform a full deployment backup/restore.
Execution results must be recorded against the immutable tested commit.

Qualified dispatch increment: `1edd0e84`. The subsequent forward-recovery
implementation/tests described above are not yet executed. Runtime full suite
on the qualified dispatch increment: 15 passed, 0 failed
or ignored (211.56 seconds); all-target Clippy for `relay-admission` and
`zoen-runtime`: passed with `-D warnings`. The unchanged filesystem suite passed
8 tests on `e7caea33`; that first source's Runtime compile failure is preserved.
Evidence index: `zoen-image-managed-continuity-v452/evidence-index.json` in the
local build evidence directory. Static peer review found no material blocker
in this dispatch increment. This is not native iPhone or deployment acceptance.

## Shared runtime deployment

`RuntimeDeployment` and the `create_runtime`/`open_runtime` anchor constructors reuse the same durable prepare, history, consumption and generation locks for every principal in one SQL/FDB pairing. Namespace, witness and source identify this shared authority; the actual agent/device remains bound to each signed publication. A later effect by another agent advances the same checkpoint, and retirement drains effects from all principals. This is metadata for continuity, not a separate balance, device enrollment, financial admission or permission to publish.

The shared representation is schema 3, with distinct genesis/transition hash domains. Schema 2 remains byte-compatible and single-device scoped. Neither constructor adopts the other format or bootstraps missing state. An existing deployment needs an explicit authority migration; changing its schema or configuration is not a migration.

Relay's trusted operator file keeps its existing `devices` array and may also declare `runtimes`: each entry supplies `deployment` (`namespace`, `witness`, `source`), an absolute `anchor_directory`, and nonempty `principals` (`agent`, `device`). Several principals in that one entry share its reader. Duplicate devices, duplicate namespaces, mixed legacy/shared namespaces, wrong pairings and more than 256 total configured principals are refused. These locators do not replace the live directory, certificate, membership or per-effect managed-binding checks. No socket request can register a runtime or choose its anchor.

This shared anchor/relay contract is part of [general agent continuity #327](https://github.com/EnzoTironi/zoen-native/issues/327). It does not activate the general runtime: capsule/cursor and owner-policy commitments, normal reply dispatch/output integration and the full isolated SQL/FDB/vault restore/takeover proof remain required. The general paid-dispatch, output and absence-based-refund gates remain closed. Apps/creation work remains paused.
