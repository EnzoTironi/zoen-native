[Research index](agent-sandbox.md).

## 6. Prices for the cost model

### Sandbox providers

| Provider | Rate | 1 vCPU + 1 GiB for 1 hour | Source |
|---|---|---|---|
| E2B | $0.0504 per vCPU-h + $0.0162 per GiB-h | **$0.067** | [E2] |
| Modal sandboxes | $0.1419 per physical core-h (2 vCPU) + $0.0240 per GiB-h | **$0.095** | [M2] |
| Fly Machine shared-cpu-1x, 1 GB | $0.0082 per hour; stopped or suspended: $0.15 per GB-month of rootfs | **$0.0082** (shared CPU) | [FL4] [FL5] |

### Hardware we would run ourselves

| Host | Spec | Price | Source |
|---|---|---|---|
| Hetzner AX102-1 | Ryzen 9 7950X3D, 16 cores / 32 threads, 128 GB DDR5 ECC | €257.30 ($302.10)/month + €129 setup, from 15 Jun 2026 | [H1] [H2] |
| Hetzner AX162-1 | EPYC 9454P, 48 cores / 96 threads, 128–512 GB | €612.30 ($722.10)/month | [H1] [H2] |
| AWS m7i.metal-24xl | 96 vCPU, 384 GiB | $4.8384/h on demand, us-east-1 (≈ $3,532/month) | [A5] |
| AWS c7i.metal-24xl | 96 vCPU, 192 GiB | $4.284/h on demand, us-east-1 | [A6] |

### Our cost per sandbox-hour and browser-hour

Assumptions (ours, to be replaced by measurements in phases 1 and 2):
- **Code sandbox:** 1 vCPU, 1 GiB. With lazy memory most sandboxes never touch all of it, but
  we size on the allocation. 10% of host RAM reserved; vCPU oversubscribed 4:1 (agent code
  is mostly waiting on the model). AX102 ≈ 100 sandboxes, m7i.metal-24xl ≈ 320.
- **Browser sandbox:** 2 GiB allocated, about 1.5 GiB effective, vCPU 2:1. AX102 ≈ 60,
  m7i.metal-24xl ≈ 190.
- **Utilization:** fleets sized for peak run at about 60% on average.
- SMT on with core scheduling, assumed at 80% of full SMT throughput until P1 measures it;
  VMs get vCPUs in pairs. SMT off halves the threads.
- Excludes egress, engineers, and residential proxies (not used in v1).

| | At 100% packing | At 60% utilization | SMT off, 60% | Provider equivalent |
|---|---|---|---|---|
| Code sandbox-hour, Hetzner AX102 (100 per host, memory-bound) | $0.0041 | **$0.0069** | $0.0108 | E2B $0.067, Modal $0.095 (≈10×) |
| Code sandbox-hour, AWS m7i.metal-24xl (307 per host) | $0.0158 | **$0.026** | $0.042 | |
| Browser-hour, Hetzner AX102 (51 per host) | $0.0081 | **$0.0135** | $0.0216 | Steel $0.08–0.10, Cloudflare $0.09, Browserbase $0.10–0.12 (≈6–9×) |
| Browser-hour, AWS m7i.metal-24xl (153 per host) | $0.0316 | **$0.053** | $0.084 | |
| WASM tool call (5 ms CPU) | ≈ $0.00000002 | | | |

Fly shared-cpu-1x 1 GB at $0.0082/h is close to our Hetzner number, which is why Fly Machines
are the right staging backend (section 7) but not the production one: Fly gives a shared vCPU,
no fork, and suspend only up to 2 GB of RAM [FL2].
