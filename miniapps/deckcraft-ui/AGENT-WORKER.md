# Certified-agent presentation worker

The server-side `zoen agent-host` can offer read-only presentation inspection and
four bounded commands through its existing gated `SpaceTools` runtime. This is a
separate WASI-p2 command component over the **same pinned DeckCraft Session and
HostBridge**, not the browser bundle or the upstream filesystem MCP server.

`headless-worker.json` records the source boundary. Apply the existing exact
`0001-host-bridge.patch` to upstream revision
`a31b990d32f2c8562f1588562f922960c3fde48b`, then the shared
`0006-portable-recovery.patch` and `0005-headless-worker.patch` in a fresh private checkout. The shared 0006 patch avoids unsupported OS PIDs on both browser and WASI while
preserving native behavior; browser JavaScript entropy is browser-only. The browser-only managed
start/freeze/UI patches are not needed by this component and remain unchanged.
The supplement adds one workspace package and narrows the engine's JavaScript
clock cfg to `wasm32` **and** `target_os = "unknown"`; WASI uses its standard clock.
No upstream license change is made (MIT OR Apache-2.0).

Build with Rust/Cargo 1.99.1 and installed `wasm32-wasip2`, the same verified local
three-font inputs described in `font-input/upstream.json`, one Cargo job and an
owned process deadline. No network/dependency installation is part of a build:

```sh
CARGO_BUILD_JOBS=1 CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 \
CARGO_TARGET_DIR=/absolute/private-target \
CRAFT_FONTS_DIR=/absolute/verified-fonts CRAFT_FONTS_REQUIRED=1 \
cargo build --release --locked --offline --target wasm32-wasip2 -p deckcraft-zoen-worker
```

The resulting `.wasm` is a WASI-p2 component. Do not use the browser's
`wasm32-unknown-unknown` module, standalone editor, or a substituted mock.
Compilation is separate from actual worker execution and canonical acceptance.

## Explicit node configuration

All three deployment variables are required together:

- `ZOEN_DECKCRAFT_WORKER_COMPONENT`: fixed local component file, at most 64 MiB.
- `ZOEN_DECKCRAFT_WORKER_MANIFEST`: local signed `zoen_agentd::SignedManifest` JSON.
- `ZOEN_DECKCRAFT_WORKER_PUBLISHER`: explicit trusted Ed25519 public key, hex.

The operator signs an existing `ToolManifest` using `ToolManifest::sign`, with
`id = "deckcraft-canonical-v1"`, `version` equal to the pinned engine revision,
`needs = Needs::Wasm`, the **actual** component SHA-256, empty `egress`, and empty
`secrets`. The existing signed WasmTier verifies the publisher, signature and
exact component bytes. There is no default trusted publisher, automatic import,
model-provided executable path, inherited filesystem/environment, or network
access. Absent configuration leaves these agent tools unavailable; partial or
invalid configuration refuses agent-host startup. Device/root identity and
provider credentials continue to use the existing agent runtime paths.

Component compilation happens once at host startup and may be expensive; it is
not the five-second per-command execution budget. Each isolated run retains the
existing 1 MiB global stdout ceiling, has two billion fuel units, a five-second
wall budget and 256 MiB memory ceiling. Failures refuse the proposal.

## Authority, limits and recovery

The component accepts a little-endian 32-bit JSON metadata length, at most
64 KiB JSON, then exact native document bytes. This executor supports at most
**512 KiB per source/proposal**. The canonical 8 MiB document/FFI capacity and
other tools' global limits are unchanged. The lower executor capacity is stated
in tool schemas and errors and checked before compute/save.

The host offers `read_deckcraft`, `deckcraft_text_set`, `deckcraft_slide_notes`,
`deckcraft_shape_insert`, and `deckcraft_slide_new` for each actual DeckCraft Item
in the current chat. Model-facing names use a stable 160-bit SHA-256 digest suffix
under the existing 64-character limit. The host map retains the entire canonical
Item ID (real IDs have 72 characters), and every call requires an `artifactId`
that exactly matches that server binding. Descriptors include the full ID as an
allowed value; a retained call cannot mutate a different Item even if an alias
collides. A collision or insufficient room under the existing 64-tool gateway
limit omits the artifact's whole set. There is no positional fallback or
persistent alias store. Mutation tools require the exact observed version/hash.
Artifact binding comes from the offered tool, actor/device from the signed core,
and source/served member from the real runtime turn. Model/browser arguments
cannot supply that authority. A Rust-only, non-serializable turn captures the
certified agent, owner, device, Space and signed source-content hash. Membership,
source edits/deletion, device retirement, served-writer status, App Act and
canonical CAS are rechecked through the core. Read completion also rechecks
access before returning inspection metadata.

The production Runtime still runs the unchanged Gate before the tool call.
Its default input remains the complete raw call for every other host. DeckCraft
uses a non-serialized, fixed-field reference type after exact offered alias,
full Item ID, strict DTO and native requested-head read validation. Runtime
excludes only those exact public `artifactId`/`sha256` values and uses the server's
canonical operation label. User title and every remaining argument stay in the
Gate; no argument can mark itself public. Preclassification is read-only;
writer, App Act, source, device and receipt/CAS checks occur again during execution
after the Gate and before compute.

Before compute, `app_binary_result` reconciles an exact accepted intent. The
intent includes Item, signed source hash, served member, observed version/hash,
pinned engine revision, tool and normalized action. Reapplying the same action
against a deliberately new observed head is a new intent. An exact original
call/base after lost response or reopening the local store returns its existing
accepted receipt. A missing/evicted receipt with an old base refuses replay. This does
not infer whether a model's newly rebased, semantically similar call is a retry.

The host verifies returned scope/base/mutation/action, byte length and SHA-256,
then saves **only those exact proposal bytes** through the existing atomic
`app_binary_save`. The existing signed Item versions, encrypted media chunks,
outbox, grants and receipts remain the sole persistent authority. Only small
inspection/receipt results enter the existing tool journal; no binary document
is inserted into a model response or a second store.

## Qualification

The separate journey uses the actual gated Runtime, SpaceTools, core and compiled
worker with a test-only scripted brain. It closes and reopens Client/SQLite in the
same test process while retaining the compiled worker. Its scope is durable
store-reopen/lost-response recovery; whole agent-host restart, provider model,
relay and iPhone interoperability require separate product journeys.

The explicit local journey passed with the exact component recorded in
`headless-worker.json`: all four commands, signed canonical versions through eight,
stable bindings across another artifact's insertion, exact-base replay, A/B/A
intent separation and SQLite reopen. Two targeted negative controls fail when
the unoffered-binding fence or typed Runtime projection is removed, then the
restored scoped suite passes.

Cold translation is a material measured limit: the unoptimized native test host
with one Rayon thread took 444.553 seconds to load the compiled component
(previous run: 234.426 seconds). The following entire signed action/reopen journey
took 1.493 seconds in that run. The five-second budget applies to each guest run;
it does not bound startup. No persistent translation cache is configured, and
startup performance remains unqualified for production.

The ordinary test suite explicitly ignores the separate component journey
because it requires an actual external pinned build. Run it deliberately:

```sh
ZOEN_TEST_DECKCRAFT_COMPONENT=/absolute/deckcraft-zoen-worker.wasm \
RAYON_NUM_THREADS=1 CARGO_BUILD_JOBS=1 CARGO_INCREMENTAL=0 \
CARGO_PROFILE_DEV_DEBUG=0 CARGO_TARGET_DIR=/absolute/private-target \
cargo test --locked --offline -p zoen-cli --bin zoen \
  deckcraft::tests::actual_pinned_component_edits_as_certified_agent_and_cold_replay_does_not_duplicate \
  -- --ignored --exact --nocapture
```

This uses a scripted brain only, actual WasmTier/HostBridge/Session, real signed
synthetic source events and SQLite core. It does not call a model provider or a
relay. It is not proof of deployed service operation, cross-device delivery,
physical iPhone rendering, exports or exhaustive upstream equivalence. Preserve
separate build, focused-test and runtime receipts and report failures/pending
work explicitly.
