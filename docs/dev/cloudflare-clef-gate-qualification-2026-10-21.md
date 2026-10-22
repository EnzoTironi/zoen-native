# Cloudflare Clef Gate — 21 October 2026

The existing production path is `zoen-cli::agent::gate()` → bounded
`ReqwestTransport` → `gate::Clef` → the existing System One answer mapping.
Clef classifies a check; it does not generate chat replies, grant access or replace
the runtime's current authorization. The separate Brain selection is unchanged.

The CLI already accepts `CLOUDFLARE_WORKERS_AI_TOKEN` and
`CLOUDFLARE_ACCOUNT_ID`. No automatic lookup of local credential files was added.
For the operator probe, the already installed Wrangler 4.151.1 CLI provides its
current OAuth credential through `auth token --json`; its output stays in memory
and is passed only to the probe child process environment. `whoami --json`
established one authenticated account with the AI scope. No token, token hash,
email, account identifier or raw authentication response belongs in the evidence.
Wrangler logs are directed to `/dev/null` through an owned temporary symlink.
No login, profile/configuration change, deployment or plan activation is required.

## Contract and reproduced failures

The [official model schema](https://github.com/cloudflare/cloudflare-docs/blob/production/src/content/workers-ai-models/clef.json)
defines typed `noul`, `choice` and `score` answers, model identity and usage.
The dated source snapshot and its digest are retained with the local evidence;
the `production` URL is a moving upstream reference. The recorded hosted response
uses those fields, including four-decimal probabilities and confidence. Confidence
is validated as supplied; it is not inferred from the winning probability.

Two new regressions ran against the original production files from `654d9c0c`:
a negative inbound probability produced `Allow`, and a context beyond the existing
6,000-character ceiling produced `Allow` after dropping its suffix. Both failed
the required `Ask` assertion. The positive implementation keeps that ceiling and
the established irreversible `Block` policy, but refuses oversized complete
context before any provider call. Admitted context is sent without local truncation.
The ordinary audit summary remains deliberately abbreviated and redacted.

The Clef adapter validates the request before HTTP and validates the Cloudflare
success/error envelope, returned model, complete question keys/types, finite
probabilities/confidence, offered choice, rubric/legend, probability distribution
and reported token-counter types before using the existing answer mapping.
Four-decimal distribution rounding is bounded explicitly; probabilities are never
renormalized and a missing confidence does not become certainty. Invalid replies
use the existing `Ask`/irreversible `Block` failure path, without automatic retry.
Other provider wire formats are not migrated to the Clef schema.
The complete serialized text-only request is capped at 32 KiB, including question
instructions and criteria. This conservative Zoen profile leaves room within the
documented 64K-token model context; it is not a tokenizer implementation or an
upstream maximum. A separate RED against the intermediate 256-KiB budget proves
that two individually admitted 6,000-character fields could otherwise exceed the
complete-request budget. The request is refused without dispatch or clipping.

The retained synthetic fixture has no human data. It checks `2 + 2 = 4`, routes
the topic between mathematics/weather, and rates an explicitly harmless impact.
One hosted request of 423 bytes returned HTTP 200, 324 input tokens and zero
output tokens. This verifies one response contract, not classifier accuracy or
an account's billing/quota policy.

## Qualification boundary

`gate::tests::clef_contract` exercises malformed envelopes, missing/extra answers,
types, ranges, distributions, request bounds, zero dispatch for invalid input,
full-context boundary and preserved failure policy. The opt-in ignored CLI test
`agent::gate_transport_tests::live_clef_configured_gate_synthetic_once` uses the
actual configured Gate and production transport for one synthetic inbound check.
It requires `ZOEN_LIVE_CLEF_QA=1` in addition to the existing credentials and cannot
be activated by an ordinary test run. It reads no Zoen account/profile/history.

Executed against code commit `52365dcb`:

| Batch                                                 | Result                                                 |
| ----------------------------------------------------- | ------------------------------------------------------ |
| All Gate unit tests                                   | 17 passed                                              |
| Actual CLI transport regression tests                 | 6 passed; live probe ignored by default                |
| All citizen runtime journeys                          | 11 passed                                              |
| Explicit configured Gate hosted probe                 | 1 passed; `allow`, `model:clear`, 512 ms provider call |
| Strict Clippy for Gate and CLI including tests        | Both passed                                            |
| Rustfmt, documentation formatting and diff whitespace | Passed                                                 |

The configured hosted probe returned probabilities 0.031 and 0.0166 for the two
inbound-injection questions. The two hosted calls in this qualification are the
small three-type contract request and that production Gate request. There are no
additional retries, background host runs or conversational-model calls in this
slice. The separate runtime journey suite uses its explicit deterministic Brain.

Evidence and raw command results are retained under
`/Users/enzotironi/.codex/builds/zoen-cloudflare-clef-v260/`.
The local schema validation and hosted probes do not establish a durable shared
quota, free-plan enforcement, current-price binding, paid-attempt settlement,
private-content remote consent or production readiness. No native build, main
worktree, simulator, parent service or user authentication configuration is changed
by this isolated branch. The probe does not configure persistent provider
credentials for an existing agent host; that host still requires the documented
environment variables in its own process.

## Canonical approval operator attempt

A later bounded synthetic attempt at source `1ad13832a27dc77365d4610936b505e07c517693`
ran the real managed Codex Brain and Cloudflare Workers AI Clef Gate together.
It reserved two Clef dispatches and two Codex turns, completed in 50.392 seconds,
and performed no automatic rerun. The low-confidence read decision remained Ask;
the owner approved its exact canonical proposal hash through
`Client.client_request` / `decide_tool_request`. After reopening the Client, the
same frozen read executed against the signed DeckCraft worker and returned the
accepted v1 document. This is a signed fixture relay projection, not an iPhone
approval-screen or persistent-daemon qualification.

The run did not reach an edit, publication, accepted-write replay or final cold
proof. The document remained v1 with its original SHA256. A test-only proposal
guard expected positional slide `1` while the real editor returned native slide ID
`257`; a separate offline signed-worker regression reproduced that mismatch. The
actual rejected second model proposal was not retained, so its correctness is
not inferred from the diagnosed guard defect. No runtime permission, Gate rule,
confidence threshold, remote provider response or worker authority was changed to
obtain a pass.

The attempt, its one-shot markers and redacted evidence remain under
`/Users/enzotironi/.codex/builds/zoen-real-canonical-provider-one-attempt-v282/`.
`root-qualified-outcome.json` records the partial result explicitly. The test-only correction at `47499f43` passed an actual signed-worker read
regression and the strict admission negatives before a separate operator attempt.
No prior marker, outcome or counter was reset.

## Corrected-source combined provider qualification

The separately reviewed corrected-source attempt at `47499f43` completed in
64.543 seconds: four actual Workers AI Clef dispatches, three managed Codex turns,
one signed-worker read and one canonical write. Both tool checks retained the
real low-confidence Ask decision. Two exact typed owner approvals resumed the
frozen read and notes edit through the existing Client command. The accepted
read identified native slide `257`; the write used that observed ID and exact
version/hash. The artifact advanced from v1 to v2, published a reply and retained
SHA256 `c418c74ce5d9ae574fa0f04cd599d761384967790c5805ebe3a8b14588d62309`.

Accepted-result replay returned the same v2 receipt with `replayed: true` and
unchanged provider counters. Cold Client reopen recovered the same v2 bytes/hash.
The real factories and ordinary core authorization/CAS/worker path were used;
no Gate threshold, provider classification, grant or budget was overridden.
There was no automatic retry. The previous partial attempt and all its one-shot
claims remain unchanged. Evidence and the bounded Root consistency checks are
under `/Users/enzotironi/.codex/builds/zoen-real-canonical-provider-corrected-v289/`.

This qualifies the synthetic combined provider and canonical approval journey.
Its relay projection is a signed test fixture; it is not the real relay/MLS,
iPhone approval screen or persistent host. Those end-to-end product proofs remain
open, as do managed remote custody, provider billing/internal retry measurement
and production readiness. Local Codex authentication stays delegated to the
supported app-server; no `auth.json` was read or copied by the adapter or wrapper.
