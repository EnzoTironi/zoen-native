# Hooks

Status: design; built after M2 (see plan-real.md). Decision record: ADR 0013.

Hooks let a person, a Space, an agent, a mini-app or an org react to anything that happens in
Zoen, the way Claude Code hooks react to tool use: block or rewrite before, observe after.

The hook catalog and handlers below are a design contract. A working sandbox provider does not establish a complete hook runtime or agent execution loop. Track implementation in [roadmap status](roadmap-status.md). The proposed hook module should use the Zoen namespace.

## The event catalog

One registry, generated from the Rust types so it can't drift:
- every log event kind (`message.sent`, `message.received`, `member.joined`, `item.created`,
  `grant.requested`, `background.set`, ...), one name per `EventBody` variant;
- lifecycle and tool events (`agent.called`, `agent.tool.before`, `agent.tool.after`,
  `mini_app.opened`, `permission.prompt`, `file.uploaded`, `call.started`).

the proposed `zoen-hooks` derives the catalog with a proc macro on the event enums and emits
`hooks/catalog.v1.json` (JSON Schema per event, a version per schema). CI fails if the
committed catalog differs from the generated one.

## Phases

| phase | runs | can | deadline | on timeout or error |
|---|---|---|---|---|
| `before` | synchronously, in the path of the action | allow, deny, modify with a typed patch, ask the user | 50 ms on device, 300 ms on the server, 2 s when it asks the user (then the user's answer) | declared per hook: `fail: closed` (safety hooks) or `fail: open` (convenience) |
| `after` | asynchronously, from JetStream (server) or a local queue (device) | observe; emit new events | none on the action; per-hook budget | retried with backoff, then dead-lettered |

Ordering is deterministic: org, then Space, then agent or mini-app, then user; within a scope
by priority, then hook id. A `before` chain stops at the first deny; patches apply in order,
each validated against the event's schema. Events produced by hooks carry
`origin: hook(<id>, depth)`; dispatch refuses depth above 4 and any cycle where the same hook
would handle an event it produced.

## Where a hook runs

The location is part of the hook's type, and it decides what the hook can see.

| location | sees | enforced by |
|---|---|---|
| device (inside the core) | full plaintext of what the device can read | runs in the user's own core |
| agent runtime | plaintext only in Spaces where the agent is a declared MLS member | MLS membership; members see the agent as a reader |
| server | metadata only: event kind, Space, author pseudonym, size, timestamps | server hooks receive `MetaEvent`, a type with no content field; the server crate can't construct one from ciphertext because the conversion doesn't exist |
| outside webhook | server metadata only, unless a Space grants an agent to forward content, shown to members as a declared reader | same `MetaEvent` type; forwarding goes through the agent runtime |

## Handlers

1. **WASM component** (the portable default): wasmtime with WASI preview 2 and the
   component model, a WIT world `zoen:hooks/handler` with `before(event) -> decision` and
   `after(event) -> list<emit>`. Fuel and memory limits, no ambient network or filesystem,
   capabilities granted through Cedar. The same component runs on a device, in agentd and on
   the server.
2. **Agent handler**: a prompt or command evaluated by an agent with a typed output
   (`allow | deny(reason) | patch(...)`), only in places where the agent is a member.
3. **Signed HTTPS webhook**: `Zoen-Signature: t=<ts>,v1=<HMAC-SHA256(secret, ts.body)>`,
   five-minute replay window, idempotency key equal to the event id, retries through
   JetStream with a dead-letter queue. `after` only.

## Manifests

```yaml
hooks:
  - id: redact-cpf
    on: message.sent
    match: { space: "*" }
    phase: before
    run: { wasm: "sha256:…", fuel: 5_000_000 }
    fail: closed
  - id: notify-crm
    on: member.joined
    phase: after
    run: { webhook: "https://example.com/zoen", secret_ref: "kv:hooks/crm" }
```

Scopes: user (personal settings), Space (admins), agent or mini-app (in its manifest), org.
Installing a hook needs a Cedar grant for its scope and shows a disclosure of what it can see;
the installation is a signed event in the Space's log, visible to members.

## Scale

`after` hooks never touch the hot path: server-side dispatch is a JetStream consumer per
hook class; device-side dispatch is a background queue in the core. `before` hooks are the
only synchronous ones, with the deadlines above, and the load test measures their p99.

## Dogfood

The pieces we already have become hooks, which proves the primitive and deletes special
cases: the safety gate becomes a `before` hook on `agent.tool.before` (fail closed); push
dispatch an `after` hook on `message.received` (server, metadata only); search indexing and
analytics `after` hooks on Closed and Public events.

## Proof

- A journey where a WASM `before` hook on the device redacts a pattern in an outgoing
  message, and an `after` webhook receives only metadata.
- A test that server hooks can't observe plaintext (type-level and a runtime scan).
- A loop-guard test: a hook that emits the event it handles stops at depth 4.
