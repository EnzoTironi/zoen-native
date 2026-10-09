# ADR 0022: Measured capacity, and what a billion people would cost

Status: accepted (S8)

## Context
Every "At 1B users" section so far rested on guesses: 150k connections per edge node at about
30 KB each, some number of messages per core. The plan said the load generator would replace
them with measurements. This ADR is those measurements, the defects they exposed, and the cost
model rebuilt on them.

## Method
`zoen-load` (crate `crates/zoen-load`) drives simulated people over the real protocol:
WebSocket, Hello/Challenge/Auth, Register, group Spaces built with `SpaceCreated` and
`MemberAdded`, then signed `Publish` frames. `scripts/bench-load.sh sweep` runs each scenario
against release relays on a fresh Postgres database and a fresh FoundationDB cell.

- **Open loop, no coordinated omission.** A scheduler fixes when each message is due
  (`--rate` per second across everyone, sender picked at random). The due time travels in the
  message, so ack and delivery latency are measured from when the message *should* have left,
  not from when a backed-up sender got round to it. A slow relay shows up as latency, never as
  a politely slower client.
- **Exact accounting.** Every delivery is counted against the deliveries the plan expects
  (group size minus one per measured message); a scenario passes only if they match.
- **Cost from the kernel.** CPU seconds and resident memory of the relay, FoundationDB and
  NATS come from `/proc` over the measured window; memory per connection is the relay's growth
  across the connect phase divided by the people connected.
- **One machine.** The box: 8 vCPU Intel Xeon, 16 GB shared with other workloads. The load
  generator, relays, FoundationDB (one `fdbserver` process holding all eleven roles, `ssd-2`
  engine) and NATS share it. No TLS (production terminates TLS at the proxy). Every simulated
  person shares one address, so only the per-address limits are lifted; all other limits stay.

Raw JSON per scenario, the relay logs and the sweep summary are in the S8 proof directory.

## Results (sweep of 2026-10-08, release build)
30 s measured after 5 s warm-up; latency is from the due time; "relay cores" is CPU seconds per
wall second.

| scenario | people | group | offered msgs/s | result | delivery p50 / p99 | relay cores | FDB cores |
|---|---|---|---|---|---|---|---|
| rate-250 | 2,000 | 8 | 250 | 52,500 / 52,500 deliveries | 3.9 / 8.2 ms | 0.43 | 0.27 |
| rate-500 | 2,000 | 8 | 500 | 105,000 / 105,000 | 4.0 / 10.9 ms | 0.77 | 0.49 |
| rate-1000 | 2,000 | 8 | 1,000 | 210,000 / 210,000 | 3.7 / 14.4 ms | 1.18 | 0.68 |
| rate-2000 | 2,000 | 8 | 2,000 | 419,986 / 419,986 | 5.6 / 26.9 ms | 1.79 | 0.85 |
| rate-4000 | 2,000 | 8 | 4,000 | saturated at 2,490 accepted/s | queueing (seconds) | 2.59 | **1.00** |
| fanout-2 | 2,048 | 2 | 500 | 15,000 / 15,000 | 3.3 / 13.4 ms | 0.55 | 0.45 |
| fanout-32 | 2,048 | 32 | 500 | 465,000 / 465,000 (15,500/s) | 5.2 / 20.7 ms | 0.99 | 0.42 |
| fanout-128 | 2,048 | 128 | 500 | 63,500 deliveries/s needed: passed once (p99 191 ms), saturated in the sweep | — | 1.82 | 0.69 |
| conns-10k | 10,000 | 8 | 200 | 42,000 / 42,000 | 4.4 / 160.8 ms | 0.32 | 0.25 |
| two-nodes-1000 | 2,000 over 2 relays + NATS | 8 | 1,000 | 209,993 / 209,993 | 7.9 / 54.3 ms | 0.54 + 0.54 | 0.59 |

Per connection and per handshake:
- **Relay memory: 37 KiB per WebSocket** (33–38 KiB across scenarios; 21 KiB per node when two
  nodes split the people, since fixed costs amortize). Before this ADR it was 160 KiB.
- **Handshakes: about 3,500 per second per relay** (TCP, WebSocket upgrade, signed challenge,
  Register), p99 125–175 ms with 256 in flight.
- **NATS** carried the cross-node half of 7,000 deliveries/s on 0.12 cores.

### What limits a node
- **The log store, as deployed here.** At the knee FoundationDB's single process reaches a full
  core while the relay uses 2.6 of 8 cores. Sampled per thread at 2,000 msgs/s, no relay thread
  is hot: the FDB client network thread uses 0.39 of a core, each tokio worker about 0.14. One `fdbserver` with every role colocated is a
  development topology; production runs logs, storage and stateless roles as separate
  processes (the fdb-operator manifests), each with its own core. The honest unit from this
  box is about **2,500 appends per second per FoundationDB core** with all roles together.
- **Relay CPU, by a linear model.** Fitting the sustained high-rate points (rate-2000 at seven
  deliveries per message, fanout-32 at thirty-one): **0.58 ms of relay CPU per message plus
  45 µs per delivered copy.** At group 8 that is about 1,100 messages, or 7,800 deliveries, per
  relay core-second; fanning out to 32 it is 15,700 deliveries per core-second.
- **Large groups on this box.** fanout-128 at 500 msgs/s asks for 63,500 deliveries a second:
  about 4–5 relay cores by the model, while the load generator on the same eight cores reads
  every one of those frames. It sustained the full rate on a quieter run (1,905,000 /
  1,905,000, p50 4.8 ms, p99 191 ms) and fell behind in the sweep. That's this machine, not a
  limit of the design, but it's why Space subjects (ADR 0019) take over fan-out above ~256
  members, and it needs measuring on separate machines.

## Defects the measurements found, fixed here
1. **A latency floor of one inter-arrival gap in the first harness.** The scheduler sent message
   *k* only once `k + 1` were due, so p50 equalled 1/rate (56 ms at 20/s, 205 ms at 5/s). Now
   message *k* leaves at *k*/rate and the loop sleeps until the next due time. Real p50 at
   light load is 4–5 ms (GRV 1.6 ms + commit 2.1 ms + two WebSocket hops).
2. **Nagle on every socket.** Nothing set `TCP_NODELAY`; small frames could wait on a delayed
   ACK. The relay listener, the app (`roda-ffi`) and `zoen-load` now disable it.
3. **128 KiB + 128 KiB of buffers per WebSocket.** tungstenite's defaults reserved 256 KiB per
   connection, so the relay held 160 KiB per person: 26 TB of RAM for 160M devices. The relay
   now starts each socket with 8 KiB read and write buffers (they still grow for a large sync
   page): **37 KiB per connection, 4.3× less**.
4. **Quadratic memory in the load generator.** Grouping with `split_off` left every group
   holding the capacity of the whole remaining list: 3.7 GB at 10,000 people, which the kernel
   killed (taking another process on the shared box with it). Groups are now exact-size; the
   tool reports its own memory per phase (116 MiB for 3,000 people).
5. **Load setup gave up on a rate limit.** It now resends the same envelope after the relay's
   temporary refusal, as the app does; the relay deduplicates by client id.
6. **Contention was invisible.** `zoen_relay_append_retries_total` now counts append
   transactions FoundationDB made run again: the signal for a hot Space.

## Decision
- Ship the six fixes above. Socket buffers start at 8 KiB (`SOCKET_BUFFER`), every socket
  disables Nagle, and the load generator and its sweep are the capacity record: any change to
  the hot path reruns `scripts/bench-load.sh sweep` and updates this table.
- The capacity units for planning are the measured ones: **37 KiB per connection, 0.58 ms per
  message + 45 µs per delivery of relay CPU, 2,500 appends per FoundationDB core** (all roles
  colocated, so conservative).
- Next lever, before more nodes: **owner-side sequencing (S9).** The append path costs 0.58 ms
  of relay CPU and half a dozen FoundationDB operations per message. The Space owner (ADR 0018)
  should serialize appends per Space in memory, keep membership cached, and commit a burst of
  envelopes in one transaction. That removes read round trips, makes conflicts impossible by
  construction instead of retried, and is what keeps a hot Space from tipping into queueing.

## Consequences
- The plan's guess of about 30 KB per connection holds, but only after fix 3; at the old
  160 KiB the edge fleet would have been four times larger.
- Numbers come from one shared 8-vCPU machine with the load generator on it, so they are lower
  bounds for relay throughput and upper bounds for latency. The next measurement puts the load
  generator on its own machines (staging, under the cost gate) and FoundationDB in separate
  processes.
- Not yet measured: TLS cost (terminated by the proxy today), MLS-sized ciphertexts (envelopes
  here are signed plaintext of about 300 bytes; the model assumes 1 KB), push, media, and
  catch-up storms after an outage.

## At 1B users
The plan's load model: 700k messages/s and 3.5M device deliveries/s at peak, 160M WebSockets.
Prices are Fly.io list prices (2026-10): a dedicated vCPU with 2 GB is $33/month, extra RAM
$6/GB/month, volumes $0.15/GB/month, egress from South America $0.04/GB.

| part | from the measurements | size | list price per month |
|---|---|---|---|
| relay CPU | 700k × 0.58 ms + 3.5M × 45 µs = 564 cores, +50% headroom | 850 vCPU (1.7 TB RAM included) | $28k |
| connection memory | 160M × 37 KiB = 6.1 TB, less the 1.7 TB included | 4.4 TB extra RAM | $26k |
| FoundationDB compute | 700k ÷ 2,500 per core, ×3 for triple replication | 840 vCPU | $28k |
| FoundationDB storage | 600 TB hot (30 days at 1 KB), ×3 replicas | 1.8 PB of volumes | $270k |
| delivery egress | 100B deliveries/day × 1 KB | 3 PB | $120k |
| **total** | | | **≈ $470k, about $0.0005 per monthly user** |

That is a quarter of the plan's earlier $2M estimate, and the shape matters more than the
total: compute is under a fifth of it. Storage and egress dominate, which is why the log keeps
30 days hot (older segments go to object storage at a fraction of volume prices) and why S9
(fewer operations per message) and Space subjects (fewer copies per delivery inside the
cluster) come before buying cores. On owned NVMe hardware the storage line drops by roughly an
order of magnitude. Media (73 PB/year behind a CDN) is outside this table, as in the plan.
