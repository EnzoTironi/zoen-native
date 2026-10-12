# Zoen model gateway

This crate implements one model-call boundary. The current profile is
`zoen-openai-chat-v1/rig-0.44.0`: non-streaming OpenAI chat-completions JSON over
bounded HTTP using exactly Rig 0.44.0. Rig types remain private; the public
contract contains Zoen-owned request, tool-proposal, output and receipt types.
It is not the durable agent worker or a billing implementation.

## Admission and outcomes

`ModelGateway::complete` validates and bounds the input and the prepared wire
request before asking the caller's trusted `DispatchAuthority` to admit it.
Preflight supplies a Zoen-owned descriptor with the validated endpoint, model,
credential reference, exact encoded byte count and transport/output bounds.
The runtime matches it to an approved finite price profile; byte count is not
estimated token usage.
The digest binds the canonical request, authenticated context versions, actual
prepared wire shape, endpoint, model, credential reference, limits and profile.
Equivalent JSON object key order produces the same digest. API keys are not
included in that digest.

The authority must independently verify the planned binding and current owner,
agent, device, Space, definition, price and policy. It must reserve and uniquely
claim the attempt in Postgres, then commit current fenced admission in FDB.
Only a known-fresh result may return `Admission::Fresh`. Read/recovery/replay or
commit uncertainty must never reconstruct it. No permissive implementation is
provided. The in-memory test authority proves adapter behavior, not this durable
handshake or owner-wide spending limits.

An admitted call has one transport attempt, with no provider retry, failover or
redirect. A returned `ModelAttemptResult` keeps output validity separate from
the bounded provider evidence and directly reported receipt. Malformed tool
arguments or unknown finish reasons can fail output while retaining usage.
Tool calls are proposals: their object arguments are checked for exact JSON
shape and offered name, but full manifest/schema validation and execution belong
to the future authorized runtime.

Missing counters remain `None`; reported zero remains `Some(0)`. Invalid,
inconsistent or overflowing counters are explicit. Rig's inferred counters and
floating-point catalog costs are not copied into the receipt. HTTP/provider
IDs and complete original bytes remain available even if SDK decoding fails.
Interrupted bodies retain the received bounded prefix with `complete = false`;
an apparently valid JSON prefix cannot prove terminal usage. This is provider
evidence, not a signed invoice or permission to publish output.

The host must seal and durably retain evidence, validate the pinned price
profile, settle once in Postgres and separately check publication authority.
Missing/invalid/uncertain usage must not become a zero bill or release a hold.
Dropping the future may return no result and does not prove remote cancellation.
An admitted attempt must already be durable and remain uncertain. Safe release
requires a durable pre-dispatch tombstone defeating old admission. A late receipt
can settle without authorizing stale run progress or publication.

## Limits and privacy

Production configuration requires HTTPS and a fixed `/v1` base path without
userinfo, query or fragment. The explicit fixture policy allows HTTP only to
literal loopback IP addresses. Destination and credentials are host supplied;
there is no model-selected endpoint or environment proxy. Current maximum
configuration ceilings are 1 MiB request, 16 MiB reply, 120-second total timeout,
256 messages and 64 tools. Lower per-model limits must come from the approved
profile; token limits do not estimate the model's input context window.

Content-bearing DTO `Debug` implementations and error categories are redacted.
Rig 0.44 logs raw requests/replies at TRACE even when content telemetry is
disabled. SDK preparation, construction, polling and future drop therefore run
under a scoped `NoSubscriber`; caller admission and surrounding tracing remain
active. The privacy test uses a global TRACE collector on a multithread runtime
with outside markers and request/key/reply canaries. Sensitive evidence and
serializable content must still remain inside the host's encrypted custody.

## Verification and remaining work

Run `cargo test -p zoen-models --lib`. Behavior tests use an actual local
TCP/HTTP server: pinned request, optional/zero/invalid usage, tool identity,
invalid output with preserved receipt, denied/unknown/replayed/concurrent
admission, semantic JSON ordering, cancellation, loss, partial loss, timeout,
redirect/error refusal, body caps, historical tool-ID reuse, argument shapes
that survive the next turn, partial-counter contradictions and active TRACE
privacy. They use fixture
credentials and make no live provider call.

All other exposed operation variants return `UnsupportedOperation` before
admission: streaming, structured output, embeddings, reranking, transcription,
audio and image generation. There is no sans-I/O agent continuation yet. Durable
SQL/FDB authority, encrypted checkpoints, certified MLS workers, exact approval
resume, integer-price settlement, streaming retention and real deployment
journeys are separate pending units in the
[capability ledger](../../docs/agent-runtime-capabilities.md).
