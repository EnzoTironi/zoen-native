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

The private `ManagedImageOutput` type is not Clone/Deserialize and is separate
from `FreshOutputNamespace`. It binds namespace/source, certified agent/device,
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
