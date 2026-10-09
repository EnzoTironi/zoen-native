# ADR 0028: Agent sandboxes we run ourselves: WASM first, Firecracker microVMs on demand, browsers in microVMs

Status: accepted (M3 design; build in phases below)

Research, prices and every citation: [docs/research/agent-sandbox.md](../research/agent-sandbox.md).

## Context
Agents in `zoen-agentd` (Rust, Rig behind Zoen traits) need to run tools. Most tools are small
and safe to run as WASM components, which hooks already use ([ADR 0013](0013-hooks.md)). Some
need a real Linux: run code, install packages, convert files. Some need a real browser. That
code is written by a model, often prompted by text a stranger sent, so we treat it as hostile.

Enzo's calls (2026-10-08): build it ourselves instead of renting E2B or Modal; work out
browser use; and **heavy tiers start only when a task truly needs them**.

Constraints: 1B-user scale with a cost per user, the strongest privacy (no plaintext in logs,
mTLS, STRIDE), everything as code (OpenTofu, Kubernetes with GitOps, k3d locally), staging
under US$50/month on Fly.io (which has no nested KVM), and no copyleft code linked into what
we ship.

## Decision

### 1. Three tiers, cheapest by default, escalation only on declared need

| Tier | Runs | Isolation | Start | Used for |
|---|---|---|---|---|
| **T0: none / WASM** | in `zoen-agentd`, wasmtime with the pooling allocator | WASM capabilities, fuel, memory caps, Cedar grants | microseconds | hooks, data transforms, parsers, API calls through host functions, most tools |
| **T1: code microVM** | Firecracker microVM on a bare-metal sandbox node | own guest kernel, jailer (chroot, cgroups v2, seccomp, one unprivileged uid per VM, own netns) | <1 s from a warm snapshot | shell, Python/Node, package installs, file conversion, builds |
| **T2: browser microVM** | same as T1, from a browser template | same as T1 | <1 s VM, then Chromium launch | browsing, forms, logins with a human handoff |

**Escalation rules**
- Every tool ships a **capability manifest**:
  `needs = none | wasm | microvm { vcpu, mem_mib, disk_mib, egress, max_secs } | browser { egress, profile, max_secs }`.
  Undeclared means `none`.
- The router in `zoen-agentd` picks the **lowest tier that satisfies the manifest**. A model
  cannot ask for a heavier tier at runtime; only a tool's signed manifest can.
- Escalating to T1 or T2 needs a Grant for that tool (Cedar). Anything above the Grant (a new
  egress domain, a longer run, a saved browser profile) becomes an `AgentRequest` the owner
  approves in the app, like every other approval.
- **Lazy start:** no sandbox exists until the first call that needs one in that task.
- **Idle suspend:** after 60 s without a call, the VM is snapshotted (diff) to local NVMe and
  stops costing CPU and RAM; after 15 min the snapshot moves to object storage.
- **Teardown:** at the end of the task, or 24 h after the last use if the task asked to keep
  state. Snapshots are encrypted to the owner's agent key and deleted on teardown.
- Each owner has a per-day budget of sandbox-minutes and browser-minutes, enforced like model
  spend, from `UsageRecorded` events.

### 2. Firecracker, driven by our own node agent, not a pod per sandbox
- **Firecracker** (Apache-2.0) is the microVM. It is what AWS Lambda, Fly, E2B, Vercel and
  Daytona use, it has the smallest attack surface (≤5 MiB VMM overhead, ≤125 ms boot), and it
  has snapshots with lazy memory (userfaultfd).
- **`zoen-sandboxd`** (new Rust crate, one per bare-metal host, deployed as a privileged
  DaemonSet on a `kvm=true` node pool): runs `jailer` + `firecracker` per sandbox, serves
  memory pages from the template's memory file through a userfaultfd handler, gives each VM a
  copy-on-write root disk (device-mapper thin snapshots), a tap device with nftables rules,
  and a vsock channel to **`zoen-guestd`** inside the guest (exec, files, process streams).
  The guest kernel (GPL-2.0) runs only inside the VM on our servers; nothing GPL is linked
  into our binaries.
- **Why not Kata or `agent-sandbox` pods:** both are good and Kubernetes-native, but each
  sandbox becomes a pod: seconds to start, no snapshot restore of a pod, and the API server in
  the hot path at tens of thousands of starts a minute. Kubernetes runs our node agents and
  control plane; the sandboxes are not pods. Kata with Cloud Hypervisor stays the plan if we
  ever need pod semantics.
- **GPU later:** Firecracker has no GPU passthrough. When a tool needs a GPU, a T1g tier uses
  Cloud Hypervisor with VFIO (Apache-2.0/BSD) or gVisor with `nvproxy`, behind the same trait.

### 3. Snapshot templates and the restore-once rule
- A template is built from an OCI image: boot it, warm it (for T1: interpreters loaded; for
  T2: kernel, display server and `zoen-browserd` up, Chromium not started), snapshot it.
- Firecracker warns that resuming one snapshot more than once is unsafe if unique state is
  inside. So templates contain **no secrets, no tokens and no seeded user-space RNG**; the
  guest kernel reseeds via VMGenID on restore; every per-sandbox credential (vsock token,
  egress identity) is minted after restore; Chromium starts after restore with a fresh
  profile.
- **Fork** = pause, diff snapshot, restore twice; the same rule applies.

### 4. Sandbox interface in `zoen-agentd`

```rust
#[async_trait]
pub trait SandboxProvider: Send + Sync {
    async fn acquire(&self, spec: &SandboxSpec, owner: &OwnerRef) -> Result<Lease>;
    async fn exec(&self, lease: &Lease, req: ExecRequest) -> Result<ExecStream>;
    async fn put_file(&self, lease: &Lease, path: &str, bytes: Bytes) -> Result<()>;
    async fn get_file(&self, lease: &Lease, path: &str) -> Result<Bytes>;
    async fn browser(&self, lease: &Lease) -> Result<BrowserSession>; // T2 only
    async fn suspend(&self, lease: &Lease) -> Result<SnapshotRef>;
    async fn resume(&self, snap: &SnapshotRef) -> Result<Lease>;
    async fn fork(&self, lease: &Lease) -> Result<Lease>;
    async fn release(&self, lease: Lease, keep: Retain) -> Result<()>;
}
```

`SandboxSpec` is the manifest plus the template id. Backends:

| Backend | Where | Isolation |
|---|---|---|
| `FirecrackerNodes` | production, and any Linux with `/dev/kvm` (the box, k3d on a KVM host) | microVM |
| `FlyMachines` | staging on Fly (no nested KVM there) | one Fly Machine (Firecracker) per sandbox |
| `Microsandbox` | Enzo's Mac for local dev (libkrun on Apple's Hypervisor framework) | microVM; libkrunfw loaded as a separate library |
| `Gvisor` | CI and hosts without KVM | user-space kernel (`systrap`) |
| `Fake` | unit tests | none |

### 5. Scheduler and pools
- **`zoen-sbx-scheduler`** (control plane, stateless, several replicas): leases live in
  FoundationDB (`/sbx/lease/{id}`, `/sbx/node/{id}`), lifecycle events go on JetStream, nodes
  heartbeat their free memory, CPU and cached templates.
- **Placement:** a node that already has the template cached, then the most free memory, never
  above 90% committed RAM, with owner spread so one owner cannot fill a node.
- **Warm pools:** per node and template, a few pre-restored VMs sized from a moving average of
  demand; zero for rare templates.
- **Wake on use:** a call to a suspended lease restores it on any node holding the snapshot,
  then falls back to object storage.

### 6. Network and data rules
- **Default deny.** Each VM has its own tap and netns; nftables drops everything except the
  egress proxy. No route to the metadata service, cluster CIDRs, other sandboxes or the host.
- **Egress proxy** (per node): enforces the manifest's domain allowlist by SNI or Host, does DNS
  itself, rate-limits, and **swaps secrets in**: the sandbox holds a placeholder and the proxy
  inserts the real token only on requests to that token's allowed hosts, so code can use a
  credential but never read or exfiltrate it.
- **Logs** hold metadata only: lease id, host, bytes, duration, exit code. Never commands,
  file contents, page contents or screenshots.
- All control traffic (agentd ↔ scheduler ↔ sandboxd) is mTLS; agentd ↔ guest is vsock with
  a per-lease token.
- **Abuse:** per-owner CPU and egress budgets, outbound SMTP blocked, and kill on mining-like
  CPU profiles.

### 7. Browser use
- **T2 runs Chromium inside the microVM.** `zoen-browserd` in the guest speaks CDP to it
  locally (a Rust CDP client, e.g. chromiumoxide, MIT/Apache) and exposes tools to the agent
  over vsock: `open`, `read` (the accessibility tree with stable refs), `click(ref)`,
  `type(ref)`, `screenshot`, `download`. The CDP port never leaves the VM. We build it
  ourselves and use Steel (Apache-2.0) as a reference.
- **Watching live:** browserd uses CDP `Page.startScreencast` and encrypts each frame to the
  watching device's key (HPKE), so the proxy in the middle sees only ciphertext. The app shows
  it as a live card in the chat. WebRTC (for example neko) is the upgrade if frame rate is not
  enough.
- **Taking over:** when the agent hits a login, a 2FA prompt or a CAPTCHA, it raises an
  `AgentRequest` of kind *handoff*. The person taps, the agent pauses, and the person's taps
  and keys go straight from the device to browserd (`Input.dispatch*Event`). While the person
  is in control the model gets no screenshots or page text; it gets "handoff complete" when
  they hand back. Typed passwords never reach the model or the logs.
- **Sessions:** a saved browser profile (cookies, local storage) is an opt-in, per-site
  approval. It is stored sealed to the owner's agent key and opened only inside the VM.
- **Anti-bot:** we do not run stealth patches or CAPTCHA solvers. Our agents identify
  themselves with **Web Bot Auth** signatures; CAPTCHAs go to the human handoff. Residential
  proxies are a paid add-on and a separate decision.

### 8. Where it runs

| Environment | How | Cost |
|---|---|---|
| Local, Linux with KVM (the box) | k3d with a `kvm=true` node that mounts `/dev/kvm`; `zoen-sandboxd` DaemonSet; `FirecrackerNodes` | free |
| Local, Enzo's Mac | `Microsandbox` backend; the k3d cluster uses `Gvisor` | free |
| CI | `Gvisor` and `Fake`; a `FirecrackerNodes` journey only on runners with `/dev/kvm` | free |
| Staging (Fly) | `FlyMachines`: a separate `zoen-sbx-staging` app on its own custom private network, so sandboxes cannot reach the relay or databases; shared-cpu-1x 1 GB for T1, 2 GB for T2; suspend on idle; destroy at teardown; a hard cap of 4 concurrent machines | ≈ $22/month at 50 sandbox-hours plus 20 browser-hours a day ($0.0082 and $0.0154 per hour); stopped machines $0.15/GB-month |
| Production | bare metal declared in OpenTofu modules per provider (Hetzner AX, AWS `*.metal`, or AWS C7i/M7i/C8i with nested virtualization for small regions), joined to the cluster as the `kvm=true` pool | see below |

### 9. Cost (details and sources in the research note)

| | Ours on Hetzner AX102 (60% utilization) | Ours on AWS m7i.metal-24xl | Providers |
|---|---|---|---|
| Code sandbox-hour (1 vCPU, 1 GiB) | $0.0069 | $0.025 | E2B $0.067, Modal $0.095 |
| Browser-hour | $0.0115 | $0.042 | Steel $0.08–0.10, Cloudflare $0.09, Browserbase $0.10–0.12 |

At 1B users (500M DAU; 20% use an agent daily; 10% of those escalate to a code sandbox for
3 min, 5% to a browser for 5 min; 60 s idle suspend; 2× peak; 20% spare):
**≈ $361k/month on Hetzner-class metal ($0.0007 per DAU)**, ≈ $1.33M on AWS metal on demand,
≈ $2.25M if bought from E2B and Browserbase. If every agent task got a microVM for 10 minutes
instead (no escalation), it would be ≈ $5.0M/month. **Escalating only on need is worth about
14×.**

## Build plan

| Phase | Scope | Proof | Spend |
|---|---|---|---|
| **P0** | `SandboxProvider` trait, manifest and router in `zoen-agentd`, `Fake` and `Gvisor` backends, WASM tier wired to the same router, egress allowlist model, budgets | journey: a T0 tool runs in WASM; a tool declaring `microvm` without a Grant raises an `AgentRequest`; with a Grant it runs in gVisor | none |
| **P1** | `zoen-sandboxd` + `zoen-guestd`, template build from OCI, snapshot restore with userfaultfd, nftables + egress proxy, suspend, resume, fork; measure start time and density on the box | journey on the box's `/dev/kvm`: acquire under 1 s from a warm pool, suspend and resume keep files, the VM cannot reach the metadata IP or the relay | none |
| **P2** | browser template, `zoen-browserd`, live view with encrypted frames, handoff `AgentRequest`, SwiftUI live card and takeover | journey: agent fills a form; handoff for a login; the model's transcript has no password; measured MB per session | none |
| **P3** | `FlyMachines` backend and staging app on its own private network | the P0–P2 journeys pass against staging | about $22/month; needs Enzo's `fly auth login` and approval to create the app |
| **P4** | scheduler with FoundationDB leases, warm pools, OpenTofu module for the bare-metal pool, load test to 1,000 concurrent sandboxes | measured density and cost per sandbox-hour replace the estimates | one or two bare-metal hosts; needs approval |
| **P5** | GPU tier, more regions, Web Bot Auth registration | | later |

## Consequences
- We own a security boundary. This needs a STRIDE review before P3, fuzzing of the vsock and
  egress-proxy parsers, fast guest-kernel updates, and a public description of the isolation
  model.
- Agents get a real computer only when a tool says so, so the common case stays cheap and
  private.
- A provider (E2B and Browserbase have free tiers) can still be plugged in behind the trait
  for comparison, without changing app code.

## Open decisions for Enzo
1. **SMT off on sandbox hosts** (Firecracker's recommendation, stronger against side channels)
   costs about half the CPU capacity, which matters most for browsers. Proposed: off.
2. **Residential proxies and CAPTCHA services** for sites that block datacenter IPs: proposed
   not in v1 (human handoff plus Web Bot Auth instead).
3. **Staging spend** of about $22/month on Fly Machines (P3), and **one or two bare-metal
   hosts** for P4.
