# Detached durable agents: implementation review

Reviewed against `09e797742000d841792ab211499dc5ee2349dcc1` on 2026-10-19.
The forwarded **Zoen Agentes duráveis e clientes desacoplados** proposal is an
evaluation input. This review does not approve a storage migration or establish
an accepted product journey.

## Direction and current boundary

Execution should belong to a durable worker. iOS, Android, Web, desktop and a CLI
submit commands and observe the same run; losing an observer must not cancel it.
The CLI is an operational client, not the executor. A durable run needs explicit
submit, observe, steer, approve, pause and cancel operations, with authorization
checked again when each command takes effect.

This direction fits the existing [capability ledger](../agent-runtime-capabilities.md).
The [model gateway](../../crates/zoen-models/README.md) currently performs one
admitted, bounded, non-streaming model transport attempt. It is not a persistent
agent loop. Separate branch tests cover financial authority, retained native
admission, actual MLS hydration and custody slices; they do not prove an
unattended worker serving the application. Existing bounded Excalidraw/MCP App
tests also do not establish durable agent execution.

## Verify the actual SDK before adding another loop

The repository pins **`rig-core = "=0.44.0"`**, with default features disabled.
Its published archive has SHA256
`6c60db6e3da7681abbdfa9637c1a76dbd5a713f935f0a6753d72c5d2770ed4c6`,
matching `Cargo.lock`. All 421 published files matched the installed local source
in this review. That package has neither `AgentRun` nor a `cassette` feature.
Its lower-level recorder interfaces are not a complete durable agent driver.

Current official documentation describes a serializable, sans-I/O `AgentRun`
whose driver supplies model and tool results. It requires preserving its exact
versioned state; resuming through an agent can execute pending tools. That is
a useful candidate continuation mechanism, subject to Zoen's dispatch authority.
The documented API uses the **`rig` facade**, rather than proving these symbols
exist in our current `rig-core` dependency.
See [Rig durable runs](https://rig.rs/docs/concepts/runs/).

Recording/replay is likewise documented through the facade's `cassette` feature.
It can replay recorded effects and reject divergent requests without executing
tools. It remains evaluation evidence, not proof that an external side effect
was cancelled or that an uncertain charge is safe to retry.
See [Rig recording and replay](https://rig.rs/docs/concepts/recording/).

Before selecting it, build an isolated, pinned facade/companion-crate experiment
with no live credentials. Demonstrate checkpoint serialization, pending-tool
restore, refusal of incompatible state, and replay without tool/provider calls.
Keep SDK types private behind Zoen-owned request, checkpoint and receipt types.
No dependency or runtime migration was performed by this review.

## SPACE as an isolated execution adapter

The user supplied [Perplexity SPACE](https://github.com/perplexityai/SPACE).
Source review on 2026-10-19 pins commit
[`a5c67e5ea4fdb3ec0900b734eddd723cea416d2f`](https://github.com/perplexityai/SPACE/tree/a5c67e5ea4fdb3ec0900b734eddd723cea416d2f).
Its useful boundary is a sandbox executor behind the Zoen run driver. Separate
sandbox, operation and observation lifetimes support work continuing after a
client disconnects. Creating a command session does not execute it; attachment
starts the process, with session-derived identity for lost-start recovery.
This does not establish exactly-once external effects.

Keep Zoen's encrypted run journal, approvals, grants, budgets and effect receipts
authoritative. In the inspected source, reconnecting native SSE with
`Last-Event-ID` explicitly reports `cursor_replay_unavailable` and resumes live
output. Its bounded guest memory journal is not durable run history. A detached
observer therefore needs Zoen's retained cursor and committed results, including
explicit handling of output gaps.
See [native SSE](https://github.com/perplexityai/SPACE/blob/a5c67e5ea4fdb3ec0900b734eddd723cea416d2f/crates/libs/data/plane/server/src/native/sse.rs#L238).

Firecracker execution requires a Linux/KVM host tier; the documented setup does
not run directly on macOS or inside ordinary Cloudflare Workers. Cloudflare can
remain the application entry point while execution sits behind a narrow adapter.
This is an architectural inference, not a validated deployment.
See [host requirements](https://github.com/perplexityai/SPACE/blob/a5c67e5ea4fdb3ec0900b734eddd723cea416d2f/docs/installation.md).

Keep identity/custody keys and broad provider credentials outside sandbox process
environment variables. Disconnect, cancellation request, process exit and an
uncertain external outcome need separate retained states. Pin service and guest
artifacts: this source declares 0.8 with a planned breaking 0.9; its public
compatibility policy excludes Rust crate source compatibility.
See [compatibility](https://github.com/perplexityai/SPACE/blob/a5c67e5ea4fdb3ec0900b734eddd723cea416d2f/docs/versioning.md).

No SPACE dependency, host installation or deployment was added. A candidate
adapter must first pass the integrated restart/approval/revocation proof below,
including durable output recovery and no duplicate external effect.

## Authority and recovery requirements

Retain signed grants, exact approval bindings, current device/Space checks,
encrypted custody, integer budgets, exclusive attempt claims, independent
receipt settlement and fenced publication. A restored checkpoint cannot create
a fresh dispatch permit. A model or tool result must be durably retained before
the continuation advances. Unknown external outcomes retain their uncertainty;
recovery reconciles them instead of issuing an automatic paid retry.

Persisted MCP App cards are views of existing authorized items. Opening one must
not repeat the tool that created it. Observer cursors, command deduplication and
run ownership must survive restart. Proactive behavior needs reversible user
preferences and bounded notifications. Streaming must have bounded retention
and resumable cursors; an observer disconnect cannot discard execution state.

## First required integrated proof

On a fresh synthetic account, submit a signed request from iPhone, disconnect
every client, and restart the executor while a real tool approval is pending.
Observe the same run from Web, approve the exact proposed operation, and finish
with one external side effect, one settlement and one signed output. Repeat with
lost acknowledgements, duplicate approval, revocation, cancellation, uncertain
provider outcome and unavailable storage. Record actual durable states and
transport attempts; a mocked loop or process launch cannot satisfy this proof.

The proposal's A01–A18 criteria remain unaccepted as an integrated application
journey. They complement the full product goal; they neither replace its
30 journeys and six platforms nor turn the production or scale gates green.
