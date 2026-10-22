[Research index](agent-sandbox.md).

## 7. One billion users

Starting point: [cost-model.md](../cost-model.md) treats 1B users as 500M daily users (DAU).

**Escalation is the main cost lever** (Enzo, 2026-10-08): the cheapest tier is the default
and a heavier tier starts only when a tool's manifest declares it needs one. Assumptions:

| Assumption | Value |
|---|---|
| DAU who use an agent on a given day | 20% → 100M |
| WASM tool calls per agent user per day | 50 at 5 ms CPU |
| Agent users whose task escalates to a code sandbox | 10% → 10M, 3 active minutes each |
| Agent users whose task escalates to a browser | 5% → 5M, 5 active minutes each |
| Idle suspend after | 60 s; snapshot kept 24 h (50 MB code, 100 MB browser, compressed diff) |
| Peak to average concurrency | 2× |
| Spare for warm pools and failures | 20% |

Results:

| | Value |
|---|---|
| Code sandbox-hours per day | 500k (avg 20.8k concurrent, peak 41.7k) |
| Browser-hours per day | 417k (avg 17.4k concurrent, peak 34.7k) |
| Hosts, Hetzner AX102 (SMT on, core scheduling) | ≈ 1,320 → **≈ $398k/month** (SMT off: ≈ 2,080 → $629k) |
| Hosts, AWS m7i.metal-24xl on demand | ≈ 435 → **≈ $1.54M/month** (SMT off: ≈ 694 → $2.45M) |
| WASM tier | ≈ 5B calls/day → **≈ $2.7k/month** of CPU |
| Snapshot storage (R2-class at $0.015/GB-month, from cost-model.md) | ≈ $15k/month |
| Same hours bought from E2B + Browserbase | ≈ **$2.25M/month** |
| Per DAU per month (Hetzner / AWS / providers) | $0.0008 / $0.0031 / $0.0045 |
| Per agent user per month | $0.0040 / $0.015 / $0.022 |
| **Counterfactual without escalation** (every agent user gets a microVM for 10 min/day) | ≈ 16.7k AX102 hosts → **≈ $5.0M/month**, about 13× more |

These are capacity costs only; model calls are owner-paid (see cost-model.md) and are larger.
One provider cannot host 1,200 servers on demand in one region; at that point we buy across
several bare-metal providers and AWS metal, which is why the host layer is declared in
OpenTofu modules per provider.
