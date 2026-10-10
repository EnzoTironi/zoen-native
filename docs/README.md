# Zoen documentation

The [README](../README.md) introduces the product. [Roadmap status](roadmap-status.md) is the dated source of truth for implementation, current PR versions, verification and remaining billion-user gates. Read [development setup](dev/setup.md) before running a current-client journey.

| Document category | How to read it |
| --- | --- |
| Current status and contracts | Implementation and requirements, with explicit verification boundaries. |
| ADRs | Decisions and compatibility rules. An accepted design does not establish a deployed product. Early superseded decisions link to their replacements. |
| Product/design policy | Required experience and workflows, including [live pages](product/live-pages.md). |
| Research and dated reviews | Evidence at their original dates and source versions. Availability and market figures have not been refreshed by this cleanup. |
| History | Previous audits preserved for traceability. Their PR states do not describe current main. |

Technical identifiers inherited from the old project name are tracked in the [Zoen naming migration](dev/naming.md). The product has one Chats inbox; the internal `Space` type describes its conversation/permission boundary.

## Current status and contracts

- [Accounts created for the project](accounts.md)
- [Approvals: the core API and what the backend must expose](api-approvals.md)
- [Profiles: the core API for the app](api-profile.md)
- [Zoen architecture](architecture.md)
- [Cost per user](cost-model.md)
- [Decision log](decisions-log.md)
- [Generated art: avatars for agents and photos for groups](design-art-generation.md)
- [Safety gate: local decision models on the Mac](gate-bench.md)
- [Hooks](hooks.md)
- [Infrastructure](infra.md)
- [Mini-apps in the chat (MCP Apps + Wabi's experiences)](mini-apps.md)
- [Zoen implementation plan](plan-real.md)
- [Repensado do zero: pessoas, comunidades e agentes](repensado.md)
- [Roadmap status: the complete Zoen product](roadmap-status.md)
- [Zoen security and privacy](security.md)
- [Zoen system design](system-design.md)
- [Telas de referência e progresso atual](telas-referencia.md)
- [Zoen motion & haptics](ux-motion.md)

## Development

- [Instalar o Zoen no seu iPhone com Apple ID grátis](dev/instalar-no-iphone.md)
- [O app de iPhone no Mac, sem simulador](dev/iphone-no-mac.md)
- [Zoen naming and compatibility](dev/naming.md)
- [Development setup](dev/setup.md)

## Product and design

- [Convites: o playbook do Instinct e como o Zoen vai usar](product/convites.md)
- [HIG acceptance and verification matrix](product/hig-compliance.md)
- [Live pages: behavior and completion contract](product/live-pages.md)
- [Métricas do Zoen](product/metricas.md)
- [UX audit: animations, haptics and micro-interactions](product/ux-audit.md)
- [Visão do produto — anotações](product/vision.md)

## Architecture decisions

- [ADR 0001: The relay is a Rust service on Postgres](adr/0001-relay-in-rust-on-postgres.md)
- [ADR 0002: Authors sign content, the relay signs nothing and chains it](adr/0002-event-format-v2.md)
- [ADR 0003: No passwords: an identity key and a key per device, in the Keychain](adr/0003-identity-and-device-keys.md)
- [ADR 0004: Offline first with a durable outbox](adr/0004-offline-first-outbox.md)
- [ADR 0005: Typing, status and presence are ephemeral](adr/0005-ephemeral-signals.md)
- [ADR 0006: The demo story only behind a dev flag](adr/0006-demo-behind-a-flag.md)
- [ADR 0007: Media travels as encrypted, content-addressed blobs](adr/0007-encrypted-media-blobs.md)
- [ADR 0008: FoundationDB is the system of record for logs and sync state](adr/0008-foundationdb-system-of-record.md)
- [ADR 0009: Per-Space logs and pub/sub fan-out, not per-device inboxes](adr/0009-fanout-logs-not-inboxes.md)
- [ADR 0010: A versioned protobuf protocol with signed bytes kept verbatim and causal links](adr/0010-binary-protocol.md)
- [ADR 0011: Storage behind traits](adr/0011-storage-seams.md)
- [ADR 0012: Infrastructure as code (OpenTofu, Kubernetes, GitOps)](adr/0012-infrastructure-as-code.md)
- [ADR 0013: Hooks as a first-class primitive](adr/0013-hooks.md)
- [ADR 0014: Memory, files, skills and knowledge are Items in the Space log](adr/0014-memory-and-knowledge-are-items.md)
- [ADR 0015: Staging runs on Fly.io (gru), with Cloudflare as the edge only](adr/0015-staging-on-fly-behind-cloudflare.md)
- [ADR 0016: Encrypted profiles](adr/0016-encrypted-profiles.md)
- [ADR 0017: Sortable ids (ULIDs)](adr/0017-sortable-ids.md)
- [ADR 0018: Persisted Space ownership and transaction fencing](adr/0018-space-ownership.md)
- [ADR 0019: Fan-out bus between relay nodes](adr/0019-fanout-bus.md)
- [ADR 0020: Abuse controls](adr/0020-abuse-controls.md)
- [ADR 0021: Telemetry without personal data](adr/0021-telemetry.md)
- [ADR 0022: Measured capacity, and what a billion people would cost](adr/0022-capacity.md)
- [ADR 0023: Per-Space sequencing in memory, batched commits to FoundationDB](adr/0023-per-space-sequencer.md)
- [ADR 0026: End-to-end Spaces on OpenMLS, with member-signed checkpoints](adr/0026-e2e-spaces-on-openmls.md)
- [ADR 0027: End-to-end by default, and privacy only goes up](adr/0027-end-to-end-by-default.md)
- [ADR 0028: Agent sandboxes we run ourselves: WASM first, Firecracker microVMs on demand, browsers in microVMs](adr/0028-agent-sandbox.md)
- [ADR 0029: The Zoen web app](adr/0029-web-app.md)
- [ADR 0040: A viewer for every file, an editor for most, and a native WYSIWYG Markdown editor](adr/0040-files-viewers-editors.md)
- [ADR 0041: Live pages: documents that people and agents keep up to date together](adr/0041-live-pages.md)
- [ADR 0042: Dynamic UI: native declarative views first, MCP Apps HTML in a sandbox second](adr/0042-dynamic-ui.md)
- [ADR 0043: Product metrics without content](adr/0043-product-metrics.md)
- [ADR 0044: Experiments, remote config, acquisition source and server-driven onboarding](adr/0044-experiments-remote-config-onboarding.md)
- [ADR 0045: Linking a second device, with its history](adr/0045-linking-devices.md)
- [ADR 0046: Encrypted server backup](adr/0046-encrypted-backup.md)
- [ADR 0047: MLS recovery without a surviving device](adr/0047-mls-peerless-recovery.md)

## Research snapshots

- [Research: where Zoen agents run code and drive browsers](research/agent-sandbox.md)
- [Análise crítica do Zoen pelas aulas de "How to Start a Startup" (CS183B, Stanford, 2014)](research/analise-critica-zoen.md)
- [Apps virais para universitários e concurseiros no Brasil](research/apps-virais-estudantes-concurseiros.md)
- [Apps virais para estudantes e "concurseiros" fora do Brasil (EUA, Índia, China, Coreia, Japão) + Brilliant como referência](research/apps-virais-estudantes-global.md)
- [Curvas de crescimento de redes sociais — o que funcionou, o que morreu, e o que isso diz para o Zoen](research/curvas-crescimento-redes.md)
- [E2B por dentro: o que o Zoen copia, adapta ou evita no "computador do agente"](research/e2b-sandbox.md)
- [Zoen: custo por usuário ativo vs. receita sem cobrar o consumidor](research/unit-economics.md)

## Dated reviews

- [2026-10-08-interrogate-codex](reviews/2026-10-08-interrogate-codex.md)
- [Response to the 2026-10-08 interrogate pass](reviews/2026-10-08-interrogate-response.md)

## Historical audits

- [Historical roadmap evidence, 9 October 2026](history/roadmap-2026-10-09.md)
