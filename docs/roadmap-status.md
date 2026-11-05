# Roadmap status: the complete Zoen product

Current execution, 5 November 2026: **finish iOS first**, using React Native/Expo,
the shared Rust core and the real backend. The active sequence and release gates
are maintained in the [complete-product goal](dev/zoen-complete-product-goal.md#execução-atual--20-de-outubro-de-2026)
and [acceptance manifest](../apps/zoen/acceptance.json). Web/Android follow the iOS
release gate; desktop follows later. Telegram iOS guides UI and interactions;
Signal guides reliability; Notion/ChatGPT Space guide collaborative creation.
Enzo confirmed the active priority: **agents and conversation only**, finishing
the selected Telegram iOS screens/interactions and their backend before Apps.
App/creation development and QA, editor integration, host/SDK expansion and
anchored comments are paused. Code, data, evidence and final scope remain intact.

[PR #865](https://github.com/EnzoTironi/zoen-native/pull/865) consolidates
30 accumulated PR heads into one integration batch, preserving their commits.
It includes personal chat folders/default inbox, contextual search and hashtags,
file albums, native spoiler handling, media replacement/playback recovery,
notification account routing, invite/call/session fixes and shared agent
capture/financial-receipt recovery. The implementation details and remaining
limits are recorded in issues #172, #175, #296 and #327. The older Rust-quality
draft #342 remains separate and unfinished.

Issue #170 records the published main commit and merge status. Source presence,
automated checks, manual acceptance and public release remain distinct. The
506-capability matrix and 30-journey checklist retain their individual gaps;
this batch does not complete Telegram parity or productive agent execution.

## Current validation policy

Enzo will perform manual product acceptance after the roadmap implementation.
During implementation, use automated checks in coherent batches. Keep those
results separate from physical iPhone acceptance and public release evidence.

GitHub Actions currently cannot start jobs because of account billing. This is
an infrastructure failure, not a code-test result. Main has no required checks;
normal merges use recorded local automated evidence without changing branch
rules or using an admin override. The combined client-core library suite passed
all 1,401 tests. Exact app/native build results are recorded on PR #865 rather
than reusing an old installed artifact as evidence for newer source.

## Historical checkpoints

[Dated implementation and validation records](history/roadmap-before-integration-2026-11-05.md)
retain their original scope and do not define current readiness.
