# Zoen relay: scale review (2026-10-15)

Scope: `crates/zoen-relay` and the wire path of the Rust core, measured against layer 1 (the
messenger) in `REPORT.md`. Builds on ADR 0022 (measured capacity) and ADR 0019/0023 (multi-node,
per-Space sequencing). Measured on the shared 8-vCPU box with load average 9–16 from other
workers, so every number is a **lower bound for throughput and an upper bound for latency**;
only paired base/new runs taken back to back are compared.

## 1. Where it stands (honest summary)

The skeleton is right for 1B: stateless edge relays, logs in FoundationDB keyed per Space, an
interest-routed NATS bus between nodes, content-free pushes, aggregated receipt watermarks, GCRA
limits per device/account/IP with bounded memory, 37 KiB per socket, Prometheus counters and
OTel trace propagation across the bus. What it is **not** yet is cheap per message on the paths
added for layer 1 (receipts, push), and several things the 1B plan assumes exist only in ADRs.

## 2. Findings by area

| Area | Finding | Severity | Status |
|---|---|---|---|
| Receipts fan-out | Every Delivered/Read receipt was broadcast to every member: O(G²) frames per message. A 32-person group at 250 msgs/s collapsed to **5–16 accepted msgs/s** | 💥 | **Fixed (PR below)**: Spaces > 16 route a receipt to the authors it acknowledges + my devices |
| Receipts storage | One Postgres upsert per receipt (≈ one per delivered copy: 3.5M writes/s at the 1B model) | 🔴 | **Fixed**: folded in memory (max per person per Space), one `UNNEST` upsert per 100 ms in chunks of 2,000; replay merges pending |
| Ephemeral path | Two FDB transactions per typing/receipt (`role` then `members`) | 🟠 | **Fixed**: one read answers both |
| Ephemeral path | Still one FDB range read of the member list per typing/receipt | 🟠 | Open → owner-side membership cache (ADR 0022 "S9") |
| Push wake | Per published message: a spawned task, one NATS ping per offline-candidate member, one Postgres query. The `relay/device-presence` branch makes that query unconditional per message | 🔴 | Open, **not touched** (owned by the device-presence branch). Fix: push-target cache + per-device wake debounce (one wake per device per N s) |
| Sync | `replay_receipts` is one Postgres query per Space per sync (a 500-chat account = 500 queries per reconnect; a reconnect storm multiplies it) | 🟠 | Open → one `WHERE space = ANY($1)` per sync page |
| Hub | One global `RwLock<HashMap>`: every connect/disconnect takes the write lock that every delivery reads | 🟡 | Open; shard it after device-presence merges (it adds `Hub::online_devices`) |
| NATS | One subscription per online identity per node (160M subjects cluster-wide at peak) and one sequential publisher task per node, rebuilding headers per recipient | 🟡 | Open; ADR 0019 Space subjects above ~256 members; batch publish |
| Retention | **No log retention or tiering exists in code.** The "30 days hot, then object storage" in ADR 0022 is a plan. Storage per message is unbounded in time | 🔴 | Open; needs a segment-to-object-store job (and an object store) |
| Postgres directory | Single database for identities, devices, key packages, push tokens, receipts. Indexes are right for the access paths (PKs + `devices_by_identity`, `push_tokens_token`, `key_packages_by_device`) | 🟠 at 1B | Needs sharding by identity (Citus or app-level) — infrastructure we don't have |
| Backpressure | Session outbox 1,024 frames; a full outbox kicks the socket so it resyncs (correct). Bus outbound 65,536 with drop counter | ✅ | — |
| Rate limits | GCRA per device/account/IP, sharded, swept, overridable by env; refused counters exported | ✅ | — |
| Observability | Counters for sessions, sequenced/duplicate/rejected, append retries/batches, bus, limits; now receipt flushes. OTel spans with W3C context across NATS | 🟡 | Missing latency **histograms** (append, sync, push), Postgres pool saturation, FDB latency |

## 3. What the PR changes, with numbers

`zoen-load --receipts` (new): every recipient answers each message with a Delivered receipt,
like the app. Paired runs, release build, 20 s measured:

| scenario | base (main) | this PR |
|---|---|---|
| 2,048 people, groups of 32, 250 msgs/s, receipts | **7–16 accepted msgs/s**, p50 20–21 s (collapsed) | **73–103 accepted msgs/s** (≈10×), relay still the limit (membership read per receipt) |
| 2,000 people, groups of 8, 250 msgs/s, receipts (≤16: still broadcast) | p50 139–161 ms, p99 1.6–2.6 s, 122–126 msgs/relay-core-s | p50 31–54 ms, p99 0.29–0.82 s, 132–142 msgs/relay-core-s |
| 2,000 people, groups of 8, 1,000 msgs/s, no receipts | 1,000/s, 523 msgs/relay-core-s | 1,000/s, 606 msgs/relay-core-s (no regression) |
| groups of 32, 250 msgs/s, no receipts (reference) | 250/s, 7,750 deliveries/s, p50 5 ms, 1.13 relay cores | — |

Postgres receipt writes: from one statement per receipt to at most one statement per 2,000
folded watermarks per 100 ms per node.

Honest gap: with receipts on, large groups still don't hold 250 msgs/s on this box; the next
cost is the FDB member-list read on every ephemeral frame. That's S9 (owner-side cached
membership), not a tweak.

## 4. Capacity math

Units (ADR 0022, confirmed today within noise): **37 KiB per socket; 0.58 ms relay CPU per
message + 45 µs per delivered copy; ~2,500 appends per FDB core (all roles colocated);
~3,500 handshakes/s per relay**.

- **Connections per node.** A 32 GB / 8 vCPU relay: memory allows ~800k sockets, but handshake
  rate is the real ceiling — after a node dies its sockets reconnect elsewhere at 3,500/s/node,
  so plan **≤150k sockets per node** (a full node's reconnect absorbed by 10 peers in ~4 s).
  160M concurrent devices → **~1,100 relay nodes**.
- **Messages/s.** 1B model peak: 700k msgs/s, 3.5M deliveries/s → 700k × 0.58 ms + 3.5M × 45 µs
  ≈ **564 relay cores** (+50% headroom ≈ 850). Receipts: with author routing, ≈ one frame per
  delivery to the author, adding ~3.5M small frames/s ≈ +160 cores at 45 µs. Before this PR, a
  64-person average group would have been ~G× that.
- **Log store.** 700k appends/s ÷ 2,500 = 280 FDB cores, ×3 replication ≈ 840 cores.
- **Storage per user.** ~20 messages sent/day × 1 KB ≈ 20 KB/day → **600 KB hot per user for 30
  days, 1.8 MB with 3 replicas** → 1.8 PB hot fleet-wide. **Without retention (today): 7.3 MB per
  user per year, ×3, forever.** Receipt watermarks: 2 bigints per (person, Space) ≈ 60 B/row
  with overhead → ~200 Spaces/user ≈ 12 KB per user in Postgres. Push tokens ~250 B/device.
- **Postgres.** 1B identities + ~2B devices + key packages (≈100 per device ⇒ ~200B rows if left
  at that depth) is far past one primary; key-package depth and directory sharding are required.

## 5. Path to 1B (what we can do vs. what needs infrastructure)

Doable in code now, on the box:
1. Owner-side membership cache + batched appends (S9) — removes the per-ephemeral FDB read and
   most append round trips.
2. Push: cached targets + per-device wake debounce (after device-presence merges).
3. Batched `replay_receipts` per sync page; sharded Hub; batched NATS publish.
4. Latency histograms (append, sync, push) and pool-saturation gauges.
5. Log retention: segment old log ranges out of FDB to an object store (code + local MinIO tests).

Needs infrastructure we don't have (and no paid cloud without approval):
- A real multi-process FDB cluster (separate log/storage/stateless processes) to replace the
  "2,500 appends/core" colocated number.
- Load generators on separate machines (today the generator shares the 8 cores).
- Sharded Postgres (Citus / per-identity shards) and an object store for log segments and media.
- APNs/FCM accounts for real push throughput (Apple Developer + Firebase).
- TLS termination at the edge and multi-region cells (the code has cells; nothing deploys >1).
