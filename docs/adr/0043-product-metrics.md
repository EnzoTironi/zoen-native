# ADR 0043: Product metrics without content

Status: accepted

## Context
Enzo set the two numbers that say whether Zoen works: **monthly active users** and
**messages sent per active user**. Growth work also needs signups, activation, retention
cohorts, invites and a viral coefficient, plus reliability guardrails. Zoen is end-to-end
encrypted by default (ADR 0027): the relay can't read messages, and the product promise
is that nobody counts what people say or to whom. The metrics have to come from metadata
the relay already sees to do its job (who is connected, that an envelope was sequenced in a
Space and who it fans out to), with as little of it kept as possible (LGPD art. 6, III).

## Decision
- **Counted at the relay, in memory, on the hot path; written in batches.** `zoen-relay`
  (`src/analytics/`) bumps an in-memory entry per account, Space and counter touched. Every
  15 s (and at shutdown) each node writes the batch with four `unnest` upserts. Cost: one
  row per active account per day per node, not one per message. A failed flush keeps the
  batch for the next one.
- **What counts as a message.** A sequenced plaintext `MessagePosted`, or a sealed envelope
  whose clear framing says `Application` (MLS application data). Commits, Welcomes,
  checkpoints, membership and Space creation aren't messages. Sealed application data also
  carries edits and other in-chat items; the relay can't (and shouldn't) tell them apart.
- **Active.** Sent at least one message *or* completed a sync in the period (opened the
  app and caught up). The strict variant, *senders* (sent ≥ 1), is reported next to it, and
  messages per active user divides by senders.
- **Who is counted.** People only in the headline. Agents (`identities.kind = Agent`) are
  counted apart (agent messages per active user). QA/test accounts are flagged by handle
  prefix (`ZOEN_METRICS_TEST_HANDLES`, default `qa_,test_,e2e_,ana_s,bruno_s,load_`) and
  reported only in the "everyone" population.
- **Pseudonyms, short retention.** Rows name an account or Space only by
  `SHA-256(key ‖ tag ‖ id)[..16]`, key from `ZOEN_METRICS_KEY` (a Fly secret; dev and tests
  fall back to a random key kept in `metrics_key`). Raw ids, handles and Space ids live
  only in memory between flushes. Per-account rows (`metrics_activity`, `metrics_accounts`)
  and per-Space rows (`metrics_spaces`) are deleted after 35 days, which is the longest
  any metric needs (MAU = 30 days, D30 retention = 31). Closed days are rolled up into
  `metrics_daily` (numbers only) and kept.
- **Who someone wrote to** is kept only as a 64-bit sketch: each recipient sets one bit
  chosen by its pseudonym. Two or more bits means "wrote to at least two people", the
  activation signal; it can't name or count anyone beyond that, and a collision (1/64 for
  two people) only ever undercounts.
- **Days are UTC.** Brazil's evening rolls into the next UTC day; consistent beats local.
- **Admin surface.** `GET /admin/metrics` (JSON) and `GET /admin` (a page that draws it),
  only with `Authorization: Bearer $ZOEN_ADMIN_TOKEN`; without the variable both are 404.
  The page keeps the token in `sessionStorage` and loads nothing from elsewhere (CSP).
- **Dashboard: PostHog, aggregates only.** With `ZOEN_POSTHOG_HOST` and `ZOEN_POSTHOG_KEY`
  set, the relay sends one `zoen_daily_metrics` event per closed day, `distinct_id`
  `zoen-relay-<env>`, with the day's numbers as properties and person processing off, and an
  id derived from the day so a retry doesn't double count. No per-user event ever leaves
  the relay. PostHog draws trends and keeps history; `/admin` is the live view.
- **Reliability from the relay.** Send latency (frame received → accepted) and time to
  first sync (login → first `SyncDone`) go into fixed-bucket histograms; failed sends are
  refusals. Crash-free sessions need client reports and come with ADR 0044's opt-in client
  aggregates.

## Proof
`crates/zoen-cli/tests/journey_metrics.rs`: five accounts (one QA) chat over simulated days
on a real relay, Postgres and FoundationDB (the metrics clock is shifted through
`ZOEN_METRICS_CLOCK_FILE`, honoured by debug builds only). The admin endpoint refuses
without the token and reports exact DAU, WAU, MAU, senders, messages per active user,
D1/D7 retention, activation, invites and K; QA accounts appear only in the "everyone"
population. No account id, id prefix or handle appears in the reports or in the relay's
debug log, and the tables hold only pseudonyms.

## At 1B users
- Writes stay one upsert row per active account per day per node; at 100M DAU that's
  ~100M small rows a day, partitioned by day (drop a partition instead of `DELETE`).
- `count(DISTINCT pid)` over 30 days becomes HyperLogLog sketches per day (`hll` or
  ClickHouse `uniqCombined`), merged for WAU/MAU: same pseudonyms, no raw rows needed past
  the day.
- The rollup moves out of the relay into a job per cell; the admin endpoint reads only
  `metrics_daily`.
