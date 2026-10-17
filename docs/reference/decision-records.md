# Decision records

Every significant technical and product decision is an Architecture Decision Record (ADR) in `docs/adr/`. Numbers are stable; gaps are intentional. The dated [decisions log](../decisions-log.md) records smaller calls.

| ADR | Decision | Status |
| --- | --- | --- |
| [0001](../adr/0001-relay-in-rust-on-postgres.md) | The relay is a Rust service on Postgres | accepted (milestone 1) |
| [0002](../adr/0002-event-format-v2.md) | Authors sign content, the relay signs nothing and chains it | superseded by ADR 0010 (format v3 signs exact protobuf bytes and a causal link) |
| [0003](../adr/0003-identity-and-device-keys.md) | No passwords: an identity key and a key per device, in the Keychain | accepted (milestone 1) |
| [0004](../adr/0004-offline-first-outbox.md) | Offline first with a durable outbox | accepted (milestone 1) |
| [0005](../adr/0005-ephemeral-signals.md) | Typing, status and presence are ephemeral | accepted (milestone 1) |
| [0006](../adr/0006-demo-behind-a-flag.md) | The demo story only behind a dev flag | accepted (milestone 1) |
| [0007](../adr/0007-encrypted-media-blobs.md) | Media travels as encrypted, content-addressed blobs | accepted (milestone 4, started during milestone 1) |
| [0008](../adr/0008-foundationdb-system-of-record.md) | FoundationDB is the system of record for logs and sync state | accepted |
| [0009](../adr/0009-fanout-logs-not-inboxes.md) | Per-Space logs and pub/sub fan-out, not per-device inboxes | accepted |
| [0010](../adr/0010-binary-protocol.md) | A versioned protobuf protocol with signed bytes kept verbatim and causal links | implemented (S1) |
| [0011](../adr/0011-storage-seams.md) | Storage behind traits | accepted |
| [0012](../adr/0012-infrastructure-as-code.md) | Infrastructure as code (OpenTofu, Kubernetes, GitOps) | accepted (tools chosen by Enzo) |
| [0013](../adr/0013-hooks.md) | Hooks as a first-class primitive | accepted, built after M2 |
| [0014](../adr/0014-memory-and-knowledge-are-items.md) | Memory, files, skills and knowledge are Items in the Space log | accepted, built after hooks (M2) |
| [0015](../adr/0015-staging-on-fly-behind-cloudflare.md) | Staging runs on Fly.io (gru), with Cloudflare as the edge only | accepted for the staging phase |
| [0016](../adr/0016-encrypted-profiles.md) | Encrypted profiles | accepted (built between S3 and S2) |
| [0017](../adr/0017-sortable-ids.md) | Sortable ids (ULIDs) | accepted (S2) |
| [0018](../adr/0018-space-ownership.md) | Persisted Space ownership and transaction fencing | implemented, with production release gates below |
| [0019](../adr/0019-fanout-bus.md) | Fan-out bus between relay nodes | accepted (S5) |
| [0020](../adr/0020-abuse-controls.md) | Abuse controls | accepted (S6) |
| [0021](../adr/0021-telemetry.md) | Telemetry without personal data | accepted (S7) |
| [0022](../adr/0022-capacity.md) | Measured capacity, and what a billion people would cost | accepted (S8) |
| [0023](../adr/0023-per-space-sequencer.md) | Per-Space sequencing in memory, batched commits to FoundationDB | accepted (S9) |
| [0026](../adr/0026-e2e-spaces-on-openmls.md) | End-to-end Spaces on OpenMLS, with member-signed checkpoints | accepted (M2) |
| [0027](../adr/0027-end-to-end-by-default.md) | End-to-end by default, and privacy only goes up | accepted (M2) |
| [0028](../adr/0028-agent-sandbox.md) | Agent sandboxes we run ourselves: WASM first, Firecracker microVMs on demand, browsers in microVMs | accepted (M3 design) |
| [0029](../adr/0029-web-app.md) | The Zoen web app | accepted (Enzo, 2026-10-09) |
| [0040](../adr/0040-files-viewers-editors.md) | A viewer for every file, an editor for most, and a native WYSIWYG Markdown editor | accepted, being built (research 2026-10-08) |
| [0041](../adr/0041-live-pages.md) | Live pages: documents that people and agents keep up to date together | proposed (2026-10-08) |
| [0042](../adr/0042-dynamic-ui.md) | Dynamic UI: native declarative views first, MCP Apps HTML in a sandbox second | proposed (2026-10-08) |
| [0043](../adr/0043-product-metrics.md) | Product metrics without content | accepted |
| [0044](../adr/0044-experiments-remote-config-onboarding.md) | Experiments, remote config, acquisition source and server-driven onboarding | accepted |
| [0045](../adr/0045-linking-devices.md) | Linking a second device, with its history | accepted (M2) |
| [0046](../adr/0046-encrypted-backup.md) | Encrypted server backup | accepted as a development implementation |
| [0047](../adr/0047-mls-peerless-recovery.md) | MLS recovery without a surviving device | development implementation |
| [0048](../adr/0048-durable-agent-approvals.md) | Durable agent proposals and approval effects | implemented in the Rust core |
| [0049](../adr/0049-agent-owner-authorization.md) | Agent enrollment requires owner device authorization | proposed |
