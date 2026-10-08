# ADR 0013: Hooks as a first-class primitive

Status: accepted, built after M2. Design: [hooks.md](../hooks.md).

## Decision
A generated, versioned event catalog; `before` (synchronous, deadline, fail closed or open
per hook) and `after` (asynchronous, observe and emit) phases; deterministic ordering and loop
guards; the run location typed by what it may see (device plaintext, agent-member plaintext,
server metadata only via a content-free `MetaEvent` type); WASM components (wasmtime, WASI
p2) as the portable handler, agent handlers and signed webhooks; manifests per user, Space,
agent or mini-app and org, installed under Cedar grants as signed events.

## Alternatives
- Webhooks only: no on-device plaintext hooks, nothing synchronous.
- Scripting language (Lua, JS): no capability model as clean as WASI components and no single
  runtime for device and server.
- WASM components: one sandbox everywhere, fuel limits, capability-based. Chosen.

## Interrogation
- *Can a server hook leak content?* It receives a type without content; the only path to
  content is an agent that is a declared member.
- *Can a slow hook stall chat?* `after` hooks are off the path; `before` hooks have deadlines
  and declared failure modes.
- *Loops?* Depth limit and origin tags.

## At 1B users
If 10% of messages match one server `after` hook, that is 2B dispatches a day (70k/s peak) on
JetStream consumers, about 25 consumer nodes. `before` hooks on the device cost the device;
server `before` hooks are rare (org policy) and budgeted at 300 ms.
