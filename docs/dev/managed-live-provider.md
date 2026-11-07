# Synthetic managed-agent live provider probe

Tracking: #175 / #170. Product direction: Enzo. Technical implementation: Codex.
This opt-in probe runs the actual managed service, owner Core, encrypted relay,
PostgreSQL and FoundationDB against one external chat provider. It does not use
the app's account, provider configuration or conversation data. It is not iPhone
acceptance, public deployment, scale evidence, or a live Clef/tools qualification.

The regular managed-service journey keeps its deterministic HTTP fixture. This
additional test is ignored by default and requires all three environment values:

- `ZOEN_LIVE_REPLY_QA=1`;
- `ZOEN_LIVE_REPLY_PROFILE`: absolute path to a private (0600) JSON file;
- `ZOEN_LIVE_CHAT_TOKEN`: an operator-supplied credential inherited in memory.

Use supported Wrangler authentication or an explicitly scoped API token. Never
copy Codex authentication into Zoen or place credentials in command arguments,
Git, the profile JSON, issue comments or test output. The profile contains only:

```json
{
  "endpoint": "https://api.cloudflare.com/client/v4/accounts/<32 hex account ID>/ai/v1/chat/completions",
  "output_directory": "/absolute/private/probe-output"
}
```

Create the output directory with mode 0700. The probe claims `one-attempt.json`
with exclusive creation and synchronizes it before starting a fixture. Reusing
that directory is refused. A failed or ambiguous run must retain its marker,
logs and fixture; inspect the original attempt rather than automatically retrying.
After setting the existing isolated journey database/FDB environment, run once:

```sh
scripts/cargo-local.sh test -p zoen-cli --test journey_reply_runtime_device \
  managed_live::managed_live_chat_provider -- --ignored --exact
```

The pinned model is `@cf/meta/llama-3.3-70b-instruct-fp8-fast`, through the existing
Rig/OpenAI-compatible gateway. The Gate is explicitly the offline floor. This
does not change product provider defaults or enable offline Gate implicitly.
The approved synthetic budget is USD 0.01 with USD 0.003 maximum per attempt.
The finite request is capped at 4,096 bytes, the input admission at 8,192 tokens,
and output at 96 tokens. Pricing observed on 2026-11-07 is USD 0.293/M input and
USD 2.253/M output, recorded in the marker. These are configured admission rates,
not a provider invoice or a general-purpose token estimator.

Sources: [OpenAI compatibility](https://developers.cloudflare.com/workers-ai/configuration/open-ai-compatibility/)
and [model/pricing](https://developers.cloudflare.com/workers-ai/models/llama-3.3-70b-instruct-fp8-fast/).

The probe requires one accepted answer containing its synthetic identifier, one
runtime attempt, one positive usage-based settlement within the signed budget,
then the same decrypted answer exactly once after service restart and owner cold
recovery. Attempts and settlement must remain unchanged. The ordinary fixture's
20-second observation is preserved; the explicit live probe observes the existing
90-second transport budget plus 30 seconds of lifecycle overhead.

Temporary chat credentials exist only in this probe's private operator vault.
Success and unwinding remove those copies before fixture diagnostics, preserving
accounts, evidence keys, failure fixtures and the dispatch marker. `outcome.json`
records completion and credential retirement without prompts, replies, account
IDs or tokens. An external kill/power loss can prevent cleanup: inspect and retire
only that probe's `reply-runtime/**/vault/chat` copies before sharing diagnostics.
