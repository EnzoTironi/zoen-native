# Independent Agent enrollment

The iOS owner can connect an independently keyed local Agent host through
**You → Permissions → Agents → Connect**. Enrollment registers that identity and
its reviewed initial definition. Starting the runtime is a separate operation.
It does not export the owner's keys, add the Agent as an owner device, configure
a provider, authorize an App or grant access to existing conversations.

## Operator flow

Build the normal `zoen-cli` binary. Use a dedicated protected host home and the
owner's public identity plus the exact trusted relay origin from existing
configuration. Do not derive the trusted destination from an unreviewed code.

```sh
zoen --home AGENT_HOME agent-enroll-request \
  --owner OWNER_PUBLIC_ID --relay TRUSTED_RELAY_URL \
  --name 'Companion' --handle UNIQUE_AGENT_HANDLE \
  --definition INITIAL_DEFINITION_FILE
zoen --home AGENT_HOME --timeout 8000 agent-enroll-wait
```

`agent-enroll-request` requires a trusted interactive terminal. Its pairing code
and check are private transport material: do not put them in arguments, logs,
screenshots, issues or shared files. Enter the code in the owner's secure field,
review the displayed profile, persona and rules, compare the check through the
trusted channel, then explicitly connect. An automated equality comparison does
not establish physical or human out-of-band verification.

The initial JSON definition requires all six fields: `version: 1`, `kind: "user"`,
an explicit bounded `persona`, `rules`, `state: {}` and `model_tier: "cheap"`.
Unsupported fields and behavior are rejected rather than silently substituted.
Providers and their credentials belong to the host's separate runtime
configuration; they are not enrollment fields. The registered Agent's owner can
later review persona and rules in the iOS [Behavior editor](agent-owner-definition.md).
That update preserves the Agent identity and grants no additional access.

Resume `agent-enroll-wait` with the same home after an interrupted or uncertain
attempt. It recovers accepted setup before waiting again and confirms the exact
registered identity and definition. It does not start `agent-host`. To cancel an
unused offer, pass its observed public digest to
`agent-enroll-cancel --offer-digest OFFER_DIGEST`; accepted or uncertain setup is
preserved. Cancelling an unused offer is not Agent revocation.

## Local host provider configuration

For the managed Codex Brain with hosted Workers AI Clef Gate, explicitly select
`ZOEN_BRAIN=codex-local` and set `ZOEN_CODEX_BIN` to the existing absolute path to
the supported Codex executable. The adapter currently requires `codex-cli 0.163.0`;
another version requires adapter qualification. `ZOEN_CODEX_MODEL` is optional and
must identify a model in that managed account's returned catalog.

Supply `CLOUDFLARE_ACCOUNT_ID` and `CLOUDFLARE_WORKERS_AI_TOKEN` through the host
process's protected environment. With `ZOEN_AGENT_GATE` unset, the existing Gate
factory uses Workers AI `@cf/cloudflare/clef-flash`. Missing credentials refuse
startup. `ZOEN_AGENT_GATE=offline` is a separate explicit offline mode, outside
the hosted Clef qualification.

```sh
# Cloudflare credentials are already supplied to this process; no values here.
ZOEN_BRAIN=codex-local ZOEN_CODEX_BIN=/absolute/path/to/codex \
  zoen --home AGENT_HOME agent-host
```

The installed Codex owns its managed ChatGPT login. Zoen uses its supported stdio
app-server and checks the account type; it does not read or copy `auth.json`,
import owner keys or fall back to an API key. The Agent home remains separate
from that managed login. This adapter is for explicitly selected local development,
not a hosted or commercial authentication flow. Codex returns proposals with its
own execution tools disabled; Zoen's Gate, canonical approval and ToolHost retain
execution authority. Clef classifies checks and does not generate chat replies.

An unset `ZOEN_BRAIN` selects the separate Workers AI conversational Brain; it does
not select Codex. Provider selection, credentials and provider model IDs are never embedded
in the Agent Definition. The dated [combined provider qualification](cloudflare-clef-gate-qualification-2026-10-21.md#corrected-source-combined-provider-qualification)
used synthetic signed relay input. Actual relay/MLS tasks and native tool approval
remain a separate acceptance journey.

## Host startup and readiness

The normal `agent-host` path waits up to 15 seconds through the existing
`Client.wait_until_settled` before reading Agent state or timeline heads. Online,
synced and an empty event outbox alone are insufficient: unknown identities,
uploads, profile work and MLS work must also be settled. A timeout fails startup.

Existing relay cursors are preserved. A Space without a cursor starts at its
current readable timeline head, so earlier traffic is deliberately skipped. A
valid empty timeline may seed zero; a failed timeline read may not. Each initial
`Advance` must be acknowledged before its cursor enters the local runtime map.
A failure leaves startup failed, even if earlier seed ACKs already committed.
The next startup reads those retained relay cursors rather than rolling them back.

After acquiring the existing local home OS lock, the CLI clears only the known
`agent-ready` marker, before opening the vault/account or configuring providers.
An unremovable marker refuses startup. Only settled sync and all initial seed
ACKs permit fresh marker creation; replacement or creation failure refuses
startup. The unchanged `agent @HANDLE hosting` line and task loop follow success.

The marker records that startup checkpoint, not current liveness, continuous
catch-up, distributed fencing or permission to act. Normal return removes it
before releasing the lock; startup errors stop sync and clean up. Abrupt death
or cleanup failure can leave a filename, which the next locked startup must clear
or refuse. The lock, current grants and runtime admission retain their own roles.

## Persistence and authority

The host owns distinct root, device, agreement and pickup keys. A signed offer
binds its identity, profile, initial definition, expected owner, relay, challenge
and expiry. Approval additionally binds the same certified owner device to that
complete offer. An ordinary device-link code is not an Agent enrollment code.

Prepared keys and accepted proof survive an interrupted installation. Reopening
does not create a replacement identity. The CLI refuses unreadable or incompatible
existing recovery slots, and shares the process-lifetime home lock with
`agent-host`. FileVault uses same-directory atomic replacement, file sync and
Unix directory sync. This is single-key durability, not a multi-key transaction,
hardware power-loss qualification or cross-platform durability guarantee.

The native owner stores a bounded recovery receipt in its protected local account
database before dispatch. Its reviewed persona and rules are private local
metadata, not a public log or export. A known ACK is displayed as Sent; an ambiguous
outcome stays Unknown. Reopening restores the
same receipt. The same approval is not automatically dispatched again. Mailbox
pickup remains destructive and is not an exactly-once durable queue: a lost
response can require a new deliberate pairing. The UI separately checks the
registered public Agent and owner binding before opening its profile.

## Dated verification — 22 October 2026

| Evidence                                    | Observed result                                                                                               | Boundary                                                                                       |
| ------------------------------------------- | ------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------- |
| Core enrollment, signature and pickup tests | Invalid/substituted offers, prepared-key recovery, failed saves and lease cancellation covered                | Synthetic transport/fault tests do not prove hardware durability                               |
| CLI source `651d12a2`                       | 12 enrollment tests, strict all-target Clippy and normal binary build passed                                  | Enrollment does not start the runtime                                                          |
| CLI startup source `9ee341ab`               | Eight startup, six lock and 12 enrollment tests, strict all-target Clippy and formatting passed               | Synthetic loopback/OS fault proof; actual relay-host/provider execution remains separate       |
| Actual local relay journey                  | Normal owner approval, production pickup, independent registration, exact definition and cold recovery passed | Explicit opt-in; retains synthetic relay rows; slow natural expiry case was not run            |
| iOS Native24 (`7dd9a49f`, Core26)           | Secure entry, explicit approval, actual CLI/relay registration, profile and process-cold recovery passed      | Dedicated QA simulator; no human pairing, model, tool, message or Space access in this journey |

The actual relay test is ignored by default. It requires both
`ZOEN_ENROLLMENT_ALLOW_LOCAL_WRITES=1` and
`ZOEN_ENROLLMENT_RELAY=http://127.0.0.1:8880`, attaches only to the existing trusted
local service, and never starts or resets it. Run only in that disposable local
test scope; normal protocol writes retain synthetic public history even after
the test revokes its Agent and removes its own local keys. Its separate natural
expiry variant additionally requires an explicit slow-test opt-in.

The startup regressions reproduced five failures against extracted original
behavior: unsettled sync, online/synced/empty-outbox with unsettled MLS, unavailable
timeline, refused cursor ACK and failed fresh-marker creation. The same eight
tests passed with strict checks, including delayed multiple ACKs, preserved
existing cursors and valid zero heads. Evidence is retained under
`zoen-agent-host-startup-v323` in the local build evidence directory.
A normal `cargo build --locked -p zoen-cli --bin zoen` produces the dev-profile
operational CLI; this source/test proof does not qualify that built binary or a
CLI Release artifact. Native26/Core28 iOS packaging remains separate and unchanged.

Native24 used the existing QA relay origin through an owned raw loopback tunnel
to the real local relay. It did not use a signed fixture projection. The same
Agent survived native process restart and CLI resume. All 420 installed bundle
files matched before and after; the main simulator was untouched. Public evidence
is retained under `zoen-agent-enrollment-native24-v304`, with source-bound CLI,
core and relay receipts under versions v302, v293 and v297 in the local build
evidence directory.

The later keyboard correction makes Check and keyboard Go share the guarded
preview action, preserves secure entry, and opts this flow into keyboard insets
and persistent taps. Sixty affected tests and the associated integrated app check
(1,627 tests, types, Oxlint and Oxfmt) passed. Native24 predates this correction;
Native25 packages it. Keyboard geometry, long labels and Dynamic Type still await
their Connect-flow native QA. Native26's Behavior-editor keyboard check is a
separate bounded journey.

The owner [definition editor](agent-owner-definition.md) now has typed Core/native
source, exact-byte owner Status reads and protected update recovery. After fixing
React Compiler's cached first-render action tokens, Native26 passed normal owner
read, persona edit with the rule unchanged, explicit Review/Apply, Confirmed UI,
process-cold reconcile-first recovery and Done. The edited Agent kept its identity.
That UI proof does not display a numeric version or independently measure wire
dispatch count; the next normal host read must verify exact version and bytes.
No model or host was started. Version-only status remains insufficient after an
ambiguous update.
Real MLS tasks with native tool approvals, persistent host supervision, natural
expiry races and physical pairing remain separate acceptance work. None of these
partial proofs changes full product, production or scale acceptance.
