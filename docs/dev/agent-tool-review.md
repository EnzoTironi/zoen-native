# Explicit Agent tool review

The model proposes actions; the existing Runtime and native Core authorize them.
Codex proposals support `say`, `call`, `review` and `quiet`. `review` uses exactly
the same strict call fields as `call` and sets `ToolCall.requires_review = true`.
`say` remains ordinary text and cannot create a canonical approval request.

The flag can only tighten policy. Gate Block still blocks. An existing Gate Ask
retains its rule and its one held call. After Gate Allow, either this flag or the
native host's existing `requires_native_review` hook holds the same frozen call
with the shared `native:review-required` rule. No tool, grant, standing approval,
budget, expiry, retry or artifact authority is added. Page first-delivery policy
uses the same hook. Approval still passes through the exact native Request and
accepted-result recovery path; changing the reviewed flag or arguments cannot
reuse that permit.

The optional field defaults to false and false is omitted during serialization.
Legacy ToolCall and DeckCraft schema-1 continuation bytes therefore retain their
existing representation. New continuations retain true inside the signed,
hashed intent. Older hosts may refuse these new continuations; they must never
interpret the flag as permission. Native approval DTOs continue to expose human
operation/policy labels, not the technical reserved rule.

Provider guidance distinguishes proposing a call from executing it, directs
action tasks to offered tools and observed read results, and describes review as
stricter owner review. Advice and genuine clarification can still use `say`.
Neither a chat permission answer nor the flag grants authority, and a saved
version must come from an actual successful tool result. This contract does not
guarantee that a model chooses a tool or that a live editing journey succeeds.

## Optional closed observations

`ZOEN_AGENT_OBSERVE_TOOLS=1` enables best-effort `zoen.agent.tools.v1` records in
the normal CLI. Every other value leaves these records off. Ordinary outcome
diagnostics remain unchanged; other CLI stdout can still contain private data.

`model_input` occurs at each actual Runtime-to-Brain boundary and reports the
round, offered count, completed/failed context counts, and recognized canonical
tool kinds with compact public artifact aliases. Classification requires the
normal host's native item binding and its exact generated model name. Generic App
names that imitate that syntax remain unclassified; UnknownTool always emits
`other`. Unknown tool names are counted but omitted. The catalog is bounded to 128 entries and a line below 16 KiB.
This observes the supplied context, not provider admission, dispatch or billing.

`tool_outcome` reports only a known tool kind/alias, the review boolean and one
closed class: `held`, `blocked`, `unknown_tool`, `preparation_refused`,
`admission_error`, `host_ok` or `host_error`. A host success is a returned API
result class, not proof of a new mutation. Recovered context is not reported as
a new execution. No arguments, results, schemas, titles, message text, persona,
rules, unknown names, raw errors or credentials enter these records.

Collectors must separately validate this exact schema, its vocabularies and
bounds, and discard every other line. Historical outcome-only collectors do not
admit this new event. Logging failure cannot change results or trigger retries.
These observations cannot retroactively identify the unrecorded tool inputs or
intermediate calls in the earlier conversational-only run.
