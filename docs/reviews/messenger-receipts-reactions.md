# Messenger receipts and reactions

This front hardens the existing shared-core receipts and annotation folding. It does not introduce a second messaging protocol or change group membership policy.

## Contracts

- Receipt watermarks only advance. An older or duplicate receipt must not lower delivered/read state.
- Delivery and reading are different: another device delivering a message does not mean its owner read it. An own-device read clears the local unread state.
- Locally persisted read state survives disconnects. Ephemeral receipt frames must be replayable after reconnect even when no new message arrives.
- Group delivery/read aggregates the current roster. Historical ticks when membership changes remain governed by that existing policy; this change does not freeze a per-message roster.
- Each person has at most one reaction per message. Replacement/removal affects that person's reaction, not other participants. Participants must survive the web DTO boundary.
- Deleted messages do not accept new reactions. The core bounds reaction payloads to 32 UTF-8 bytes, including compound emoji.
- Native reaction changes use the existing surfaced-error path. Web read failures are reported and remain retryable rather than closing the account or being discarded.

## Controls

Android, iOS/macOS, and web retain quick reactions and add a picker that accepts keyboard emoji beyond the shortcuts. Reaction details show participants and allow removal of the current person's reaction. Android receipt metadata retains its accessibility semantics.

## Validation boundaries

Core tests exercise receipt/reaction state and replay deterministically; they are not proof of production transport or cross-platform UI interoperability. Web controller tests cover DTO participant preservation and surfaced retryable read failures. The Android `ReactionJourneyTest` exercises real controls with the demo core and captures participant details in CI; it is not a two-account network test.

Local simulators/emulators are intentionally not run for this front. Android CI owns native build/instrumentation coverage. Swift parsing alone is not an iOS/macOS app build. Any UI fixture screenshot is labeled as a component fixture, not as end-to-end evidence.
