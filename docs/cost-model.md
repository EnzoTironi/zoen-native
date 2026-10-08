# Cost per user

Infrastructure cost per daily user per month, before agent model calls (owner-paid) and
before payment fees. Assumptions come from the scale model in [plan-real.md](plan-real.md);
per-node rates are estimates until `zoen-load` (S8) measures them, and this file gets updated
with the measured numbers.

## Assumptions per DAU per day

40 messages sent, 5 device deliveries each, 1 KB per envelope, 2 photos of 200 KB, 0.33
connected devices at peak per DAU, 20 pushes after collapsing.

## Unit prices (on-demand cloud list prices, rounded, São Paulo carries about a 40% premium)

| resource | price |
|---|---|
| 8 vCPU, 32 GB general node | $300/month |
| 8 vCPU, 64 GB with local NVMe (FDB) | $650/month |
| object storage | $0.015/GB-month (R2-class, no egress fee) |
| CDN egress | $0.01/GB at volume |
| managed Postgres | $0.20/GB-month all-in |

## Per-node capacity (estimates, replaced by S8 measurements)

| service | per node |
|---|---|
| edge | 150k connections |
| sync | 3,500 appends/s per 8-core node |
| FDB | 7,000 appends/s per NVMe node at triple replication |
| NATS | 300k deliveries/s per node |
| push | 3,500 pushes/s per node |

## Totals

| | 1M DAU | 100M DAU | 1B DAU-equivalent (500M DAU) |
|---|---|---|---|
| messages/s peak | 1.4k | 140k | 700k |
| edge nodes | 3 (minimum for availability) | 220 | 1,100 |
| sync nodes | 3 | 40 | 200 |
| FDB nodes | 5 (minimum cluster) | 60 | 300 |
| NATS nodes | 3 | 15 | 60 |
| push nodes | 2 | 25 | 100 |
| compute $/month | $6k | $150k | $750k |
| hot log storage (30 days, 3x) | 3.6 TB | 360 TB | 1.8 PB (inside FDB node cost) |
| media stored after a year | 146 TB | 14.6 PB | 73 PB |
| media $/month at year end | $2k | $220k | $1.1M |
| CDN egress (each photo viewed 3x) | $0.4k | $40k | $180k |
| Postgres | $1k | $20k | $60k |
| total $/month | about $10k | about $430k | about $2.1M |
| per DAU per month | $0.010 | $0.0043 | $0.0042 |

Small scale is dominated by minimum cluster sizes; large scale by media. The levers, in
order: media retention tiers and resizing, connection density per edge node, FDB write
amplification (batching in T2/T3 Spaces).

Agents are metered per call (tokens and cost) and charged to the owner's budget, so they
don't appear here; see the ledger design in plan-real.md.
