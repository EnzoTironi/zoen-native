# ADR 0021: Telemetry without personal data

Status: accepted (S7)

## Context
The relay logged to stdout and served Prometheus counters, nothing more. A slow delivery
couldn't be followed from the WebSocket frame through the FoundationDB transaction to the
other relay node that delivered it. Telemetry is also the easiest place to leak what the
rest of the system works hard to protect: a raw identity in a log line, a URL with a blob
hash, a client address on every span. An exported log or trace leaves our hands and goes to
vendors, dashboards and laptops.

## Decision
- **OpenTelemetry, OTLP/HTTP, opt-in.** `OTEL_EXPORTER_OTLP_ENDPOINT` turns on export of
  traces and logs (`crates/zoen-relay/src/telemetry.rs`), and the standard `OTEL_*` variables
  apply. Unset means stdout only, which is how staging on Fly runs today. The exporters run on
  their own threads with batching, so a slow collector costs memory up to the queue bound,
  never request latency.
- **Metrics stay Prometheus.** The relay keeps its `/metrics` text, and the collector scrapes
  it. Labels are bounded enums (scope, outcome), never ids.
- **What a span carries:**
  - Names: `handshake`, `request` (with `op`), `publish` → `fdb.append`, `sync`, `ephemeral`,
    `bus.deliver`, and `GET /v1/sync`-style HTTP spans that record the route template, never
    the URL. Health probes aren't traced.
  - Fields: keyed pseudonyms (`p:` + 12 hex of SHA-256(key ‖ id), from
    `ZOEN_LOG_PSEUDONYM_KEY`, shared by a cell's replicas so their logs correlate), the
    partition, the sequence number, counts, an `outcome` and constant reject reasons.
  - Never: a raw identity, device or Space id, handle, invite code, chat name, envelope
    byte, URL or client address.
- **Only our code is exported.** The OTLP layers accept spans and events whose target is
  `zoen_relay` and nothing else, so a dependency that logs a SQL statement, a header or a URL
  at debug level can reach stdout (where `RUST_LOG` governs it) but never the collector.
  Spans become traces and events become log records that carry the trace and span they
  happened in, so nothing is exported twice.
- **Across nodes.** A publish captures its W3C trace context, and the bus carries it as
  `traceparent` and `tracestate` NATS headers. The receiving node's `bus.deliver` span joins
  the same trace, so one trace shows a message from the sender's frame to the other node's
  delivery.
- **One egress.** In Kubernetes the relays send to an OpenTelemetry Collector in the cell
  (`infra/k8s/base/otel-collector*.yaml`, contrib 0.162.0 pinned by digest). It scrapes the
  relays' metrics through pod discovery and deletes `client.address`, `url.*`,
  `user_agent.original`, `enduser.id` and the like from anything that arrives, as defense in
  depth against a producer added later. Overlays replace its `debug` sink with the
  environment's backend.
- **Sampling.** Relays use `parentbased_traceidratio` at 0.1 in the base manifests. The
  parent decision travels with `traceparent`, so a trace is kept or dropped as a whole across
  nodes.

## Proof
- `crates/zoen-cli/tests/journey_telemetry.rs` runs two relay nodes on NATS exporting to an
  OTLP receiver in the test, with `RUST_LOG=debug`. Ana on node A and Bruno on node B chat
  live (typing included), Ana makes a group and an invite, and a third client registers
  through a client-IP header from 203.0.113.77. The test then asserts:
  - every span kind is present;
  - `fdb.append` is a child of `publish`;
  - a `bus.deliver` on node B has node A's `publish` or `ephemeral` as its parent, in the
    same trace;
  - the `space created` log record carries its trace id;
  - none of 18 known secrets appears anywhere in the exported OTLP bytes or in the relays'
    debug-level stdout. The secrets are three identity ids, their device ids, Space ids,
    handles, a name, the message text, the chat name, the invite code and the client
    address.
- The real otelcol-contrib 0.162.0, running this repo's collector config, received a live
  relay's traces, logs and scraped metrics
  (`roda-shots/real-s7/collector-proof.{sh,txt}`); `otelcol-contrib validate` passes and
  kubeconform validates the rendered manifests.

## At 1B users
- **Volume.**
  - Head sampling at 0.1 is the dial; at 1B users it drops to 0.001–0.01 for `publish` and
    `sync`.
  - A two-tier collector keeps the interesting traces whole: a `loadbalancing` exporter
    routes by trace id to a tail-sampling tier, which keeps every error, every rejection
    other than rate limits, and every trace slower than its SLO.
  - Rate-limit refusals are counted, not traced.
- **Cost.** The span set is fixed and small (no per-recipient spans; fan-out records counts).
  A publish is at most three spans per node it touches. Log records are info level and up,
  from our code only.
- **Key rotation.** Rotating `ZOEN_LOG_PSEUDONYM_KEY` breaks correlation across the rotation
  on purpose, so long-term linkage across log retention windows isn't possible. Rotate with
  the 30-day retention.
- **Cells.** Each cell has its own collectors. Backends are per region, so telemetry stays
  in the region whose users produced it (LGPD/GDPR residency).
- **Clients.** App telemetry (crashes, performance) goes through the same rules when it
  arrives: pseudonymous, allowlisted, sampled, behind the same collector scrub.
