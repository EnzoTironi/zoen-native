# ADR 0020: Abuse controls

Status: accepted (S6)

## Context
The relay had one guard: a token bucket inside each WebSocket session, 20 events/s with a
burst of 60. Reconnecting gave a fresh bucket, so a flood just reconnected. Nothing capped
handshakes (two signature checks each), account creation, handle lookups (directory
enumeration), invite previews (code guessing), typing signals, or blob bytes.

## Decision
- One mechanism, GCRA (`crates/zoen-relay/src/limits.rs`): per key, store only the
  theoretical arrival time. That is 8 bytes plus the key, refills continuously, charges
  weighted costs exactly (blob KiB), and gives an exact "retry in" on refusal. 64 shards per
  limiter; idle keys are swept when a shard grows past 50 000.
- Buckets live on the node and outlive sessions, so a reconnect doesn't refill them.

| scope | key | default |
|---|---|---|
| `connect_ip` | client address | 60/min, burst 60 |
| `register_ip` | client address | 10/h, burst 10 (new accounts only) |
| `publish_device` | device | 20/s, burst 60 |
| `publish_account` | identity, across its devices | 40/s, burst 120 |
| `ephemeral_device` | device | 10/s, burst 30 (excess is dropped silently) |
| `request_device` | device | 50/s, burst 200 (every request op) |
| `lookup_account` | identity | 60/min, burst 60 |
| `invite_account` | identity | 30/min, burst 30 |
| `blob_kib_device` | device | 100 MiB/min, burst 200 MiB |

- **Refusals:**
  - All of them use the same words: `slow down, retry in 1.2 s`.
  - Publishes get a transient `Rejected`, so the outbox keeps the event and retries.
  - Request ops get an error result.
  - Handshakes get `ErrorCode::RateLimited` before any signature work. The client retries no
    sooner than the hint (`roda_proto::retry_hint`).
  - Blobs get HTTP 429 with `Retry-After`.
  - Older clients decode the new code as `Other`.
- **Client fix in the same change:** a handshake refusal used to stop the client until the
  user poked it, even "database unavailable". Now `Unavailable` and `RateLimited` retry with
  backoff, and only `Unauthorized`, `UpgradeRequired` and `Other` wait for the user.
- **Client address:** `ZOEN_CLIENT_IP_HEADER` names the header the edge sets and clients can't
  forge (`fly-client-ip` on Fly, which overwrites it). Without it, the TCP peer address is
  used. Once the hostnames move behind the Cloudflare proxy, the header becomes
  `cf-connecting-ip`. The `*.fly.dev` origin stays reachable directly and could be spoofed
  there, so per-IP limits are a coarse first tier, never the only one.
- **Tuning:** `ZOEN_LIMITS="publish_device=20/s:60,register_ip=5/h:10"` overrides any scope
  without a rebuild. Unknown scopes or malformed quotas refuse to start, so a typo can't
  silently leave a default in place.
- **Metrics:** `zoen_relay_rate_limited_total{scope}`.

## Proof
`crates/zoen-cli/tests/journey_limits.rs`:
- A real client that sends faster than its bucket allows gets refused and still delivers
  all six messages in order.
- A raw flood client gets "slow down" with a usable hint.
- The fourth account from one address is refused, handle lookups get capped, and handshakes
  from one address end in `RateLimited`.
- Every refusal shows up in the metrics.

## At 1B users
- Node-local buckets multiply by the number of nodes one client can reach. The edge routes a
  device by consistent hash on its identity, so its sessions land on the same edge, and the
  local bucket is effectively global for the scopes keyed by device or identity.
- Account creation and other scarce, global actions get a second tier in FoundationDB:
  per-minute counters with atomic adds under `("rl", scope, key, minute)`. That is one write
  per registration, which is negligible at signup rates and exact across nodes.
- Per-IP tiers move to the edge (Cloudflare rate-limiting rules for `/v1/sync` handshakes)
  so floods die before they reach a node; the relay's own limits stay as defense in depth.
- Memory: about 100 bytes per active key. 1M online devices per edge across 4 device scopes
  is about 400 MB, so the sweep threshold is set per pod size.
