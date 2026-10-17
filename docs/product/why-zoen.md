# Why Zoen

> **Zoen is the messenger where friendships, relationships and communities form — and where people build the apps and agents they share with each other.**
>
> *O mensageiro onde a vida acontece — e onde cada um cria seus próprios apps e agentes.*

## The idea in one paragraph

Most of life already runs through a messenger. Friends plan trips there, couples keep their lists there, communities gather there, and small businesses answer every customer there, by hand. Zoen keeps everything people love about a private messenger and adds the thing a messenger never had: **the conversation can produce real things**. A plan with a budget. A page everyone edits. A small app the group plays with. An agent that books haircuts for the barber while he works.

## Who it is for

| People | What changes for them |
| --- | --- |
| **Friends and families** | Plans, lists, polls, games and shared pages live inside the chat, the same for everyone, end-to-end encrypted. |
| **Couples** | Shared lists, plans and moments that stay private to the two of them. |
| **Communities** | Groups grow into communities with their own resources and permissions, kept inside the conversation. |
| **Small businesses** | A barber, a baker or a neighbourhood shop gets an agent that answers, books and asks the owner before anything sensitive. |
| **Creators of apps and agents** | Anyone can create an agent or a mini-app and share it with friends by a link. |

## Principles

These come straight from the product decisions recorded in the repository ([decisions log](../decisions-log.md), [founder notes](vision.md)).

1. **Private by default.** Direct messages and new groups are end-to-end encrypted ([ADR 0027](../adr/0027-end-to-end-by-default.md)). Privacy only goes up: a chat can become encrypted, never the reverse.
2. **Agents are members, not spies.** An agent reads a conversation only when it is a declared member with its own device key. Sensitive actions wait for a person's approval ([ADR 0048](../adr/0048-durable-agent-approvals.md), [ADR 0049](../adr/0049-agent-owner-authorization.md)).
3. **Everything is one conversation model.** Direct chats, groups and communities live in **Chats**; plans, pages, files and mini-apps are items inside them, not separate inboxes.
4. **Native first.** SwiftUI on iPhone and Mac, Kotlin and Jetpack Compose on Android, and a web client for instant entry from invite links. One Rust core underneath, so behaviour is identical everywhere.
5. **No technical jargon in the product.** People see "Approve", "Your agent needs you" and "Link a device" — never keys, epochs or relays.
6. **Honest about what's real.** Demo data is behind an explicit development flag and labelled ([ADR 0006](../adr/0006-demo-behind-a-flag.md)).

## Where Zoen comes from

Zoen **v1 runs on WhatsApp today, with real users**. It proved that people want an assistant inside the conversations they already have — and showed the limits of living inside someone else's messenger.

This repository, `zoen-native`, is **Zoen v2**: the messenger itself, rebuilt from the ground up so that communication, community, work and commerce can share one private place. v2 has not launched yet; the [roadmap](roadmap.md) tracks exactly what is done and what is still open.

## Why now

- **Mini-apps have a standard.** [MCP Apps](../technical/mini-apps.md) lets a tool return an interactive interface inside a conversation. Implementing it as a host means Zoen can run apps built for that ecosystem.
- **Agents can be first-class.** With their own encrypted identity, profile and invite link, an agent can join a chat like a person — and be held to the same rules.
- **Brazil first, ready for the world.** WhatsApp is near-universal in Brazil, small businesses run on it, and the product is designed to ship in Portuguese, English and Spanish ([founder notes](vision.md)).

## Read next

- [Features](features.md) — what Zoen does today and what's coming.
- [Roadmap](roadmap.md) — v1 scope, the barber journey and the status of every area.
- [Architecture](../technical/architecture.md) — how the pieces fit.
