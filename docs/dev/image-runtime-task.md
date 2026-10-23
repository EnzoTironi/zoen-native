# Native image tasks and the existing financial authority

Tracking: #325, parents #179/#175, gateway #307 and pricing #317.

Image creation is an explicit schema4 `ToolProposal` inside the existing signed
`RequestOpened` / `RequestResolved(ApproveOnce)` flow. Its original source,
owner, agent, certified device, prompt, specification, definition and price digest
are immutable. Reply-only trust cannot authorize this operation. Native facts
cannot be constructed or deserialized by a host or WebView.

`RuntimeAuthority::prepare_image_task` derives the request from the authenticated
native capsule, checks the current relay source and directory, and freezes the
original owner policy. The task uses the existing run/device fences, prepared
binding, native reference pins, reservation, claim, admission witness, evidence,
disposition and balanced postings. `BoundPrice::Chat` preserves the original
chat JSON bytes; `BoundPrice::Image` selects the explicit image obligation.

Paid execution and output preparation require the existing restore-continuity
and output barriers. Both production barriers remain closed. A task locator,
successful constructor, approved proposal or retained capsule cannot open them.
The controlled adapter used by tests is private and does not make paid HTTP
calls. Its explicit empty-namespace output fixture does not qualify production
restore or old-writer exclusion.

Image evidence uses a bounded binary frame containing the original provider
body, normalized receipt, immutable task and verified PNG bytes. The existing
custody key/subject scheme seals it. Existing FDB evidence/transfer roots refer
to immutable 64 KiB physical chunks; a root is committed only after all chunks
are known retained. SQL retains the same sealed frame in `runtime_evidence`.
Migration0036 permits at most32MiB only for explicit image obligations; legacy
chat keeps its98KiB SQL/application ceiling. Reconciliation batches retain at
most32MiB of payload. Missing or changed chunks cannot settle a partial receipt.
Unpublished physical chunks can remain after a failed write; garbage collection
for those orphans is not claimed here.

Incomplete usage, unsupported usage detail and unrepresentable integer charges
retain an unresolved hold and the original evidence. They do not produce a zero
charge. Supported charges above the configured hold are posted in full. A lost
result or retry cannot mint another claim or provider admission. A result whose
association write failed can recover that association from the authenticated
capture and its original admitted nonce. Conflicting retained results are
refused; recovery never invents an admission or calls the provider.

Output preparation validates the original admitted result and current native
approval, then uses the existing encrypted `FileDoc` / PNG path. The signed
original wire is retained by its producer before transport. The native image and
publication reference activate under the same directory/device/source fences.
The known SQL acknowledgement window is limited by the original approval's
remaining lifetime; losing an activation acknowledgement reads the retained
output instead of signing another one.

## Remaining acceptance

Tests and native/financial integration must be executed before qualification.
The full task-to-delivery journey is not accepted by separate Core and financial
fixtures alone. Production continuity/output barriers, real-provider execution,
blob upload/delivery receipts, app UI and physical-device memory remain pending.

The complete native capsule has an existing4MiB ceiling. A PNG whose file/blob
state cannot fit that capsule must fail without expanding that global bound;
its incurred cost/evidence remain retained. Supporting the whole12MiB provider
output domain requires storing encrypted file blobs through the existing file
authority outside the capsule, while retaining authenticated `FileDoc`, pending
upload and publication references inside it. This remains part of #325; no
second document state, accounting store or editing authority is introduced.
