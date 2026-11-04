# Images-only output authority

Tracked by #399, under #366/#380/#325. Executable source `6556773c` is written
and statically reviewed; its combined actual-services qualification is pending.
This document is not a production, paid-provider or iPhone acceptance report.

## Normal host construction

After existing certified native/vault construction and managed continuity
initialize/open, the normal host consumes its Runtime through
`qualify_managed_images_output(self, target)`. The fixed target is the same
trusted route later supplied to `ImageRuntimeHost`. User/model tool arguments
cannot supply the route, native facts, pairing witness or an output token.

The constructor derives current SQL/FDB checkpoints, live external anchor and
existing certified device. It retains the real generation OS lock and requires
both network observations against that exact route: ordinary legacy binding is
refused; managed checkpoint MAC binding succeeds with the mandatory capability
on the same actual authenticated source/backend. Authentication/network failure
is not accepted as the negative half followed by success. No image/provider
request, upload or publication occurs during qualification.

Initial generation-1/sequence-0 may prove only read-only RuntimeBinding. The
actual registry requires exact SQL/FDB/live-anchor genesis; HTTP upload and WS
Publish reject sequence 0 before effects. Later historical checkpoints require
their exact retained Prepared record. A fabricated checkpoint or capability
string cannot replace the pairing proof or external history.

## Private scope and actual effects

The private `ManagedNativeOutput` type is not Clone/Deserialize. Its checks and
qualification implementation live behind the shared `NativeOutputAuthority`
interface. The independent `FreshOutputNamespace` implementation is compiled only
for tests; production text-output transactions no longer depend on its type. It binds namespace/source, certified agent/device,
fixed route and the live generation lock. Each native owner transaction reads
current FDB checkpoint with conflicts and compares it with the independent live
anchor, in addition to existing root/head/lease/directory checks. Missing or
rolled-back checkpoint state fails closed. A retained historical publication
may survive unrelated tip advancement in the same generation.

Only Image stage/run/output preparation/delivery and the typed NativeImage paid
admission use this scope. Shared admission chooses the scoped continuity path
from the private verified-step variant, never a caller operation flag. Existing
claim/witness/finance authority is unchanged. Current owner review, trust,
expiry, directory, exact specification and budget remain independently required.
General reply output, `run_model` continuity and absence-based refund boundaries
remain closed. The constructor never creates a financial claim or fixture.

Text activation now reuses the same retained activation intent and paired
SQL/FDB/external-anchor publication checkpoint. Its sealed record optionally
carries that checkpoint; unmanaged historical records keep their original shape.
A managed context refuses an uncheckpointed record instead of downgrading to the
legacy path. Recovery validates the original paid capture, run, envelope and
native root before restoring its reference. Current output context is checked
before automatic checkpoint maintenance. This is exercised with explicit native
fixture continuity; it does not open production paid chat or text transport.

Retained text transport reads the exact pending native reply (including its
original reference, event and wire digests), with current grant, per-chat FIFO
and failed-head checks. Runtime reconciles historical receipts first; only a
still-pending reply loads fresh run/device leases, directory and source facts.
The shared relay transport rebinds to the publication checkpoint before invoking
the owner's final authorization callback. Cancellation, grant expiry, native
root or source changes during the handshake therefore refuse before Publish.
A lost ACK is resolved by the original sequencer position after cold reopen,
without another model call. The public paid-chat boundary remains closed until
normal host admission/continuity is qualified; these are real local relay tests
with explicit fixture continuity, not production readiness evidence.

Reply transport additionally binds the native source head, original run/attempt
and optional grant deadline into the existing managed transport MAC. Its relay
queue carries those facts on the same private linear publication permit. Before
a fresh append, the real log transaction compares its current (including
same-batch) head, reads cancellation/closure with conflicts, and checks deadline.
Historical exact dedupe remains observation before these fresh-effect checks.
Invalid or foreign-space fences refuse before independent consumption. Unknown
append outcomes after consumption retain the original response and financial
obligation; they never authorize resend/refund from absence. Recovering such a
refused/uncertain original as an explicit terminal result remains part of normal
paid-chat host integration, whose production boundary stays closed.

Scope lifetime retains the OS generation lock; stopping the host/dropping its
last scope releases it before retirement can complete. Actual relay effects
also retain their own guard/linear publication permit through commit. Runtime
restart must re-open the original anchor and repeat qualification; serialized
metadata cannot reconstruct a scope.

## Evidence and limits

The existing preserved combined `de7f749e` compiled CLI/Runtime successfully,
reached the actual #391 recovery marker, then aborted later with stack overflow.
It did not include this constructor. The successor splits review/model/delivery
polling into sibling phases and preserves the original assertions. A new frozen
combined batch must prove the normal ToolHost command, owner ApproveOnce,
controlled actual gateway, same FileDoc after restart, one claim/charge, and
continued refusal of general reply output. No earlier fixture PASS is assigned
to this source.

The external owned local Unix anchor remains outside SQL/FDB backups. This is
single-host cooperative retirement, not distributed/NFS/privileged-tampering
protection. Missing original consumed publication evidence remains Unknown;
ordinary lost Accepted can recover only the exact retained original event.
Native SQL-first activation crash recovery and pre-consumption queue reservation
remain explicitly tracked under #380/#394.
