# Agent conversation classification and recovery

The local Agent host uses the managed Codex Brain and Cloudflare Workers AI
Clef through the existing factories. Clef checks policy; it does not generate
replies or grant permission. Configuration remains in the host's protected
environment, separately from the Agent's owner-reviewed definition. See
[provider configuration](independent-agent-enrollment.md#local-host-provider-configuration).

## Direct conversation input

An authenticated conversation message enters the Gate at the typed
`ConversationMessage` point. Its injection question distinguishes ordinary tasks,
clarifications and permission replies from attempts to override higher-priority
instructions, impersonate authority, bypass permission, exfiltrate data or smuggle
instructions through embedded content. Claimed roles in text confer no authority.

Tool, web and App output retains the separate `Inbound` check. The tool-call
questions, hard rules, confidence thresholds, canonical approval, worker checks
and retry bounds are unchanged. A direct message can still be blocked or held;
this distinction does not allow owner text to bypass authorization.

Offline regressions exercise the typed boundary and both instruction-injection
questions. They prove request construction and preserved policy behavior, not
hosted classification accuracy.

## Bounded outcome diagnostics

The CLI emits best-effort `zoen.agent.outcome.v1` diagnostic lines for selected
continuation paths, runtime outcomes, and enqueue/staging results. Phase, outcome,
request status and optional rule class use closed vocabularies. The source
correlation is a domain-separated, length-framed SHA256 of the Space and source
locators. A separate `requestCorrelation` includes the exact returned Request
locator. Neither correlation is a content hash or an authorization token. The
trace excludes message text, persona, rules, tool names, arguments/results, raw
errors, audit summaries, content hashes and credentials.

`continuation: normal` selects the ordinary turn path; it can include previously
recorded steps and does not mean a new task. `approved` selects the existing
approved-call path. `progress` selects completed-result recovery after the
existing core verification and persistent attempt reservation. These events occur
before Runtime continuation; they do not prove that its remaining checks passed,
that the model consumed the context, or that a new effect executed.

A held `ToolAsk` reports `completedSteps`, `failedSteps`, `remainingCalls` and the
zero-based `roundIndex` from its actual runtime object. Failed steps are a subset
of completed context steps, not successful effects. Approved/progress paths also
report their retained context counts; the normal path has only step counts.
Counts are bounded by the existing runtime limits; invalid counts are omitted,
never clamped or treated as zero. None measures provider dispatch, internal
retries, billing or accepted mutations.

`approval_staging: ok` reports the Request actually returned by the core, its
projected status and held step index. Staging may return an existing Request. The
same Request correlation identifies the same canonical locator; it does not claim
that a Request was newly appended, that approval was reused as permission, or
that authorization remains current. Selected continuation early returns now emit
closed stop categories for waiting/denied/cancelled/stale requests, missing or
invalid continuation state, refused recovery/admission/reservation, the attempt
limit, and an existing reply. This does not trace every host early return.

`reply: queued` and `owner_note: queued` confirm enqueue only. Diagnostics confer
no delivery, approval, execution or mutation authority, and failed diagnostic
writes cannot change a task result or trigger retries. Existing CLI output may
contain private content; the trace does not make all stdout suitable for logging.
A collector must validate the phase-specific fields and counter bounds and
otherwise discard the entire record, including unknown or raw fields.

[Trace contract tests](../../crates/zoen-cli/src/agent/trace/tests.rs) exercise
actual typed outcomes, payload canaries, bounded counts, Request correlation and
failed writers. The existing
[offline canonical read/cold-recovery fixture](../../crates/zoen-cli/src/agent/canonical_read_recovery_tests.rs)
asserts actual emitted normal, pending, recovered-progress and reply-dedup paths
alongside its accepted-effect and cold-replay checks. Its controlled interruption
invokes Runtime directly, so it does not qualify a joined normal-host approved
path or the real model/MLS/iPhone journey below.

## Read-context contracts

The signed WASI worker's actual read result reaches the next fake Codex process
as exact `completed_tool_steps`, including slide/shape IDs, content, scope and
head. Replacing it with only a receipt fails the same assertion. A separate
approved/cold-recovery fixture checks exact read JSON at the next controlled
Brain, retained checkpoint and cold continuation. Seven ordinary fake-Codex
cases and both explicit WASI cases passed, with strict affected Clippy and
formatting. These are adjacent offline contracts, not one joined real-model,
native MLS editing journey or proof that a model chooses the requested edit.

## Actual local MLS checkpoint — 22 October 2026

Native26 created a real DeckCraft App in an MLS conversation with an independently
keyed Agent. An ordinary task and its subsequent permission answer survived a
process-cold app relaunch. The initial host returned a conversational permission
question; that first attempt did not establish a canonical tool approval or edit.

After the typed Gate correction, a new ordinary task produced an actual held
**Read presentation** request. The native review showed the App's v1 and action,
and one **Allow once** showed Approved. The bounded host window had already
ended, so a separate normal host resumed the same source without sending another
task or textual permission reply.

That resumed host emitted a ToolAsk for the original source and another for an
approval receipt. The native timeline exposed the receipt's public source ID;
its computed locator correlation exactly matched the second trace outcome.
Source inspection found that canonical tool resolution projected both its System
entry and an owner-authored ordinary Message containing the receipt. The host
could therefore consume its own approval status as a new task. This is an actual
observed defect; the earlier source audit, recorded before the native correlation
was available, correctly left that association unverified.

Core now omits the synthetic Message specifically for typed canonical tool
decisions. The signed RequestResolved event, System entry, status, receipt and
original-source continuation remain. Legacy item-proposal delivery keeps its
existing Message and card. This changes projection, not the wire format, grants,
permission checks or signed history. Reprojection will refuse a historical child
Request whose sole source was that removed synthetic receipt; it does not cancel
or delete its signed events. The original genuine-source Request remains valid.
The focused batch passed 53 core approval tests and strict all-target Clippy.
That Native26 observation used the earlier Core. The later Native27 installation
checkpoint below is separate from real Agent editing acceptance.

Both hosts exited normally, with their owned process groups absent. A normal CLI
read after the resume recovered the same v1 file, 4,592 bytes and unchanged
SHA256. No Agent edit was accepted. The diagnostics do not establish whether the
approved read executed or errored before the next ToolAsk. No Gate answer,
threshold, permission, model proposal or retry budget was overridden to obtain a
pass.

The outcome receipts are retained under
`zoen-agent-live-mls-outcome-v330` and
`zoen-agent-approved-resume-outcome-v340` in the local build evidence directory.
The original partial attempt, operator claims and decisions remain preserved.
Real Agent editing, fresh manual editor input, completed-result recovery in this
native journey, public operation and production/scale acceptance remain open.

A later Native27/Core29 build from `5a9a8d07` was installed into the same QA
profile with its data retained; its bundle manifest and strict code signature
matched the recorded installation. After a cold open and stable wait, the latest
visible viewport retained the original task and approved **Read presentation**.
It did not show the old synthetic receipt row or its child Read request that had
been visible on Native26. This snapshot used a private-AX fallback and reported a
repeated-navigation warning: it does not prove deletion or a complete history
reprojection, nor a new accepted Agent edit.

A separately qualified normal CLI from `005c43bc` used the existing public API to
recover the unchanged v1 baseline: 4,592 bytes with SHA256
`c1eabc895761e375e7516389cd7f4b029d29fef0da9382effbb831b0453b42d9`.
This checkpoint made no new task, owner approval, host run or model/provider call.
Native27 uses `5a9a8d07`; the CLI uses `005c43bc`. These are separate source and
artifact qualifications, not a joined editing journey.

The subsequent Native27 journey used a new ordinary task sent after the normal
host reported readiness. The iPhone displayed its exact **Read presentation**
proposal, and the owner approved it once. The real-provider host then reported
one completed step, zero failed steps, and a pending **Update slide notes**
proposal. It exited normally with its owned process group absent. The agent's
normal API read retained that proposal, but the iPhone's conversation and
Activity view did not expose it, including after a cold app open. No Write was
approved or accepted. A fresh normal API recovery still returned v1, 4,592 bytes,
with the baseline SHA256 above. The original operator claim and result remain
unchanged under `zoen-normal-host-native27-v359`.

Source investigation identified a feature-dependent JSON boundary to reproduce:
the CLI enables `serde_json/preserve_order`, while the native Core does not.
Re-serializing a parsed multi-field tool argument can therefore produce different
bytes. This observation does not by itself establish the exact rejected native
event; the cross-feature regression and corrective change are tracked separately.

On the same installation, actual portrait and both landscape orientations were
observed in the editor. The Notes panel stayed open across rotation, the editor
fitted the available area, and closing it returned to the same conversation.
The post-rotation normal API recovery retained the exact baseline file. These
checks establish layout and panel-state observations only; manual text input,
unsaved-edit recovery and all-App rotation acceptance remain open.

The corrective source `904201a7` freezes recursively sorted tool arguments and
command actions, and pins the shared `float_roundtrip` parser. Native-default
and CLI-feature approval batches each passed 24 cases, including the reproduced
key-order and precision failures, signed noncanonical-event retention, exact
intent vectors, reload and one-time approval. Existing signed proposal strings
and hash domains are unchanged. Older noncanonical events are retained and
refused; this does not migrate arbitrary previously accepted command receipts.
The actual QA presentation had no accepted Write before this change.

With that normal CLI, canonical recovery resumed the existing approved Read
continuations and produced Write proposals that were visible on Native27. The
operator cancelled the older Native26 QA proposal, reviewed the Native27 text
against v1, let the host exit normally, then approved that exact Write once on
the iPhone while the host was offline. A fresh normal host resumed the same
original source and delivered its reply in the native conversation. A planned
new ordinary task was therefore never sent; the earlier operator binding and
claim remain preserved, with the actual dispatch outcome recorded separately.

Fresh normal API processes recovered exactly two versions: owner-created v1 and
Agent-authored v2. The accepted file is 4,629 bytes, SHA256
`a3052589efca4a2ff6d6a547ab9efdc5873e6557144ca41f079fbdda090f2ed7`.
Only the first slide's notes changed, to
`Native27 shared editing verified - 2026-10-22`; every other JSON value and the
MIME entry matched the baseline. Both bounded hosts exited normally with their
owned process groups absent. Evidence is retained under
`zoen-normal-host-json-interop-v364` and `zoen-canonical-write-resume-v365`.
This qualifies the observed text/integer Notes journey on Native27/Core29, not
precision-f64 compatibility on that older installed Core. The rebuilt Core30
and manual editor input/save/reopen/export batch remain separate pending work.

Source: [Gate points](../../crates/gate/src/lib.rs),
[citizen runtime](../../crates/zoen-agentd/src/citizen/host.rs),
[normal host](../../crates/zoen-cli/src/agent.rs) and
[diagnostic trace](../../crates/zoen-cli/src/agent/trace.rs).
