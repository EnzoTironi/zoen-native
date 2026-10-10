# ADR 0048: Durable agent proposals and approval effects

Status: implemented in the Rust core; cloud execution and release gates remain open.

## Problem

The Suggest path constructed a complete plan or mini-app but persisted only its request
card. After restart, approval reported simulated success without creating the proposed
item. Resolution also committed before a plan edit or permission write; a later failure
could leave an approved request without its effect. Replay accepted a signed resolution
without checking its owner, hash, Space or earlier status.

## Decision

`AgentRequest.proposal` retains a typed `ItemProposal`: output id, kind, content, origin,
completion text and model cost. SHA-256 binds the versioned proposal, request id and visible
metadata, agent and Space. Unknown or lost creation content cannot be approved.

`RequestResolved.resolution` is the owner's signed receipt. The projector derives the
item and its initial mini-app permission from the original proposal; the resolution cannot
substitute content, output id, cost or completion text. The decision, item, scoped permission
and chat card are effects of one durable event, so another reader cannot observe an approved
card without its retained output. The same decision returns its receipt without another
append. A conflicting decision fails.

A standing grant is part of the first resolution receipt. Each covered request settles
atomically. A failed covered write returns an error; retry resumes the remaining requests
without repeating settled effects. Revocation prevents that retry from authorizing more
work. Removal or a reader role prevents new agent output.

Model usage is incurred preparing a proposal, including a denied proposal. It is projected
once at opening, requires an owner-signed trust grant for paid proposals, and is not charged
again during approval or replay. Direct local generation uses a SQLite savepoint covering
the output, card, app permission and usage. Failure rolls back disk and rebuilds memory.
Budget arithmetic rejects negative and overflowing costs.

The same projection checks apply to verified stored logs, pending outbox entries and live
ingestion. Unauthorized signed commands remain on the cryptographic chain but do not alter
approval, item, grant or usage state. Item ids cannot be recreated, and item edits must be
authored in their item's Space. Agent ownership metadata arriving after history triggers
reprojection rather than permanently losing the request.

## Compatibility and remaining M3 work

Both fields are optional and omitted when absent. Historical signed request and resolution
bodies round-trip byte for byte. New receipts require readers that implement their effects;
this change does not prove that mixed deployed versions can publish them safely. Shared
agent publishing remains gated on upgraded readers. Protocol 4 and the relay schema are
unchanged in this slice.

The core currently holds local agent keys. This is not proof of an owner-authorized cloud
principal, registered agent MLS membership, or a durable model/tool executor. The M3 runtime
remains a Zoen-owned Rust process, with Rig behind `zoen-models::ModelGateway`, sealed state
in FoundationDB and wakeups through JetStream, as specified in `plan-real.md`. Restate is
not a runtime dependency.

Before cloud execution is enabled, the runtime must verify owner authorization at enrollment,
reserve and settle budget durably from actual provider receipts, resume approvals after
restart, fence duplicate workers and revocation, and publish authenticated agent results
through the relay. Caller-supplied local costs do not prove provider billing. External sends,
payments, production account authority, paid sandbox deployment and billion-user capacity
remain separate completion gates.

## Verification

The new core journeys create fresh principals and real SQLite logs. They reproduce the old
missing-output behavior, reopen before approval, verify usable apps and scoped permissions,
inject write failures, resume interrupted standing batches, revoke or remove principals,
ingest authenticated wrong-owner/hash/Space decisions and prove that replay has no duplicate
effect. A second engine receives signed binary frames and recreates the same approved item.
These are core and wire-ingestion checks, not a live cloud model or production scale proof.
