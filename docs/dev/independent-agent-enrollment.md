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
configuration; they are not enrollment fields. Codex and Workers AI Clef have
their own [provider qualification](cloudflare-clef-gate-qualification-2026-10-21.md).

Resume `agent-enroll-wait` with the same home after an interrupted or uncertain
attempt. It recovers accepted setup before waiting again and confirms the exact
registered identity and definition. It does not start `agent-host`. To cancel an
unused offer, pass its observed public digest to
`agent-enroll-cancel --offer-digest OFFER_DIGEST`; accepted or uncertain setup is
preserved. Cancelling an unused offer is not Agent revocation.

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

The native owner stores a bounded public receipt before dispatch. A known ACK is
displayed as Sent; an ambiguous outcome stays Unknown. Reopening restores the
same receipt. The same approval is not automatically dispatched again. Mailbox
pickup remains destructive and is not an exactly-once durable queue: a lost
response can require a new deliberate pairing. The UI separately checks the
registered public Agent and owner binding before opening its profile.

## Dated verification — 22 October 2026

| Evidence                                    | Observed result                                                                                               | Boundary                                                                                       |
| ------------------------------------------- | ------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------- |
| Core enrollment, signature and pickup tests | Invalid/substituted offers, prepared-key recovery, failed saves and lease cancellation covered                | Synthetic transport/fault tests do not prove hardware durability                               |
| CLI source `651d12a2`                       | 12 enrollment tests, strict all-target Clippy and normal binary build passed                                  | Enrollment does not start the runtime                                                          |
| Actual local relay journey                  | Normal owner approval, production pickup, independent registration, exact definition and cold recovery passed | Explicit opt-in; retains synthetic relay rows; slow natural expiry case was not run            |
| iOS Native24 (`7dd9a49f`, Core26)           | Secure entry, explicit approval, actual CLI/relay registration, profile and process-cold recovery passed      | Dedicated QA simulator; no human pairing, model, tool, message or Space access in this journey |

The actual relay test is ignored by default. It requires both
`ZOEN_ENROLLMENT_ALLOW_LOCAL_WRITES=1` and
`ZOEN_ENROLLMENT_RELAY=http://127.0.0.1:8880`, attaches only to the existing trusted
local service, and never starts or resets it. Run only in that disposable local
test scope; normal protocol writes retain synthetic public history even after
the test revokes its Agent and removes its own local keys. Its separate natural
expiry variant additionally requires an explicit slow-test opt-in.

Native24 used the existing QA relay origin through an owned raw loopback tunnel
to the real local relay. It did not use a signed fixture projection. The same
Agent survived native process restart and CLI resume. All 420 installed bundle
files matched before and after; the main simulator was untouched. Public evidence
is retained under `zoen-agent-enrollment-native24-v304`, with source-bound CLI,
core and relay receipts under versions v302, v293 and v297 in the local build
evidence directory.

The later keyboard correction makes Check and keyboard Go share the guarded
preview action, preserves secure entry, and opts this flow into keyboard insets
and persistent taps. Sixty affected tests and the final integrated app check
(1,627 tests, types, Oxlint and Oxfmt) passed. Native24 predates this correction;
its keyboard geometry, long labels and Dynamic Type await the next native batch.

The owner definition editor, real MLS task with native tool approvals, persistent
host supervision, natural expiry races and physical pairing remain separate
acceptance work. Version-only owner status cannot verify exact definition bytes
after an ambiguous update. None of these partial proofs changes full product,
production or scale acceptance.
