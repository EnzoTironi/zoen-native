# Owner-reviewed Agent behavior

The implemented iOS entry is the registered Agent's profile → **Behavior**. The
owner edits what the Agent does and its rules, reviews the complete **Before** and **Your changes**
text, then explicitly chooses **Apply these changes**. The Agent keeps its
identity. Editing does not grant chat, App, tool, trust or spending authority,
configure a provider, start the host, pause/resume it or revoke it.

This checkpoint is `dfd76938`, including Core `206543be`, exact raw relay comparison
and the compiler-aware editor action fix. Native26 passed a bounded real owner
edit and process-cold recovery against the private local relay. Enrollment's
earlier native journey and the remaining MLS/provider/tool journey are separate.

## Read, review and apply

Opening or focusing the editor first queries its protected local recovery receipt.
Only a successful absent result permits a fresh authorized definition read.
Receipt-load or read failure keeps editing locked. A retained attempt restores
its immutable review and offers **Check result**; it does not send Apply again.

The current editor accepts the shared strict canonical User definition: Cheap
tier, empty state, version 1 through `i32::MAX`, a nonempty persona of at most
768 UTF-8 bytes, and up to eight nonempty rules of at most 256 UTF-8 bytes each.
The complete encoded definition is limited to 1,024 bytes. Persona may contain
newlines; other control characters and all rule control characters are refused.
Unknown, duplicate, missing, noncanonical or unsupported fields fail closed.
These are this editor profile's supported bounds, not final product tier limits.

Only persona and rules are editable. Whitespace, Unicode, rule order and duplicate
rules remain exact; the editor does not trim or normalize them. It preserves the
definition kind, tier and state. An unchanged edit, overflow or unsupported
definition is refused. Paused Agents stay paused; revoked behavior is read-only.
Back/Done leave a dispatched attempt intact. Discarding unsent local text requires
an explicit confirmation.

The typed commands use existing `Client.client_request` / native vault RPC:

| Command                      | Input beyond the common scope                          | Result                                               |
| ---------------------------- | ------------------------------------------------------ | ---------------------------------------------------- |
| `agent_definition_read`      | None                                                   | Current `AgentDefinitionDto`                         |
| `agent_definition_preview`   | Observed version/hash and exact proposed persona/rules | Immutable Current/After review and digest            |
| `agent_definition_update`    | The same edit plus its observed preview digest         | Held update receipt and reconciled outcome           |
| `agent_definition_reconcile` | None                                                   | The retained update's current observation, or absent |
| `agent_definition_dismiss`   | Exact receipt ID and observed preview digest           | Whether that same receipt was removed                |

Every command supplies `agent_id`, `expected_identity_id` and
`expected_device_id`. Core binds the registered, unlocked Person owner to its
current certified device, certificate and relay. Requests to the relay additionally
require the matching authenticated connection; absent-receipt probes and local
dismissal need no network request. Core rechecks context around awaits. UI scope,
generation and revision checks retire
stale account, device, relay, route, text and dialog callbacks.

## Relay authority and concurrent changes

The owner branch of existing `Status` returns bounded exact definition bytes,
kind, version and status; runtime cursors remain empty. Bytes above 64 KiB are
omitted to retain ordinary status compatibility. The editor refuses absent,
oversized or incompatible bytes, including an older relay's version-only reply.
Decoded kind/version must match relay metadata. A version number alone never
proves definition equality.

The review binds exact before/after bytes, status and full local scope. Update
reads current state again and submits only the reviewed observed version + 1.
Existing `Define` serializes the Agent row and requires a strictly increasing
version. A competing forward update committed first therefore rejects this exact
next version. The editor never bumps the version or retries automatically. This
is monotonic version fencing, not an expected-hash server CAS. The runtime can
install its initial User definition only while the current version is zero;
later behavior changes require the owner.

Scope retains both the validated canonical relay authority and the exact stored
configured string. The DTO's `relay_url` equals `AccountDto.relay_url` verbatim;
UI compares it exactly. Core passes that configured string to the existing
authenticated channel check. Accepted slash, ws/wss, bare host, host-case and
default-port spellings therefore need no duplicate JavaScript URL parser. A raw
configuration change invalidates the old review even if its canonical authority
would be equivalent.

## Uncertain outcomes and retained receipts

Before any possible `Define`, Core writes and reads back a bounded 8 KiB receipt
in the same protected owner database. It contains the full immutable review,
owner/device/certificate/relay/Agent scope and a receipt ID; it is recovery context,
not permission or cancellation. Persona and rules must not be logged or exported
as public diagnostics. A load, write or read-back failure never dispatches Define.
Duplicate held attempts reconcile read-only.

| Outcome     | Meaning                                                                                                                                       |
| ----------- | --------------------------------------------------------------------------------------------------------------------------------------------- |
| `Confirmed` | Current non-revoked version, kind and exact bytes equal the reviewed next definition. It confirms current state, not which request caused it. |
| `Conflict`  | Current definition differs from both reviewed states, or access was revoked. Review current behavior before another edit.                     |
| `Unknown`   | No usable current result, or the exact old base is still observed. An earlier request may still arrive.                                       |

A successful Define ACK alone does not confirm the displayed result. A lost ACK,
timeout or old-base observation does not cancel an in-flight operation. Reopening,
focus and **Check result** use read-only reconciliation, without automatic resend,
version change or implicit confirmation.

**Start another edit** explicitly dismisses only the matching receipt ID/digest
under the same scope. Its confirmation explains that a sent change may still
arrive. Dismissal does not cancel it or revoke the Agent; Back/Done/refresh never
dismiss it. A stale dismissal cannot remove a newer receipt.

The raw+canonical follow-up preserves the original five-field ordered receipt
lookup key. Raw configuration is required in new receipt contents and the review
digest. An older v306 receipt lacking it, or a same-key receipt from another raw
configuration, remains stored and fails closed; it is not migrated, erased or
treated as permission for a fresh dispatch. The six generated wire shapes did
not change.

## Verification boundary — 22 October 2026

Core `206543be` passed 16 definition tests, 16 enrollment regressions and strict
all-target Clippy. Private ephemeral WebSocket tests cover read/review/update,
lost ACK, cold recovery, raw relay changes and legacy receipt refusal. Actual
isolated PostgreSQL tests passed four relay cases, including concurrency,
owner-only bytes and size bounds. UI's exact-raw follow-up passed 86 affected
tests. The earlier `bb5e1274` baseline passed 1,713 integrated app tests and
Core28/Native25 packaging, but did not prove that the compiled editor accepted input.

Native25 exposed a real no-op: React Compiler cached null action tokens from the
first render because their reads depended only on the stable mutable session.
The route now passes its subscribed immutable snapshot to `ticket`, `confirmation`
and `dirty`; stale snapshots are refused. The actual compiler-transformed route
regression retained memo caches across activation: original source failed two of
three cases; the fix passed four focused and 90 affected cases. This changes no
header geometry, Core contract or provider authority.

At `dfd76938`, all 1,717 integrated app tests, types, Oxlint and Oxfmt passed.
Native26 Release/signature passed with exact unchanged Core28 reuse; all 420
installed QA bundle files matched before and after. Normal owner UI read the
current definition, accepted and read back the edited persona with the existing
rule unchanged, reached Review with the keyboard open, hid it on one explicit
Review, and showed Confirmed after one explicit Apply. Process-cold relaunch
preserved the account and Agent, restored Confirmed before/after/current text
through reconcile-first recovery without another Apply, and **Concluir** returned
to the same Agent profile. The main simulator was untouched.

This proves that bounded native owner-edit journey. The UI does not display the
numeric definition version, and an independent wire Define count was not measured.
The next normal host read must verify the exact version and bytes. No model call
or host startup occurred. Real MLS/provider/tool approval, persistent host
supervision, public production and scale acceptance remain separate journeys;
global readiness flags remain false. Provider
selection and authentication are documented in
[independent enrollment](independent-agent-enrollment.md#local-host-provider-configuration).

The retained native receipt is `zoen-owner-definition-native26-v321/source-ready.json`
in the local build evidence directory. The initial compiled failure and focused
fix remain under `zoen-owner-definition-compiler-v319`; the earlier source/build
proofs retain their original checkpoint.

Source: [Core contract](../../crates/client-core/src/agent_definition.rs),
[relay Status/Define](../../crates/zoen-relay/src/agents/mod.rs) and
[native controller](../../apps/zoen/src/agent-definition/session.ts).
