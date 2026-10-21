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

Evidence and raw command results are retained under
`/Users/enzotironi/.codex/builds/zoen-cloudflare-clef-v260/`.
The local schema validation and hosted probes do not establish a durable shared
quota, free-plan enforcement, current-price binding, paid-attempt settlement,
private-content remote consent or production readiness. No native build, main
worktree, simulator, parent service or user authentication configuration is changed
by this isolated branch. The probe does not configure persistent provider
credentials for an existing agent host; that host still requires the documented
environment variables in its own process.
