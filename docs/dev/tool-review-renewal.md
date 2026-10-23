# Explicit renewal of an expired tool review

An expired `ApproveOnce` remains expired. The owner can request another review of
the exact frozen action. That request neither approves nor executes the action.
The agent must prepare a new pending request, which needs its own normal
`ApproveOnce`. This applies to schema1 DeckCraft and schema2 native Page actions.

## Signed intent and lineage

The existing `decide_tool_request` command accepts `RequestReview` with only the
predecessor request ID and its observed content hash. Core atomically verifies
ownership, the expired approved predecessor, current source/head/access, and
non-overridable Block before appending `ToolReviewRequested`. It does not change
the predecessor's request, resolution or grants. Repeating that owner command
returns the existing intent/successor instead of appending another intent.

Only the certified agent can stage a successor. Its frozen source, actor/device,
arguments, artifact version/hash, worker, continuation, Gate policy, step/round
budgets and descriptive fields equal the predecessor. Only the ID, expiry,
content hash and optional review lineage differ. The lineage names the original
root, immediate predecessor, signed owner-intent client ID and next generation.
Command receipt IDs exclude review generation and remain unchanged.

Checked signed projection accepts the first valid owner intent and first valid
successor in canonical log order. It rejects forged lineage, changed frozen
fields, extra successors and a fresh legacy root at the same task/step after an
explicit lineage exists. Ordinary staging returns the current review generation.
A renewed execution requires confirmed opening, owner intent and approval, with
the same checked projection and no optimistic approval overlay. A new pending
request has no execution authority.

## Operational recovery

The host keeps the existing journal and resolves each successor to the original
request/hash key. Attempts, prepared calls, completed progress and original
limits survive every generation. No new journal, approval queue or cursor is
introduced. Selected-source hosting observes the owner-intent revision without
advancing any whole-Space cursor. If synchronization is temporarily unsettled
before admission, both host modes defer that intent under the original host
deadline. They publish no final reply, reserve no attempt and advance no cursor
for that source. Only this known pre-IO deferral clears an in-memory scheduling
observation; unknown model/executor outcomes retain their refusal policy.

A genuinely new staged hold gets a durable `registered` witness at attempt zero.
Before entering a canonical executor, the host durably marks `outcome_unknown`.
Fresh staging is allowed only with that never-started witness and remaining
original attempt budget. Missing/legacy data, a corrupt or interrupted journal,
or an unknown result cannot prove that execution never began.

Known completed progress is verified by the existing native receipt/content
recovery. A stable accepted command receipt prevents new staging or repeated
effects. Unknown results without exact recovery stop for reconciliation. An
owner request does not reset a failed operation, silently retry IO, reclassify a
model response as approval, or bypass current grants/device/version checks.

## Owner UI and lifecycle

The ordinary request review shows an expired approval and offers **Request a new
review** when Core exposes eligibility. The button itself explicitly requests
preparation only; the visible explanation states that a new request still needs
independent approval. No extra confirmation dialog or model call is needed to
submit that owner intent. The UI rereads the exact predecessor before the
command and retires late preflight reads on account, device, relay, session,
focus or canonical revision changes. A failed command only triggers read-only
reconciliation; it is never automatically resent. The previous approval is
labeled as a historical decision, not confirmation that an action ran. A single
focus/foreground-scoped expiry timer refreshes metadata read-only; it never
requests a review or approves anything. Render-time actions depend on the
current immutable session snapshot so the native React compiler cannot retain
an initial null action after the asynchronous read completes.

`Requested` waits for the agent. `Superseded` opens the exact new pending request.
`Reconcile` explains that an unknown result must be checked rather than repeated.
The existing independent approve/deny/cancel controls remain authoritative.

## Compatibility and qualification boundary

Absent lineage preserves exact historical schema1/schema2 proposal JSON and
hashes. Existing `ToolDecision` values and the first three `ToolRequestDecision`
variants keep their names/order; `RequestReview` is appended. The optional DTO
field defaults to absent when reading historical JSON.

The new event, optional proposal lineage, record and fourth UniFFI enum variant
are a client compatibility boundary. Participating owner/agent readers need the
updated contracts. This is not an unchanged serialized ABI. Actual generated
TypeScript/Swift/FFI contracts and a matched native build must qualify the new
surface before runtime acceptance. Older sealed apps/binaries are not relabeled.

Source fixtures cover signed ownership, expiry, concurrent/forged lineage,
revocation, exact pending approval, accepted results, root journal budgets and
cold recovery. Qualification results belong to their immutable receipts. This
change does not revive the earlier source71 attempt or claim that its exact
refusal cause was observed. No real renewal or provider execution is implied by
source/fixture checks. Refs #261, #175 and #174.
