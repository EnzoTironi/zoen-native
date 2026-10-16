# Repository map

```text
zoen-native/
├── crates/                  Rust workspace
│   ├── roda-types           the five primitives and log events
│   ├── roda-log             signed, hash-chained per-Space log
│   ├── roda-store           local SQLite storage
│   ├── roda-mls             end-to-end Spaces on OpenMLS
│   ├── roda-docs            Loro block pages, Markdown in/out
│   ├── roda-grants          trust levels, budgets, act/ask/block
│   ├── roda-gate            safety gate
│   ├── roda-proto           Zoen Sync wire protocol (v4)
│   ├── roda-ffi             engine facade for Swift/Kotlin (UniFFI), local MCP server for mini-apps
│   ├── zoen-web-core        browser adapter (WASM)
│   ├── zoen-relay           the sync relay (axum, FoundationDB, Postgres, NATS)
│   ├── zoen-cli             `zoen`: the core at a terminal; journey tests
│   ├── zoen-load            simulated people for capacity runs
│   ├── zoen-agentd          agent runtime: citizens, Creator, tool tiers, budgets
│   ├── zoen-models          model gateway (Rig)
│   ├── zoen-runtime         financial authority and fenced execution admission
│   ├── zoen-sandboxd        Firecracker sandbox node agent
│   ├── zoen-guestd          agent inside each microVM
│   ├── zoen-egress          sandbox egress proxy
│   └── zoen-liveview        encrypted live view/takeover of browser microVMs
├── apple/                   SwiftUI apps (XcodeGen project.yml)
│   ├── Shared/              features, design system, navigation, sync, widgets
│   ├── iOS/ · Mac/          platform entry points
│   ├── Packages/            the core package (XCFramework + bindings, generated)
│   └── Tests/ · UITests/
├── android/                 Kotlin + Jetpack Compose app (Gradle)
├── packages/
│   ├── zoen-ui              React UI kit, app shell preview, authenticated web client (web/)
│   └── zoen-miniapp-sdk     MCP Apps bridge for mini-apps
├── miniapps/                mini-app sources (hike, schedule)
├── tools/
│   ├── miniapp-build        one mini-app → one offline, hash-pinned HTML file
│   ├── miniapp-gen          headless render harness
│   ├── uniffi-bindgen       pinned binding generator
│   ├── hig-audit            advisory HIG source audit
│   ├── art                  illustration pipeline
│   └── gate-serve           local safety-gate model server
├── infra/                   fly/ (staging), k8s/, tofu/, fdb/, k3d/
├── scripts/                 build, run, journey and service scripts
├── docs/                    product, technical, guides, reference, ADRs, evidence
├── docs-site/               the documentation website (Astro Starlight)
└── .github/workflows/       ci, android, apple-design, web, messenger-search-edit
```

> Crates with a legacy prefix in their directory name are part of the Zoen core; the rename is a planned atomic cutover ([naming](../dev/naming.md)).

## Where to start reading

| If you want to… | Start at |
| --- | --- |
| Understand the data model | `crates/roda-types/src/lib.rs` |
| Follow a message through the relay | `crates/zoen-relay/src/lib.rs`, `crates/roda-proto/src/lib.rs` |
| See what the apps can call | `crates/roda-ffi/src/lib.rs` and `api.rs` |
| See how mini-apps are served | `crates/roda-ffi/src/apps.rs` |
| See how agents are born | `crates/zoen-agentd/src/citizen/` |
| Read the decisions | `docs/adr/` ([index](decision-records.md)) |
