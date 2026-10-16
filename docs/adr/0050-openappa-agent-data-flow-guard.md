# ADR 0050: OpenAPPA as the data-flow guard for Zoen agents

Status: accepted. Implementation deferred until agents unfreeze (#91), after the messenger ships.

Upstream: [archestra-ai/OpenAPPA](https://github.com/archestra-ai/OpenAPPA) (MIT, Rust,
workspace version 0.34.0 at `804dd5c`, 2026-10-16). Paper:
[APPA: Recoverable Information-Flow Control for Real-World LLM Agents](https://arxiv.org/abs/2607.24625).

## Context

Zoen agents are first-class members of chats and Spaces with their own MLS identity
([ADR 0026](0026-e2e-spaces-on-openmls.md), [ADR 0049](0049-agent-owner-authorization.md)).
They run in `zoen-agentd` (Rust, Rig behind Zoen traits), call tools in WASM, Firecracker or
browser sandboxes ([ADR 0028](0028-agent-sandbox.md)), reach the network only through the
egress proxy that injects credentials, render MCP Apps mini-apps
([mini-apps](../mini-apps.md), [ADR 0042](0042-dynamic-ui.md)), and ask their owner through
durable approval cards ([ADR 0048](0048-durable-agent-approvals.md)).

Cedar grants answer *may this agent call this tool*. They do not answer the question that
matters most for an agent sitting in many conversations: *may the data this agent has read
go to this destination*. The concrete risk is an agent reading a private DM or a closed Space
and then repeating it in a bigger group, a public web form, or a mini-app. A prompt injection
in any message or web page can ask for exactly that. Classifiers that guess "is this
sensitive" are probabilistic; we want a check that gives the same answer every time.

OpenAPPA (Agentic Permissions Policy Algebra) is that check. It tracks a two-dimensional
label on everything the agent reads (audience: who may see it; trust: how much we believe it)
and checks every tool call against the accumulated label **before** the call runs. Policy is
declarative TOML. The engine decides from its event log alone, with no network or file calls,
so a log always replays to the same decision. It runs in-process (`AdapterName::Embedded`:
the host builds events and reviews through its own surface) or as a sidecar. It is written in
Rust, so it embeds directly in `zoen-agentd`. Upstream reports 0 successful attacks in 1,320
scored runs with 88–90% task completion.

## Decision

Adopt OpenAPPA as the deterministic data-flow guard for every agent tool call. Cedar stays
the capability layer (who may call what, which sandbox tier); APPA is the flow layer (where
read data may go). A call must pass both.

### 1. Where it sits

- **In-process in `zoen-agentd`, at tool dispatch.** The single dispatch path that already
  routes calls to WASM/T1/T2 tiers asks the guard first. Order: Cedar grant → APPA check →
  tier router → sandbox. A denial means the tool never starts and no sandbox is created.
- **Every read is an event.** Message history, Space items, files, memory, web fetches and
  tool outputs are recorded with their label when they enter the agent's context.
- **MCP Apps host.** Opening or feeding a mini-app, and every `tools/call` a mini-app makes
  back through the host, go through the same check. A mini-app is a destination like any
  other.
- **Egress proxy as backstop.** The proxy keeps enforcing domain allowlists and credential
  injection. It does not decide flows; it only refuses traffic the guard never approved
  (calls carry the decision id).
- No sidecar in production: one process, no extra hop, no extra failure mode. The sidecar
  mode is used only by CI and local debugging.

### 2. Label mapping

| Zoen source or destination | APPA audience | Trust |
|---|---|---|
| DM | literal readers: the two members | member-authored: `member` |
| Group chat | `@zoen:chat/<id>`, resolved to the current MLS member set | `member` |
| Space (closed) | `@zoen:space/<id>`, Space members | `member` |
| Space marked public / published Live Page | `public` | `member` |
| Owner-only data (agent memory, owner's files, credentials) | `self` (the owner) | `owner` |
| Public web, search results, fetched pages | `public` | `untrusted` |
| Mini-app (MCP App) | audience of the chat/Space it is rendered in; third-party server = `public` | output `untrusted` |
| External tools via egress (email, GitHub, Slack…) | per tool `delta` in policy, default `public` | output `untrusted` |

- Trust chain: `untrusted < member < owner`. Untrusted content may inform an answer but cannot
  authorise a write or an escalation by itself (prompt-injection defense).
- Membership comes from a Zoen **audience source** backed by the local MLS group state the
  agent already holds. Members are reported as Zoen identity ids, never phone numbers or
  names. The answer is pinned into the decision record, so replay does not depend on later
  joins or leaves.
- A send to a chat is allowed when the destination's member set is a subset of the data's
  audience. Example: text read in a DM between Ana and the owner may go back to that DM, but
  not into a group that also contains Bruno.
- Policies live in-repo as `agents/policy/zoen.appa.toml`; owners and Space admins can only
  tighten them, never loosen them.

### 3. How denials surface

- The agent never fails silently. It posts a short card in plain words, in the user's
  language: "I can't share this here: it came from your private chat with Ana, and this
  group has people who weren't in that chat."
- When policy marks the flow **reviewable** (APPA `review`), the card is a normal durable
  approval card ([ADR 0048](0048-durable-agent-approvals.md)): swipe to allow once, or
  decline. Only the owner of the data's audience can approve. For a DM that means the owner
  of the agent. Content from other members' private chats needs each of those members. The
  signed resolution is recorded as the override in the APPA log.
- Hard denials (credentials, `self` data to `public`, untrusted input driving a write) show
  no override button.
- Decision logs are encrypted to the owner's agent key like other agent state, with no
  plaintext in server logs.

### 4. Policy tests in CI

- `appa describe --config agents/policy/zoen.appa.toml --check` validates the policy loads.
- `appa replay --config … agents/policy/tests/` runs scripted traces with expected decisions:
  DM → bigger group denied, Space → same Space allowed, web content cannot trigger send,
  mini-app in a private Space cannot post to a public URL, approved override allowed once.
- Both run as a required check on any change to `agents/policy/**` or the guard crate. The
  audience source in tests is a fixed roster fixture, never a live resolver.
- Every reported agent leak becomes a replay trace before the fix merges.

### 5. Containing the risk of a preview dependency

- Upstream calls itself a **preview and RFC**; config and wire surfaces may break without
  shims. Therefore:
  - Pin an exact version/commit (`=0.34.x` or a git rev) in `Cargo.lock`. Upgrades are their
    own PR and must pass the replay suite unchanged.
  - Wrap it behind our own trait in a `zoen-agent-guard` crate, e.g.
    `trait FlowGuard { fn observe(&mut self, read: Labeled); fn check(&self, call: &ToolCall) -> Verdict /* Allow | Review(reason) | Deny(reason) */; }`.
    Nothing else in `zoen-agentd` imports APPA types. Swapping or forking means
    reimplementing one crate.
  - Our replay traces are written against Zoen events and translated in the adapter, so they
    survive an upstream format change.
  - If upstream stalls, the MIT license lets us vendor and maintain the crates we use.
- Fail closed: if the guard cannot load its policy or resolve an audience, tool calls that
  send or write are denied; read-only calls continue.

## Consequences

- Agents get a deterministic, explainable answer to "can this data go there", which is the
  main privacy promise of agents living inside encrypted chats.
- Some valid tasks will need an approval swipe. Upstream reports about 10% lower completion
  than unguarded runs; we accept that and tune with replay traces.
- One more dependency in a security path, contained behind `FlowGuard` and a pinned version.
- Work is deferred: nothing changes in the code until #91. When agents unfreeze, the order
  is guard crate + trait, Zoen audience source, dispatch hook, denial cards, CI replay, then
  the MCP Apps host.

## Alternatives considered

- **Cedar only.** It handles capabilities but has no notion of data flow across calls.
- **LLM or PII classifiers.** Probabilistic and bypassable by injection. Useful as an extra
  label annotator later, but not as the gate.
- **Writing our own IFC.** The same model with no paper, benchmarks or community. We would
  only do that by forking OpenAPPA if upstream disappears.
- **Archestra LLM proxy.** Works at the model proxy, outside our E2E boundary. It would need
  plaintext on a server we don't want to have.
