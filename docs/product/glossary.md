# Glossary

Plain-language meanings first, then the technical term behind them. Words in **bold** are what people see in the app; `code` is what you'll find in the source.

| Term | In the product | In the code |
| --- | --- | --- |
| **Chat** | Any conversation: a direct chat, a group or a community. | A Space. |
| **Space** | — (never shown) | The unit of conversation: a signed, append-only, hash-chained event log with members, items and grants. |
| **Community** | A large, open-to-join, moderated group. | A Space with `Closed` privacy (relay-readable by design; see ADR 0027). |
| **Thread** | Replies grouped under one message. | A reply marked `in_thread`; one level deep. |
| **Item** | Something a chat produced: a page, a plan, a file, a mini-app. | A versioned `ItemContent` changed only by signed events. |
| **Live Page** | A document the chat edits together. | A Loro block document stored as signed Item versions. |
| **Plan** | A page with rows, costs and a budget. | A typed Live Page (PlanDoc) with integer-cent costs. |
| **Agent** | A member that can answer, book or act — with your approval. | A first-class identity of kind Agent with its own MLS device, run by `zoen-agentd`. |
| **Creator** | The built-in agent that creates agents from a conversation. | `creator_definition()` in `zoen-agentd`. |
| **Agent link** | A link that opens a chat with an agent. | `/a/CODE` public page plus a redeemable invite. |
| **Approval** / **Your agent needs you** | A card asking you to approve before something happens. | A durable `AgentRequest` resolved by the owner's signed `RequestResolved`. |
| **Mini-app** | A small app inside a chat. | An MCP Apps server (tools + `ui://` HTML resource) whose state is an Item. |
| **Grant** | What an agent or mini-app is allowed to do. | One of the five primitives; evaluated to act / ask / block. |
| **Linked device** | Another phone, computer or browser signed in to your account. | A device key certified by the identity key; its own MLS leaf. |
| **Recovery key** | 64 digits that restore your account if you lose every device. | Key for the client-side encrypted backup (ADR 0046). |
| **Relay** | — (never shown) | `zoen-relay`: orders, stores and fans out signed envelopes per Space. |
| **Identity** | Your account. | An Ed25519 identity key (person, agent or Space). |
| **Member** | Someone in a chat. | A membership event plus a role. |
| **MLS** | — | Messaging Layer Security (RFC 9420), via OpenMLS, for end-to-end groups. |
| **UniFFI** | — | Generates Swift and Kotlin bindings for the Rust core. |
| **Sandbox tier** | — | Where an agent tool runs: WASM component, Firecracker microVM, or browser in a microVM. |
| **Protocol 4** | — | The current Zoen Sync wire protocol version. |

## The five primitives

Everything in Zoen is built from five types defined in the core's types crate:

1. **Identity** — a person, an agent or a Space.
2. **Space** — created by a signed event, projected from its log.
3. **Member** — added by a signed event, with a role.
4. **Item** — versioned content changed by events.
5. **Grant** — trust levels, budget and capabilities.
