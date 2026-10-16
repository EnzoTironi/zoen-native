# Features

Zoen is a messenger first. Everything else — pages, agents, mini-apps — lives **inside** a conversation, so nothing needs a second inbox.

Each feature below carries a status so you always know what's real:

| Badge | Meaning |
| --- | --- |
| **Built** | Implemented in the shared core and/or clients on `main`, with tests or recorded journeys. |
| **In progress** | Partly implemented; completion gates are still open (see the [roadmap](roadmap.md)). |
| **Planned** | Designed or decided, not implemented yet. |

## Messenger

The everyday core: talk to one person, a group or a whole community.

| Capability | Status | Notes |
| --- | --- | --- |
| Direct messages and groups, end-to-end encrypted by default | **Built** | OpenMLS; the relay stores ciphertext only ([encryption](../technical/encryption-and-devices.md)). |
| Communities | **In progress** | Conversation and permission foundations exist; discovery, moderation and reporting are open. Communities are moderated and relay-readable by design ([ADR 0027](../adr/0027-end-to-end-by-default.md)). |
| Unified **Chats** list with optional filters | **Built** | Direct chats, groups and communities share one list (Apple; web shell follows the same hierarchy). |
| Offline-first sending | **Built** | A durable outbox survives restarts and reconnects ([ADR 0004](../adr/0004-offline-first-outbox.md)). |
| Replies and quotes | **Built** | An inline reply stays in the timeline and quotes the original. |
| Delivery/read receipts and reactions | **Built** | One reaction per person per message; receipts only move forward ([review](../reviews/messenger-receipts-reactions.md)). |
| Search, edit and delete | **Built** | Scoped search with stable paging; edits and deletes are signed events ([review](../reviews/search-edit-delete.md)). |
| Typing and presence | **Built** | Ephemeral — forwarded, never stored ([ADR 0005](../adr/0005-ephemeral-signals.md)). |
| Encrypted profiles and chat backgrounds | **Built** | Names, bios and photos are sealed ([ADR 0016](../adr/0016-encrypted-profiles.md)). |
| Linked devices with history | **Built** | Phone and computer share one account and its history ([ADR 0045](../adr/0045-linking-devices.md)); browser pairing is in progress. |
| Encrypted backup with a recovery key | **Built** (development) | Production recovery authority still open ([ADR 0046](../adr/0046-encrypted-backup.md)). |
| Photos and voice messages | **In progress** | Encrypted blob transport exists; native photo/voice delivery is open. |
| Push notifications | **In progress** | Contract and seams exist; APNs/FCM delivery is open ([push](../dev/push.md)). |
| Voice/video calls and live | **Planned** | Listed for a later phase in the [plan](../repensado.md). |

## Spaces

Under the hood, every conversation is a **Space**: a signed, append-only log with members, items and permissions. People never see the word — they see a chat. Spaces are what make every feature consistent: a message, a page edit, a vote in a poll and an agent's approval request are all signed events in the same log ([ADR 0014](../adr/0014-memory-and-knowledge-are-items.md)).

- **Owned and ordered** by the relay per Space ([ADR 0018](../adr/0018-space-ownership.md), [ADR 0023](../adr/0023-per-space-sequencer.md)).
- **Readable only by members** when end-to-end (the default for direct chats and new groups).
- **Resources stay inside the conversation**: community permissions, shared files, pages and mini-apps belong to the Space.

## Threads

Reply to a message *in a thread* to keep side discussions out of the main timeline. **Built** in the core and the Apple apps.

- A thread hangs under one root message; the root shows its reply count.
- Threads are one level deep: replying to a thread reply points back at the root.
- Opening a thread shows the root followed by its replies, oldest first.

## Live Pages

Documents that people and agents keep up to date together, inside the chat ([ADR 0041](../adr/0041-live-pages.md), [contract](live-pages.md)).

| Capability | Status |
| --- | --- |
| Block document (headings, paragraphs, tasks, lists…) on Loro, Markdown in and out | **Built** |
| Encrypted drafts, retries and signed versions; restore an older version as a new one | **Built** |
| **Plans** — a page with typed rows, costs in exact cents and a budget, shown as a card in chat | **In progress** (canonical writer and approvals migration open) |
| Full block editing UX, comments, nested pages, live bindings, two-device convergence | **Planned** |
| Agent-maintained sections | **Planned** |

## Agents

Agents are **first-class members** of Zoen — not a bot bolted onto a chat.

| Capability | Status |
| --- | --- |
| Own encrypted identity and device, profile and avatar | **Built** (runtime base in `zoen-agentd`) |
| Owner authorization: an agent can only register with its owner's signed consent | **Built** ([ADR 0049](../adr/0049-agent-owner-authorization.md)) |
| Shareable agent link with a public page (`/a/CODE`), revocable, with expiry and use limits | **Built** |
| **Creator** agent: describe an agent in conversation, approve it, and it exists | **Built** (prompt-driven; owner approval required) |
| Durable approval cards: "Your agent needs you" before replies or actions | **Built** in the core ([ADR 0048](../adr/0048-durable-agent-approvals.md)) |
| Owner notes ("New booking", "Booking cancelled") | **Built** |
| Sandboxed tools: WASM, microVMs and browsers in microVMs | **In progress** ([ADR 0028](../adr/0028-agent-sandbox.md)) |
| Production model execution, managed custody and billing | **In progress** — deliberately closed until certified |

See [Agents runtime & sandboxes](../technical/agents.md) for how it works.

## Mini-apps

Small apps that open inside a conversation, with shared state everyone sees live. Zoen is an **MCP Apps host** ([mini-apps](../technical/mini-apps.md)).

Mini-apps on `main` today:

| Mini-app | What it does |
| --- | --- |
| **Agenda** (`schedule`) | Booking for any business: services, prices, weekly hours, free slots. Clients see busy times only, never other clients' names. |
| **Saturday hike** (`hike`) | Three trails with maps and photos; vote, compare, then the agent plans the day. |
| **Poll** and **Group list** | Vote on options; a shared "what to bring" list. |
| **Group pet**, **MapTap**, **Dinner** | A group pet with a mini-game, a turn-based geography game, a shared recipe with live servings. |

Remote MCP servers, a public catalog and the Store are **planned**.

## Look and feel

| Capability | Status |
| --- | --- |
| Apple: glass surfaces for cards and buttons (custom shader on `main`) | **Built** |
| Apple: system **Liquid Glass** | **In progress** — on the `ui/native-glass` branch, not yet on `main` |
| Android: Material 3 with Jetpack Compose, adaptive navigation (bar, rail, two panes) | **Built** — a fuller native Material pass is in review |
| Chat backgrounds: colours, gradients, doodles and encrypted photos | **Built** |
| A sky background that follows the person's local time of day | **Planned** |
| Hand-drawn, animated mascot and illustrations; Reduce Motion respected | **Built** |
| Plain language everywhere — no technical jargon in the interface | Product rule |
