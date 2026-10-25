# Runtime-owned Image host

`ImageRuntimeHost` composes the existing native custody, review, model gateway,
finance and original-output transport. The normal host must first call
`RuntimeAuthority::qualify_managed_images_output` on its exact configured route.
That constructor can establish Images-only authority from existing certified
custody, live external continuity and mandatory relay proof. General reply
output and global continuity remain closed; no fixture or Boolean opens them.
See [Images output scope](managed-image-output-scope.md) for the source contract
and current qualification boundary.

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
Runtime's private checkpoint-bound Image transport, described in
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

## Earlier executed host increment

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

## Owner controls without content workers

`zoen --home <existing-agent-home> agent-controls [--for SECS]` runs only the
existing authenticated owner-control reconciler. It uses the same home lock,
account, certified device, relay, private control conversations, native Runtime
and financial authority. It supports active and paused agents without changing
their status. Startup reads the current cursors without seeding or advancing them.

This service does not construct a model, gate, editor worker or image-content
worker, discover approved tool continuations, answer messages or emit read
watermarks. The ordinary `agent-host` command retains its existing behavior.
`agent-controls` rejects `--source`, `--offline`, unknown/duplicate flags and
invalid durations before opening the existing home. It cannot run alongside
another host on the same home.

The control reconciler retains both signed inquiries and already authorized
policy submissions. It is not a read-only permission bypass: installing or
disabling a policy still needs the exact owner's signed confirmation and all
existing freshness, directory, MLS, version and Runtime checks. Reading options
does not create that authority. A missing Runtime Welcome follows the existing
scoped recovery; neither another worker's group nor its readiness is imported.

The fresh readiness marker describes this connected service's startup only.
The existing closed worker observation separately reports whether its configured
Runtime transport is available; neither marker nor observation proves an inquiry
was admitted or a policy installed. On exit the service cancels and awaits only
its own control worker before the caller stops sync and releases the home lock.

## Remaining acceptance

Issue #366 remains open. The normal CLI/ToolHost owns a separately enrolled
Runtime device and retained custody; it must not share another Client's active
MLS state. Managed external checkpoints, mandatory relay registration and the
Images-only constructor now have source implementations, tracked by #380,
#386/#394 and #399. Their combined actual host/restart/approval/output journey is
still under qualification. Earlier fixture results above do not qualify these
successors. Neither an advertised capability alone nor a readable marker grants
Images output: qualification requires the paired live proof and refusal of
legacy binding. The final user journey must also prove one bill and reopenable
output on iPhone. No paid provider or native acceptance is claimed here.
