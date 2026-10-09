# ADR 0044: Experiments, remote config, acquisition source and server-driven onboarding

Status: accepted

## Context
Enzo wants to iterate fast with A/B tests, change UI and copy without an App Store review,
and give each acquisition source its own first minutes: a friend's invite should open the
chat with that friend, a Space link the Space, an ad its own intro. All of it has to fit
ADR 0043: aggregates only, no content, pseudonyms that expire, no fingerprinting.

## Decision
- **One remote config document** (`roda-proto/src/experiments.rs`, built-in default
  `remote-config.json`): flags (salt, rollout %, weighted variants, `experiment`,
  guardrails), copy defaults and per-arm overrides, onboarding flows (steps, landing, copy)
  and rules mapping source/campaign (optionally through an experiment's arms) to a flow.
  `GET /v1/config` serves the newest version (ETag, 5 min cache). `PUT /admin/config`
  (admin token) validates and stores a new version in `remote_config`; every node picks
  it up within 15 s. Devices cache the last good document and fall back to the built-in one.
- **Bucketing on the device, checked at the relay.** `bucket = SHA-256(salt ‖ 0 ‖ unit)[..4]
  mod 10 000`. The unit is 128 random bits made on first open, before any account exists,
  so onboarding is testable and the unit links to nothing. Variant weights split the
  bucket space; `rollout` uses its own salt; a holdout (5% by default, its own salt) gets
  control everywhere so the sum of shipped changes can be measured. The same Rust code
  runs in the app core and the relay, which recomputes the arm before accepting an exposure.
  Accounts on a second device may land in another arm until device linking (real/m1)
  carries the unit in the account bundle; onboarding happens once per install anyway.
- **Exposure, not assignment.** The app reports an arm only when it showed it (the
  onboarding plan was rendered, a flag was read). Analysis is over exposed units.
- **Acquisition source, first touch.** The core reads the link that opened the app:
  `zoen://friend/<handle>`, `https://tryzoen.com/@<handle>` (friend); `zoen://join/<code>`,
  `/j/<code>` (Space); `c=`/`utm_campaign=` (campaign, `[a-z0-9_-]{1,32}` or dropped);
  nothing (organic). The friend's handle or the code stays on the device to drive the
  landing; the relay gets only the kind and the campaign id, once, in a signed report, and
  keeps them on the 35-day account row.
- **No fingerprinting.** A link opened before install isn't recovered by matching IP or
  device traits. The onboarding's first screen offers a system `PasteButton` for an invite
  link, so the person chooses to bring it. Paid ads, when they come, are measured with
  AdAttributionKit/SKAdNetwork postbacks at the campaign level (aggregated by Apple, sent to
  the ad network), never joined to accounts. App Store campaign links (`ct=`) are read in
  App Store Connect, not in the app.
- **`POST /v1/report`**, signed by a registered device like a blob upload
  (`zoen-sync/2:client-report:<relay>:<sha256(body)>:<ts>`): `{unit, attribution?,
  exposures[], health?}`. Health (sessions, crashes since the last report) is sent only
  when the person opted in, and only as counts.
- **Server-driven onboarding router.** `growth_onboarding_plan()` returns the steps, copy
  and landing for this install's source and arm. Two real variants ship in
  `onboarding_friend_v1`: `control` (the 8-screen flow) and `direct` (hello with "@ana
  invited you", profile, done, then straight into the chat with the inviter). Space links
  get `space_link` (lands in the Space); the `ads_concurso` campaign gets `campaign_study`.
  The app skips step ids it doesn't know and never drops the profile step.
- **Analysis** (`/admin/metrics` → `experiments`, `acquisition`):
  - Per arm: units, messages per exposed day, the same CUPED-adjusted with the unit's
    messages in the 7 days before exposure, D1 retention, crash-free sessions.
  - Against control: difference and lift, a fixed-horizon p-value, and an **always-valid
    p-value** (mSPRT, normal mixture with τ² = (0.2·σ)²), so the dashboard can be read every
    day without inflating false positives.
  - Guardrails (`retention_d1`, `crash_free`, `send_latency`) mark an arm that hurts them,
    and a decision line reads "keep running", "significant" or "stop".
  - Send latency is measured at the relay for everyone, so it guards releases, not arms.
  - Funnel (signups, first message, 24 h activation) and D1/D7 retention by source and
    campaign, and by onboarding arm.
- **Retention limit.** Exposures and account rows live 35 days, so experiments are read
  within 4 weeks of exposure: fast iteration is the point, and longer effects show in the
  holdout's aggregate curves.

## Why not PostHog flags/experiments
PostHog would bucket and analyse on person profiles and per-user events, exactly what
ADR 0043 keeps off vendors. Evaluating our own small document on the device costs nothing
at scale (one cached GET per install per few minutes, served from memory) and works
offline. PostHog still draws the daily aggregates.

## Proof
`crates/zoen-cli/tests/journey_growth.rs` (real relay, Postgres, FoundationDB, `zoen`
installs) and the unit tests in `roda-proto` (link parsing, bucketing uniformity and
determinism, flow selection, validation) and `zoen-relay` (normal and mSPRT p-values,
CUPED recovering an effect under heavy pre-period noise).

## At 1B users
- The config is one small document per cell, served from memory behind the CDN.
- Exposure rows are one per unit per experiment, partitioned by first day; per-arm
  analysis becomes a daily job writing per-arm aggregates, like `metrics_daily`.
