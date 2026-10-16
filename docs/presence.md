# Presence and last-seen privacy

Presence is relay-authenticated, scoped to co-members, and separate from the event log.
Online means at least one live session of the identity exists (including sibling
devices and the existing cross-node fanout lookup). Closing one device must not
announce the account offline while another remains online.

## Additive wire fields

- `ClientFrame` oneof tag 8: `SetPresencePrivacy`, with boolean `hide_last_seen` tag 1.
- `Presence`: identity tag 1 and online tag 2 are unchanged. Optional uint64
  `last_seen_ms` is tag 3. Optional bool `hide_last_seen` is tag 4 and is sent only
  to the account's own devices. Optional uint64 `revision` tag 5 is relay delivery
  metadata; legacy clients may ignore it.
- An absent last-seen value means online, hidden, or unknown; it is never a client
  clock-derived estimate. Legacy presence frames decode with both optional fields
  absent. Legacy clients can continue consuming the original two fields.

Migration `0032_presence_privacy.sql` persists account policy and the relay's
last observed final-device disconnect time. The relay removes the timestamp for
hidden accounts before constructing peer frames, both live and on reconnect.
Policy/live publication uses account-scoped PostgreSQL advisory locks. Final
WebSocket delivery rechecks the durable revision and policy, dropping delayed
cross-node frames and retaining a shared row fence through the actual write.
This keeps a queued old public timestamp or self preference from overtaking a
completed privacy change without modifying the NATS scaling implementation.
Cancellation closes lock-bearing connections; server-side lock waits are bounded.
Database errors fail closed for private fields but preserve timestamp-free offline
signals. Newly shared memberships receive a presence snapshot without reconnecting.
A process crash is not a final-disconnect observation:
there is no claim that the stored timestamp precisely records an unobserved crash.

After Ready, every registered device receives its own authoritative setting.
Changes are echoed to all sibling devices, without exposing the setting to peers.
Clients never replay a stale stored preference on login. Native clients retain
only an explicit unacknowledged user change for retry. Hiding last seen does not
hide online status or disable typing/read signals.

Typing and status signals are not stored or replayed. Clients keep separate
per-Space, per-person deadlines; a stop or expiry removes only that sender.
Typing refreshes while composing, expires after six seconds without a refresh,
and clears on loss of connection. Native transient command queues are scoped to
one authenticated connection and a six-second age bound, so old typing/status
commands cannot replay after reconnect. Receipt queuing is unchanged. This bounds stale indicators after dropped stop
frames. Native and web interfaces use Portuguese copy.

## Checks

- `cargo test -p roda-proto --lib`: additive frame roundtrips and legacy presence.
- `cargo test -p zoen-relay --lib`: peer privacy frame policy.
- `ZOEN_TEST_PG=... cargo test -p zoen-relay --test presence`: isolated PostgreSQL,
  real authenticated sockets, sibling aggregation, privacy echoes, hidden live
  disconnect/reconnect snapshots, public last-seen after opting back in, delayed
  policy/transition publication, cancellation cleanup, SQL failure fallback, stale
  remote-frame rejection and final-delivery fencing.
- `cargo test -p roda-ffi presence_privacy_retries_only_explicit_unacknowledged_changes`.
- `cd packages/zoen-ui && npm run typecheck:web && npm run test:web`.
- `swiftc apple/Shared/Sync/LiveActivityState.swift apple/Tests/LiveActivityJourney.swift -o /tmp/zoen-live-activity && /tmp/zoen-live-activity`.

Native app builds and journeys require regenerated UniFFI bindings. Do not edit
those generated outputs by hand. No simulator or emulator is needed for the pure
activity-state tests.
