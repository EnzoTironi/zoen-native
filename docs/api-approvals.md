# Approvals: the core API and what the backend must expose

For the approvals swipe stack (`apple/Shared/Features/Approvals`). An agent's request is
something it wants to do that its owner must confirm. The stack lets the owner decide
once (approve or deny) or set a **standing decision** ("always approve" / "always deny")
for that kind of action from that agent in that Space. Everything below is in the Rust
core (`roda-ffi`), generated into Swift by UniFFI (`RodaCore`). Plan and mini-app proposals
retain their complete typed output. The Debug showcase seeds three extra requests (`seedShowcaseApprovals`,
`#if DEBUG` only).

## Calls

```swift
// Pending requests (existing). The stack shows the ones from your own agents
// (`agent.isMine`), oldest first.
let reqs: [AgentRequestDto] = core.requests()
//   new fields: actionKey ("external", "money", …), canAlwaysApprove (false on red lines),
//   byStanding (it was resolved by a standing decision, not by hand)

// Decide one. Approve / Deny resolve this request only.
// AlwaysApprove / AlwaysDeny also issue a standing grant (agent + actionKey + this Space)
// and resolve the other pending requests it covers.
let out: DecideOutcome = try core.decideRequest(requestId: r.id, decision: .alwaysApprove)
//   out.request          the resolved request
//   out.message          what the agent posted in the chat ("Feito…" / "Ok, não vou…")
//   out.standingGrantId  the new grant, for standing decisions
//   out.alsoResolved     how many other pending requests it resolved

// Standing decisions you made, newest first (Permissões screen, profile sheet).
let mine: [StandingDecisionDto] = core.standingDecisions()
//   grantId, agent, spaceId, spaceTitle, actionKey, actionLabel, allow, atMs

// Revoke one: the agent asks again from now on (already-resolved requests stay resolved).
try core.revokeStanding(grantId: d.grantId)
```

Errors: `decideRequest(.alwaysApprove)` on a red line returns `Forbidden`. A wrong owner,
changed content, removed agent or conflicting second decision fails. Repeating the same
decision returns its stored receipt without creating another item or charging usage again.
An interrupted standing batch can resume while its original grant remains the current
standing decision. Replacement, expiry or revocation prevents that resume. A relay-ordered
decision or membership change takes precedence over a pending local resolution, including
before its acknowledgement; live and reopened views show the same effects.

## Events (signed, in the Space's log)

| When | Event | Notes |
|---|---|---|
| agent asks | `RequestOpened { request: AgentRequest }` | `proposal` retains the item id, typed document, origin, completion text and model cost; its hash also binds the request metadata, agent and Space |
| owner decides | `RequestResolved { request, approved, content_hash, resolution }` | the optional receipt creates the proposed item and its app permission in the same signed event; one accepted decision per request |
| always approve / deny | `standing_grant` inside the first `RequestResolved.resolution` | committed with that decision; see the shape below |
| revoke | `GrantRevoked { grant }` | grantor must be the agent's owner |

Model cost is incurred when the proposal is prepared, so it is projected once from
`RequestOpened`, including if the owner later denies it. A paid proposal needs an active,
owner-signed trust grant in that Space; a self-claimed owner in a profile is insufficient.
Approval does not charge that cost a second time. Direct local outputs, chat cards,
permissions and usage commit together in SQLite.

An external action without a retained executable proposal records the owner's decision
and reports that execution is waiting. Approval does not mark a payment or send as done.
Old creation cards that lost their payload must be regenerated before approval.

The projector checks owner, hash, Space, membership, prior status and receipt fields during
replay and live ingestion. A signed but unauthorized decision remains in the verified
chain without changing the request, item or usage projection. Duplicate item creation and
edits from another Space cannot replace an approved output.

Absent optional fields retain the exact historical JSON bytes. Older readers still do
not understand embedded item effects, so shared-agent publishing requires upgraded readers
before enablement; see [ADR 0048](adr/0048-durable-agent-approvals.md).

Grant shape for a standing decision:

```rust
Grant {
    id,                                   // new GrantId
    grantor: owner,                       // the agent's owner (only they can issue it)
    grantee: Some(agent),                 // the agent
    scope: GrantScope::Space(space),      // the request's Space
    capability: Capability::Standing { action: "external".into(), allow: true },
    expires_at_ms: None,                  // until revoked
}
```

`action` is `roda_grants::standing_key(&ActionClass)`: `reply`, `reversible`, `external`,
`irreversible`, `money`, `public_audience`, `third_party_data`. The newest unrevoked
Standing grant for (agent, Space, key) wins.

## Evaluation (core and server must agree)

`decide()` wraps `roda_grants::evaluate()`:

1. `evaluate()` says *Act* or *Block*: that stands. Standing grants never widen beyond it.
2. It says *Request* and there's a matching **standing deny**: *Block(StandingDeny)*. No
   request is opened; the agent says it won't.
3. It says *Request* and there's a matching **standing allow** that
   `standing_allow_permitted(action, policy)` accepts: *Act* (undoable as Reversible).
4. Otherwise: open a request.

`standing_allow_permitted`: `reply`, `reversible`, `external` yes; `money` only up to the
policy ceiling (R$ 100 today); `public_audience`, `third_party_data`, `irreversible` never.
Red lines therefore always ask, even after "always approve". "Always deny" covers anything.

Cedar equivalent for `zoen-agentd` (one policy per Standing grant, owner as the issuer):

```cedar
// GrantIssued Standing { action: "external", allow: true }, scope Space(s), grantee agent a
@grant("<grant id>") @issuer("<owner id>")
permit (principal == Agent::"a", action == Action::"external", resource in Space::"s")
unless { context.red_line };

// allow: false
@grant("<grant id>") @issuer("<owner id>")
forbid (principal == Agent::"a", action == Action::"external", resource in Space::"s");
```

`context.red_line` is true for money above the ceiling, public audience, third-party data
and irreversible actions. A `GrantRevoked` drops the policy. `forbid` beats `permit`, which
matches the core: a newer allow replaces an older deny only because the core keeps the
newest grant per key, so the server must also drop the older policy when a newer Standing
grant for the same (agent, Space, key) arrives.

## What the backend has to do

The local approval implementation does not establish a cloud execution or billing service.
The remaining M3 work includes owner-authorized agent registration, authenticated MLS
membership, a sealed durable model/tool run loop, budget reservations and provider receipts.
Local cost values supplied by a caller are not provider settlement evidence.

1. **Redeploy relays with the new `roda-types`.** The relay parses `EventBody`; an old relay
   can't read `Capability::Standing` and breaks signature checks for those events.
2. **`zoen-agentd` must run the same `decide()`** before opening a request, so an agent with
   a standing allow acts without asking and one with a standing deny stops, on the server
   too.
3. **Push:** one notification per `RequestOpened` for the owner (category `approval`). Opening
   it should land on the approvals stack (the app needs a route for it; today the bell
   opens the stack when approvals are waiting). No push for requests a standing decision
   resolved.
4. **Multi-device:** a decision on one device resolves the card on the others through the
   live sync (`RequestResolved`). The stack reloads on every sync revision.

## Not in the core yet

- **Join requests** (someone asking to enter a Space) and **hook installs** are not
  `AgentRequest`s. The stack shows only agent requests. To include them, the core needs a
  request kind with its own resolve path; until then they stay in the notifications list.
- **Mac** keeps the notifications list; the swipe stack is iOS only.
