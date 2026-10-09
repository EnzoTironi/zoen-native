# Research: where Zoen agents run code and drive browsers

Date: 2026-10-08. Sources were read on that date; prices change, so every number links to the
page it came from. The decision lives in [ADR 0028](../adr/0028-agent-sandbox.md).

The question from Enzo: what is the best sandbox for agent work, built by us rather than rented,
and what do we do about browser use?

## 1. What we need from a sandbox

- **Isolation that survives hostile code.** Agents run code that a model wrote, often from text
  a stranger sent. Assume the code is malicious.
- **Cheap when idle, fast when needed.** Most agent turns never need a sandbox. When one does,
  it should be ready in under a second and cost nothing while the user is not using it.
- **Snapshot, suspend and fork.** Keep a task's state between turns without paying for a
  running machine, and branch a sandbox to try two things.
- **Runs on Kubernetes-managed hosts and is declared in code** (OpenTofu plus GitOps), locally
  on k3d and in staging under US$50 a month.
- **No copyleft code linked into what we ship.** Running GPL software as a separate process or
  inside a guest VM on our own servers is fine (it is "mere aggregation", and running a
  service is not distribution). Linking it into the app or into `zoen-agentd` is not.

## 2. Isolation technologies

| | What it is | Isolation | Start | Memory overhead | Snapshot / fork | GPU | Kubernetes fit | License | Maturity |
|---|---|---|---|---|---|---|---|---|---|
| **WASM (wasmtime)** | WebAssembly runtime, already used for hooks ([ADR 0013](../adr/0013-hooks.md)) | language-level sandbox, capabilities only | ~5 µs instantiation with copy-on-write heaps [W1] | KBs to MBs | not needed (stateless) | no | runs in-process | Apache-2.0 [G] | very mature |
| **Firecracker** | minimal KVM VMM in Rust, used by AWS Lambda | separate guest kernel, jailer (chroot, cgroups, seccomp, unprivileged uid) [F3] | ≤125 ms from InstanceStart to guest init; API ready within 8 CPU ms [F1] | ≤5 MiB per VMM [F1] | full and diff snapshots; lazy memory via userfaultfd [F2]; restore-once caveat (below) | no GPU passthrough yet: PCIe landed in 1.13 without VFIO; VFIO work is early [F5] | not a pod runtime by itself; Kata or our own node agent | Apache-2.0 [G] | very mature (Lambda, Fargate, Fly, E2B, Vercel, Daytona) |
| **firecracker-containerd** | containerd plugin that runs containers inside Firecracker | as Firecracker | as Firecracker | as Firecracker | limited | no | containerd, not k8s-native | Apache-2.0 [G] | low activity |
| **Cloud Hypervisor** | fuller Rust VMM (rust-vmm family) | separate kernel | sub-second | small | snapshot/restore, not across versions [CH1] | VFIO passthrough, hotplug [CH1]; VFIO snapshot only for devices with migration v2 [CH2] | Kata backend (`kata-clh`, virtio-fs) | Apache-2.0 and BSD-3-Clause | mature |
| **Kata Containers** | OCI runtime that runs each pod in a microVM | VM per pod | pod start (seconds with k8s) | VM + kata-agent | no snapshot-restore of pods | via CH/QEMU | native (`RuntimeClass`); Firecracker needs the devmapper snapshotter [K1] | Apache-2.0 [G] | mature |
| **gVisor (runsc)** | user-space kernel intercepting syscalls | strong, but a shared host kernel behind the Sentry | container speed | low | checkpoint/restore exists | yes, `nvproxy` [GV1] | native `RuntimeClass` | Apache-2.0 [G] | mature (Modal's default [M1]) |
| **libkrun / krunvm / microsandbox** | library VMM (KVM on Linux, HVF on macOS) | separate kernel | fast | small | no general snapshot | no | not a k8s runtime | libkrun Apache-2.0; libkrunfw bundles a GPL-2.0 kernel and is LGPL-2.1, fine to load as a separate library [LK1]; microsandbox Apache-2.0 [MS1] | young |
| **Unikraft** | unikernels; Unikraft Cloud sells <10 ms cold starts [U1] | VM | very fast | very small | yes on their cloud | full VMs only [U1] | no | BSD-3-Clause | young; apps must be ported |

Notes:
- **gVisor needs no KVM.** Its default `systrap` platform uses seccomp traps and runs inside a
  VM or on hosts without virtualization; the KVM platform is faster on bare metal [GV2]. That
  makes it the fallback wherever `/dev/kvm` is missing.
- **Snapshot clones are a security hazard.** Firecracker says resuming the same snapshot more
  than once is insecure unless unique state stays unique (RNG seeds, tokens) [F2]. It ships
  VMGenID, which makes Linux 5.18+ reseed its kernel RNG on restore, but user-space caches are
  not covered [F4]. Rule for us: template snapshots hold no secrets and no started user-space
  RNG consumers; every per-sandbox secret is minted after restore.
- **SMT.** Firecracker's production guide recommends disabling SMT on hosts because it enables
  cross-tenant speculation side channels [F3]. That halves the thread count per host. Linux
  core scheduling (`PR_SCHED_CORE`, kernel 5.14+) is the middle path: tasks with different
  cookies never share a core at the same time. The kernel docs say it mitigates some, not all,
  cross-HT attacks: kernel contexts on siblings (IRQ, syscall, VMEXIT) are not protected, MDS
  between user and kernel mode and L1TF guest attacks remain, and it can cost throughput
  through forced idle, so measure [LX1]. Zoen uses it (ADR 0028, section 6b).
- **Kubernetes sandbox orchestration exists upstream.** `kubernetes-sigs/agent-sandbox`
  (Apache-2.0) adds `Sandbox`, `SandboxTemplate`, `SandboxClaim` and `SandboxWarmPool` over
  gVisor or Kata pods [AS1]. It is the k8s-native path, but a pod per sandbox inherits pod
  start latency and API-server load.

## 3. How the providers build theirs

| Provider | Isolation | Notable pieces | Source |
|---|---|---|---|
| **E2B** | Firecracker microVM per sandbox, own cgroup and network namespace | A sandbox *is* a resumed snapshot; memory loaded lazily by a userfaultfd handler from the template memfile, with a prefetcher; root fs is a copy-on-write NBD device; pause exports dirty blocks as a diff; per-node Go orchestrator as a Nomad job; `envd` agent inside the VM; paused sandboxes wake on traffic | [E1] Apache-2.0 [G] |
| **Daytona** | Firecracker microVMs, resumed from memory snapshots | Runners pull jobs (no inbound connections), egress policy enforced on the host, secrets swapped in by an outbound proxy so code can use but not read them; went closed source in June 2026 for security reasons | [D1] [D2] [D3] |
| **Vercel Sandbox** | Firecracker microVM per sandbox | own kernel, filesystem and network namespace | [V1] |
| **Cloudflare Sandbox** | container inside a VM per instance, managed by a Durable Object | Workers call it; lifecycle and routing by the Durable Object | [CF1] |
| **Modal** | gVisor by default; VM sandboxes in beta; GPU sandboxes only on gVisor | bill on max(request, usage) | [M1] |
| **Fly Machines** | Firecracker | suspend and resume from a memory snapshot; per-second billing; no nested KVM inside a Machine | [FL1] [FL2] [FL3] |

The pattern is clear: everyone serious about hostile code uses a **microVM per sandbox,
created by restoring a pre-booted snapshot, with lazy memory**, and enforces network policy on
the host. Kubernetes is used to run the node agents, not to schedule each sandbox as a pod.

## 4. Where KVM is available

| Host | KVM for our own VMs | Source |
|---|---|---|
| AWS `*.metal` | yes, bare metal | [A1] |
| AWS virtual instances | since Feb 2026 on C8i/M8i/R8i, since Jun 2026 also C7i/M7i/R7i and others; Intel only; AWS still recommends metal for latency-sensitive work | [A2] [A3] [A4] |
| Google Compute Engine | nested virtualization on Intel VMs (Haswell+), not ARM, AMD only on N4D | [GC1] |
| Azure | nested virtualization on Dv5 and others | [AZ1] |
| Hetzner dedicated (AX) | yes, bare metal | [H1] |
| Fly.io Machines | **no** nested virtualization | [FL3] |
| The box (this dev machine) | `/dev/kvm` exists; the `box` user is not in its group yet | measured |

## 5. Browser use

### Architectures
- **Browserbase, Steel, Hyperbrowser, Browser Use Cloud** all run Chromium in a remote
  sandbox and hand the agent a CDP endpoint (Playwright, Puppeteer or raw CDP), plus a live
  view URL. Steel's runtime is open source, Apache-2.0 [ST1]; it bundles Chrome, a CDP API on
  9223 and a viewer UI [ST2]. Browser Use (MIT) is the agent library [G].
- **Live view and takeover.** Browser Use returns a live URL a person can open to take over the
  same session, and warns that anyone holding the live or CDP URL controls the browser [BU1].
  Cloudflare has explicit `handoff` and `handoffComplete` calls for human-in-the-loop [CF2].
- **Streaming options.** CDP `Page.startScreencast` streams JPEG/PNG frames with acks and
  needs nothing else in the VM [CDP1]; input goes back through `Input.dispatch*Event`. WebRTC
  (for example neko, Apache-2.0 [G]) gives smoother video and audio but needs a desktop, a
  WebRTC stack and TURN. noVNC is MPL-2.0, which is file-level copyleft; fine unmodified, but
  we do not need it.
- **Memory.** Plan 250–500 MB per typical headless Chrome, 400–700 MB for heavy SPAs, more
  than 1 GB with video or WebGL; measure with PSS or cgroups, not summed RSS [BR1]. This is a
  secondary source; we will replace it with our own measurement in phase 2.
- **Anti-bot.** Datacenter IPs get challenged. Providers sell residential proxies by the GB
  (Browserbase $10–12/GB [BB1], Steel $6–10/GB [ST3], Hyperbrowser $10/GB [HB1]) and CAPTCHA
  solving. The honest alternative is to sign our agent's requests with **Web Bot Auth**
  (HTTP message signatures), which Cloudflare recognizes as "signed agents" [CF3].

### Provider prices (per browser-hour)

| Provider | Price | Source |
|---|---|---|
| Browserbase | $0.12 (Developer, $20/mo with 100 h), $0.10 (Startup, $99/mo with 500 h); 1-minute minimum per session | [BB1] |
| Steel cloud | $0.10 (Launch), $0.08 (Scale, $250/mo) | [ST3] |
| Cloudflare Browser Run | $0.09 beyond included hours | [CF4] |
| Hyperbrowser | $0.10 | [HB1] |

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

## Sources

- [G] GitHub repository metadata (license, activity) read on 2026-10-08 for firecracker-microvm/firecracker, firecracker-microvm/firecracker-containerd, kata-containers/kata-containers, google/gvisor, bytecodealliance/wasmtime, superradcompany/microsandbox, e2b-dev/infra, steel-dev/steel-browser, browser-use/browser-use, m1k1o/neko, kubernetes-sigs/agent-sandbox, mattsse/chromiumoxide (all Apache-2.0 or MIT).
- [W1] Wasmtime 1.0 performance: https://bytecodealliance.org/articles/wasmtime-10-performance ; pooling allocator: https://docs.wasmtime.dev/examples-fast-instantiation.html
- [F1] Firecracker specification: https://github.com/firecracker-microvm/firecracker/blob/main/SPECIFICATION.md
- [F2] Firecracker snapshot support: https://github.com/firecracker-microvm/firecracker/blob/main/docs/snapshotting/snapshot-support.md
- [F3] Firecracker production host setup (jailer, seccomp, disable SMT): https://github.com/firecracker-microvm/firecracker/blob/main/docs/prod-host-setup.md
- [AP1] App Review Guidelines 5.4 (VPN apps) and 2.4.2: https://developer.apple.com/app-store/review/guidelines/
- [LX1] Linux core scheduling: https://www.kernel.org/doc/html/latest/admin-guide/hw-vuln/core-scheduling.html
- [F4] Random for clones / VMGenID: https://github.com/firecracker-microvm/firecracker/blob/main/docs/snapshotting/random-for-clones.md
- [F5] PCIe in Firecracker 1.13: https://github.com/firecracker-microvm/firecracker/issues/5133 ; VFIO: https://github.com/firecracker-microvm/firecracker/pull/5870 , https://github.com/firecracker-microvm/firecracker/issues/5679
- [CH1] Cloud Hypervisor README: https://github.com/cloud-hypervisor/cloud-hypervisor ; snapshot: https://github.com/cloud-hypervisor/cloud-hypervisor/blob/main/docs/snapshot_restore.md
- [CH2] VFIO migration v2: https://github.com/cloud-hypervisor/cloud-hypervisor/pull/8303
- [K1] Kata with Firecracker: https://github.com/kata-containers/kata-containers/blob/main/docs/how-to/how-to-use-kata-containers-with-firecracker.md
- [GV1] gVisor GPU: https://gvisor.dev/docs/user_guide/gpu/
- [GV2] gVisor platforms: https://gvisor.dev/docs/architecture_guide/platforms/ , https://gvisor.dev/docs/user_guide/platforms/
- [LK1] libkrunfw licensing: https://github.com/libkrun/libkrunfw/blob/main/README.md ; libkrun: https://github.com/libkrun/libkrun
- [MS1] microsandbox: https://github.com/superradcompany/microsandbox
- [U1] Unikraft Cloud: https://unikraft.com/ , https://unikraft.com/docs/features/checkpoints
- [AS1] Kubernetes agent-sandbox: https://github.com/kubernetes-sigs/agent-sandbox
- [E1] E2B architecture: https://github.com/e2b-dev/infra/blob/main/docs/ARCHITECTURE.md
- [E2] E2B pricing: https://e2b.dev/docs/faq/calculate-sandbox-price , https://e2b.dev/pricing
- [D1] Daytona architecture: https://www.daytona.io/docs/en/architecture/
- [D2] Daytona isolation: https://www.daytona.io/docs/en/isolation/
- [D3] Daytona going closed source: https://www.daytona.io/dotfiles/updates/daytona-is-going-closed-source
- [V1] Vercel Sandbox concepts: https://vercel.com/docs/sandbox/concepts
- [CF1] Cloudflare Sandbox architecture: https://developers.cloudflare.com/sandbox/sdk/concepts/architecture/
- [CF2] Cloudflare Browser Run human in the loop: https://developers.cloudflare.com/browser-run/features/human-in-the-loop/
- [CF3] Cloudflare signed agents / Web Bot Auth: https://blog.cloudflare.com/signed-agents/ , https://developers.cloudflare.com/bots/reference/bot-verification/web-bot-auth/
- [CF4] Cloudflare Browser Run pricing: https://developers.cloudflare.com/browser-run/pricing/
- [M1] Modal sandbox resources (gVisor default, GPU only on gVisor): https://modal.com/docs/guide/sandbox-resources.md
- [M2] Modal pricing: https://modal.com/products/sandboxes , https://modal.com/pricing
- [FL1] Fly Machines API (suspend): https://docs.fly.io/machines/api/machines-resource
- [FL2] Fly suspend and resume: https://fly.io/docs/reference/suspend-resume/
- [FL3] Fly community, nested virtualization: https://community.fly.io/t/nested-virtualization-on-fly-io/11778
- [FL4] Fly pricing: https://fly.io/docs/about/pricing/ , https://fly.io/pricing
- [FL5] Fly billing (stopped and suspended Machines): https://docs.fly.io/about/billing
- [FL6] Fly custom private networks: https://fly.io/docs/networking/custom-private-networks/
- [A1] EC2 nested virtualization docs: https://docs.aws.amazon.com/AWSEC2/latest/UserGuide/amazon-ec2-nested-virtualization.html
- [A2] AWS announcement, Feb 2026: https://aws.amazon.com/about-aws/whats-new/2026/02/amazon-ec2-nested-virtualization-on-virtual/
- [A3] AWS announcement, Jun 2026: https://aws.amazon.com/about-aws/whats-new/2026/06/nested-virtualization-intel-us-gov-cloud/
- [A4] InfoQ summary: https://www.infoq.com/news/2026/03/aws-ec2-nested-virtualization/
- [A5] m7i.metal-24xl price: https://cloud-bench.com/instance/aws-m7i-metal-24xl
- [A6] c7i.metal-24xl price: https://aws-pricing.com/c7i.metal-24xl.html
- [GC1] GCE nested virtualization: https://docs.cloud.google.com/compute/docs/instances/nested-virtualization/overview
- [AZ1] Azure Dv5 series: https://learn.microsoft.com/en-us/azure/virtual-machines/sizes/general-purpose/dv5-series
- [H1] Hetzner price adjustment, 15 Jun 2026: https://docs.hetzner.com/general/infrastructure-and-availability/price-adjustment/
- [H2] Hetzner AX102 / AX162: https://www.hetzner.com/dedicated-rootserver/ax102/ , https://www.hetzner.com/dedicated-rootserver/ax162
- [ST1] steel-browser: https://github.com/steel-dev/steel-browser
- [ST2] Steel self-hosting with Docker: https://docs.steel.dev/overview/self-hosting/docker
- [ST3] Steel pricing: https://docs.steel.dev/overview/pricinglimits
- [BB1] Browserbase plans: https://docs.browserbase.com/account/billing/plans
- [HB1] Hyperbrowser pricing: https://www.hyperbrowser.ai/docs/pricing
- [BU1] Browser Use live preview and human in the loop: https://docs.browser-use.com/cloud/browser/live-preview , https://docs.browser-use.com/cloud/agent/human-in-the-loop
- [CDP1] CDP Page.startScreencast: https://chromedevtools.github.io/devtools-protocol/tot/Page/#method-startScreencast
- [BR1] Chrome memory per instance (secondary): https://www.pandastack.ai/blog/browser-automation-concurrency-limits/ , https://server.express/blog/how-many-chrome-instances-fit-on-a-9950x/
