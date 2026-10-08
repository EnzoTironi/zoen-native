# ADR 0004: Offline first with a durable outbox

Status: accepted (milestone 1)

## Decision
Writes never wait for the network. A write to a synced Space is signed, stored in the
`outbox` table and projected immediately. The core's net task (a small tokio runtime inside
`roda-ffi`) flushes the outbox after every login and whenever it's poked. The relay's answer
is `Accepted` (then the sequenced copy arrives like anyone else's) or `Rejected` with
`permanent`: transient refusals (rate limit, database hiccup) retry; permanent ones mark the
message "Not delivered" and keep it visible.

Catch-up uses per-Space cursors (`next_seq`). The relay pages history (500 per page) and
ends with `SyncDone`. Out-of-order arrivals trigger a targeted sync for that Space.

## Consequences
- Kill the app mid-send: the outbox row is still there on relaunch, and `client_id`
  idempotency makes the resend harmless.
- The UI shows `Sending…` only for messages that really haven't reached the relay.

## At 1B users
The outbox lives on the device, so it costs the server nothing until a device reconnects.
What matters is the reconnect storm: retries are idempotent by `client_id`, so a storm
only costs duplicate-detection point reads, and jittered backoff (0.5 s to 30 s) spreads
160M reconnects over the backoff window instead of one instant.
