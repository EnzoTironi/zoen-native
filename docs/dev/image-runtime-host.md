# Runtime-owned Image host

`ImageRuntimeHost` composes the existing native custody, review, model gateway,
finance and original-output transport. It is a Rust host boundary, not a running
MCP server or an enrolled agent daemon. The production continuity and output
admission guards remain closed.

## Trusted configuration and requested content

The operator supplies a fixed certified agent/device, definition version, exact
`ImagePriceProfile`, vault key locator and `ImageDeliveryTarget`. Requested data
contains only conversation/source/request locators and the prompt. A model or
user must not construct the operator configuration, native facts or tariff.

The host derives size, quality, background, maximum image bytes and price digest
from that profile. The provider secret is loaded through the configured server
vault key; the public `credential_ref` is not treated as that key. No credential
replacement, default tariff, second Client or owner approval is synthesized.

HTTPS is the default endpoint policy. Its serialized price profile and digest
remain compatible with existing profiles. `LoopbackFixture` is an explicit,
signed profile variant for unpaid local QA: HTTP numeric loopback only. DNS,
private non-loopback addresses and HTTPS endpoints refuse under that variant.
This configuration grants no admission or budget authority.

## Original review and continuation

`stage(space, source, prompt)` stages the runtime device's own signed review.
The exact canonical wire and native capsule activate together before network
publication, under the current source head, SQL directory authorization, native
device lease and FDB root/record compare-and-set. Current source, owner trust,
device, expiry and original pending outbox are checked before sending it.

A lost connection retains that original. A lost real relay `Accepted` is
recovered from the actual source, matching the exact envelope and position;
reopening does not create another review. Review and output connections both use
Runtime's private `connect_bound_output`, described in
[runtime-relay-binding.md](runtime-relay-binding.md).

`advance(space, request)` first applies bounded actual source history to the
native capsule without discovering another chat reply. A partial batch reports
`CatchingUp`; it cannot dispatch. The normal owner's `ApproveOnce` must exist in
that native history. The host reuses the retained original run after output is
prepared, verifies its actor/profile/dispatch binding, and recovers retained
capture/output before considering a provider attempt. An existing financial
attempt without deliverable capture reports `AwaitingRecovery`; it does not
request another generation. SQL absence is an observation, not a permit: the
existing common one-shot admission still decides any dispatch.

Output remains the same canonical FileDoc, retained ciphertext references and
original publication. No result, upload receipt or relay ACK is manufactured.

## Executed scope

Combined executable source: `70e14aec48aef76589646ec11bba308c6806ad48`.

- Core bound transport 1, blob transport 2 and original-review 2 tests passed.
- Full Runtime: 15 passed, 0 failed, 0 ignored; 196.06 seconds actual test time.
- All-target Clippy with warnings denied passed for protocol, relay-admission,
  client-core, zoen-relay and zoen-runtime.
- Protocol binding 3 and admission binding 1 passed on unchanged source in
  `e19016f4`; the later change only corrected a Core test fixture's optional time.

The actual native journey uses the bounded production Images gateway against an
unpaid local HTTP provider, validates the PNG, records one claim/settlement and
uploads/delivers the original through real relay handlers. It loses genuine HTTP
and WS confirmations, reopens custody and recovers the exact original. Actual
wrong SQL/cell, split HTTP route, challenge replay, header stripping and current
revocation cases passed. The existing isolated `cfg(test)` output namespace
capability is the explicit admission substitution; public production guards are
asserted closed. This is not paid provider, iPhone or production launch evidence.

Logs, receipts, original compilation failures and cleanup results are indexed at
`/Users/enzotironi/.codex/builds/zoen-image-runtime-host-v451/evidence-index.json`.
The final owned PG/FDB services were removed/stopped with no cleanup errors.

## Remaining acceptance

Issue #366 remains open. The real MCP/native daemon adapter must consume this
host with retained custody of its own enrolled device; sharing another Client's
live device/MLS state would duplicate authority. Production also needs an
independent restore/old-writer fencing primitive: matching SQL/FDB pair markers
can both roll back, and a vault load/save contract provides no monotonic CAS.
No readable marker or operator boolean opens the current guard. The final user
journey must prove review, restart, revocation, one bill and reopenable output on
iPhone after those prerequisites are implemented.
