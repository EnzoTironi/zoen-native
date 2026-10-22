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

The CLI emits best-effort `zoen.agent.outcome.v1` diagnostic lines around actual
runtime results and enqueue/staging results. Phase, outcome and optional rule
class use closed vocabularies. The correlation is a domain-separated,
length-framed SHA256 of the Space and source locators; it is not a content hash.
The trace excludes message text, persona, rules, tool arguments/results, raw
provider errors, audit summaries and credentials.

`approval_staging: ok` means staging succeeded or reused an existing request.
`reply: queued` and `owner_note: queued` confirm enqueue only. None proves
delivery, approval, tool execution or an accepted document mutation. Early
returns before the runtime call are not traced. Provider dispatch and internal
retry counts are not measured. Existing CLI output may contain private content;
the trace does not make all stdout suitable for logging.

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
Native26 still contains the earlier Core, so this source correction is not yet
qualified by a fresh installed native journey.

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

Source: [Gate points](../../crates/gate/src/lib.rs),
[citizen runtime](../../crates/zoen-agentd/src/citizen/host.rs),
[normal host](../../crates/zoen-cli/src/agent.rs) and
[diagnostic trace](../../crates/zoen-cli/src/agent/trace.rs).
