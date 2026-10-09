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
  copy-on-write root disk (device-mapper thin snapshots), no network interface (see §6), and a
  vsock channel to **`zoen-guestd`** inside the guest (exec, files, process streams, egress).
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

### 6. Network: default deny, and the egress proxy as a first-class component
- **Default deny, by construction (changed in P1).** A VM has **no network interface**, only
  loopback. Tools find the proxy at `127.0.0.1:3128` in the guest (`HTTP(S)_PROXY` are set);
  `zoen-guestd` carries each connection over vsock to `zoen-sandboxd`, which hands it to
  `zoen-egress` already bound to the lease, so the guest holds no proxy credential. There is
  no tap, no netns and no nftables to get wrong, and no route to the metadata service,
  cluster CIDRs, other sandboxes or the host. Cost: raw TCP and UDP don't work, which v1
  doesn't need (a tool needing them would get a manifest-scoped tap later). WASM tools (T0)
  have no sockets at all; their HTTP goes through the same proxy via a host function.
- **`zoen-egress`** is its own crate and process (one per sandbox node, one per agentd pod for
  T0), not a feature of something else. It:
  1. **Allows only what the tool's manifest lists.** The manifest declares `egress` as hosts
     (`api.github.com`, `*.pypi.org`) with ports and methods. The proxy checks the CONNECT
     host or the Host header, resolves DNS itself, and refuses IP literals and private,
     loopback, link-local and metadata addresses even if a listed name resolves to them.
  2. **Injects credentials.** The sandbox and the model only ever see a placeholder such as
     `zoen-secret://github`. The proxy swaps it for the real token, which it gets sealed from
     the owner's vault for this lease, only on requests to the hosts that secret is bound to.
     A placeholder sent anywhere else is refused, so code can use a credential but can never
     read it or send it elsewhere. **HTTPS:** a tool with secrets gets a per-lease CA, made in
     the proxy and name-constrained to the hosts its secrets are bound to; only its
     certificate enters the VM's trust bundle. CONNECTs to those hosts are intercepted
     (HTTP/1.1, one request per connection), the placeholder swapped, and the request sent on
     over verified TLS; every other CONNECT is an opaque tunnel. Absolute-form `https://`
     requests (busybox `wget`) are opened by the proxy itself.
  3. **Turns an unlisted request into an approval card.** A request to a host outside the
     list is refused with `EGRESS_NEEDS_APPROVAL` and an id, and the proxy raises an
     `AgentRequest` (kind `egress`, tool, host, port, method, never the path or body). The
     owner chooses *once*, *for this task* or *always for this tool* (a new Grant); the
     tool retries after approval. Nothing is held open while waiting.
  4. **Logs metadata only:** lease, tool, host, port, method, decision, bytes in and out,
     duration. Never URLs past the host, headers, bodies, secrets or page contents.
  5. **Rate-limits** per lease and per owner, blocks SMTP and raw TCP not in the manifest.
- All control traffic (agentd ↔ scheduler ↔ sandboxd ↔ egress) is mTLS; agentd ↔ guest is
  vsock with a per-lease token.
- **Abuse:** per-owner CPU and egress budgets, and kill on mining-like CPU profiles.

### 6c. "Route through my device" (optional exit, designed in P0, built later)
Purpose: sites that block or CAPTCHA datacenter IPs see the person's own home or phone
connection instead, without paid residential proxies.
- **Off by default.** The person turns it on per device in Settings, and can turn it off any
  time. The app shows the number of requests routed through that device today and this week,
  by site (host only).
- **Only your own agents.** The exit accepts a tunnel only for leases whose owner is the
  device's identity: `zoen-egress` asks for a tunnel ticket signed by the owner's identity key
  (ADR 0003) naming the lease, tool and expiry, and the device checks it. No other person's
  agent, no community agent and no Zoen service can use it.
- **End-to-end.** `zoen-egress` keeps doing the allowlist, approval cards and credential
  injection, then sends each permitted connection over a tunnel encrypted end to end to the
  device (Noise or HPKE with the device key; the relay forwards ciphertext only). The device
  opens the outbound connection itself and **checks the destination again against the
  allowlist carried in the signed ticket**, refusing private, loopback and LAN addresses, so
  the person's home network is never reachable. TLS stays end to end between the browser in
  the VM and the site; the device sees host names and byte counts, not content.
- **Platforms.**
  - macOS: a background login-item helper (SMAppService) holds the tunnel while the Mac is
    awake.
  - iOS: in-app while Zoen is in the foreground, which covers the common case of a person
    watching or taking over a browser. A Network Extension (`NEPacketTunnelProvider`) for
    background use is an option to evaluate, not a plan: App Review guideline 5.4 treats VPN
    services strictly (organization account, `NEVPNManager`, data disclosures), and 2.4.2
    limits unrelated background work.
- **Caps.** Wi-Fi only by default (cellular is an extra toggle); not on Low Power Mode or
  below 20% battery unless charging; a per-day byte cap (default 200 MB) and requests-per-
  minute cap per device; video and downloads above a size limit stay on our egress.
- **Abuse limits.** The same egress budgets apply as on our own exit, plus a lower
  per-host rate, no SMTP, no ports other than 80 and 443, and an automatic off switch if a
  site starts refusing the person's IP. Every routed request is in the owner's metadata log.
- **Fallback.** If the device is offline, asleep, over a cap or refuses, traffic goes through
  our egress as usual, and the tool is told which exit was used.

### 6b. SMT stays on, with core scheduling
Enzo's call (2026-10-08): keep SMT (hyper-threading) on and use Linux **core scheduling**
instead of disabling SMT.
- `zoen-sandboxd` gives every sandbox a unique core-scheduling cookie
  (`prctl(PR_SCHED_CORE, PR_SCHED_CORE_CREATE, pid, PR_SCHED_CORE_SCOPE_THREAD_GROUP)`) on the
  jailer process before it execs Firecracker. Cookies are inherited across clone and exec, so
  every vCPU and VMM thread of that VM carries it, and the kernel never runs two different
  cookies on the two threads of one physical core at the same time. Host kernel 5.14+ with
  `CONFIG_SCHED_CORE`.
- Sandboxes get vCPUs **in pairs** (2 by default) so a VM can fill both threads of a core;
  a 1-vCPU VM would leave its sibling forced idle.
- **Residual risk** (from the kernel's own documentation): core scheduling does not protect
  kernel contexts on sibling threads from each other (IRQ, syscalls, VMEXIT); it cannot stop
  MDS between a sibling in user mode and one in kernel mode, nor an L1TF guest attacker on
  affected CPUs; and there is a short window while siblings receive the IPI to switch. So
  hosts also run the full CPU mitigations (`mitigations=auto`, plus L1D flush on VM entry
  where the CPU needs it), we buy CPUs not listed as affected by L1TF and MDS in
  `/sys/devices/system/cpu/vulnerabilities`, and `zoen-sandboxd` refuses to start on a host
  whose kernel lacks core scheduling.
- **Fallback:** a `smt=off` node pool (`nosmt` on the kernel command line) for high-risk
  tenants: organizations that ask for it, and owners flagged by abuse signals. The scheduler
  places them there by label.
- Cost: core scheduling has overhead (forced idle), and the kernel docs say to measure. We
  assume 80% of full SMT throughput until P1 measures it.

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
- **Anti-bot (decided for v1):** no residential proxies, no CAPTCHA-solving services and no
  stealth patches. Our agents identify themselves with **Web Bot Auth** signatures (signed
  agents), and CAPTCHAs and blocks go to the human takeover.

**As built in P2 (2026-10-09).** Where it differs from the plan above:
- **No separate `zoen-browserd`, no CDP library.** `zoen-guestd` starts Chromium (Alpine's
  package, headless) with `--remote-debugging-pipe` and speaks raw CDP over that pipe: no
  port exists, not even inside the VM. Chromium starts once per template, so a lease gets a
  browser that is already running. Chromium's own sandbox is off (`--no-sandbox`); the
  microVM is the sandbox.
- **The model's tools are four:** `browser_open`, `browser_read` (page text plus numbered
  links, buttons and fields), `browser_click` and `browser_type` (by number or CSS selector).
  No screenshot, no script evaluation, no shell for the model. `read` shows password and
  one-time-code fields as `[hidden]`; `type` refuses them (`FIELD_NEEDS_OWNER`).
  `screenshot` and `download` wait for a need.
- **Network:** Chromium's only proxy is the in-VM address to the egress proxy. A browser lease
  signs its requests (Web Bot Auth, below), so its HTTPS to allowlisted hosts is intercepted.
  Chromium can't take a new CA once it runs, and the template exists before any lease, so each
  node has a **sandbox root** baked into browser templates, and a browser lease's CA is a
  name-constrained intermediate under it. Only the node's egress proxy holds the root key,
  and a VM's traffic reaches no other proxy.
- **Web Bot Auth:** RFC 9421 with Ed25519 over `@authority` and `signature-agent`,
  `tag="web-bot-auth"`, the RFC 7638 thumbprint as `keyid`, 5-minute validity, a random
  nonce. The signed key directory (`/.well-known/http-message-signatures-directory`) is built
  by the same module; hosting it and registering with Cloudflare stay in P5. `Signature*`
  headers the page sends are dropped.
- **Live view:** frames are sealed in the VM to the owner device's X25519 key (a fresh VM key
  per session, HKDF-SHA256, ChaCha20-Poly1305; the HPKE base-mode construction without the
  HPKE wrapper) and streamed to the host on vsock port 1081; the host relays ciphertext.
- **Takeover:** the owner starts it (needs a live view). From then on the VM refuses the
  model's browser calls and also exec and file access (`TAKEOVER_IN_PROGRESS`). The device's
  input is sealed the other way, with sequence numbers: replays and anything the host makes
  up are refused, and only the device's sealed `Done` ends it. Text the owner typed stays in
  the VM's memory only, to scrub it from anything the model reads later (a site that echoes a
  password back shows `[hidden]`).
- **Not yet:** the handoff `AgentRequest` and the SwiftUI live card (the app side), saved
  profiles, WebRTC.

### 8. Where it runs

| Environment | How | Cost |
|---|---|---|
| Local, Linux with KVM (the box) | k3d with a `kvm=true` node that mounts `/dev/kvm`; `zoen-sandboxd` DaemonSet; `FirecrackerNodes` | free |
| Local, Enzo's Mac | `Microsandbox` backend; the k3d cluster uses `Gvisor` | free |
| CI | `Gvisor` and `Fake`; a `FirecrackerNodes` journey only on runners with `/dev/kvm` | free |
| Staging (Fly) | `FlyMachines`: a separate `zoen-sbx-staging` app on its own custom private network, so sandboxes cannot reach the relay or databases; shared-cpu-1x 1 GB for T1, 2 GB for T2; suspend on idle; destroy at teardown; a hard cap of 4 concurrent machines | ≈ $22/month at 50 sandbox-hours plus 20 browser-hours a day ($0.0082 and $0.0154 per hour); stopped machines $0.15/GB-month |
| Production | bare metal declared in OpenTofu modules per provider (Hetzner AX, AWS `*.metal`, or AWS C7i/M7i/C8i with nested virtualization for small regions), joined to the cluster as the `kvm=true` pool | see below |

### 9. Cost (details and sources in the research note)
With SMT on and core scheduling (80% of SMT throughput assumed until measured):

| | Ours on Hetzner AX102 (60% utilization) | Ours on AWS m7i.metal-24xl | Providers |
|---|---|---|---|
| Code sandbox-hour (2 vCPU shared, 1 GiB) | $0.0069 (memory-bound, unchanged) | $0.026 | E2B $0.067, Modal $0.095 (1 vCPU + 1 GiB) |
| Browser-hour | $0.0135 (SMT off: $0.0216) | $0.053 (SMT off: $0.084) | Steel $0.08–0.10, Cloudflare $0.09, Browserbase $0.10–0.12 |

At 1B users (500M DAU; 20% use an agent daily; 10% of those escalate to a code sandbox for
3 min, 5% to a browser for 5 min; 60 s idle suspend; 2× peak; 20% spare):
**≈ $398k/month on Hetzner-class metal ($0.0008 per DAU)**, ≈ $1.54M on AWS metal on demand,
≈ $2.25M if bought from E2B and Browserbase. With SMT off everywhere it would be ≈ $629k
(Hetzner) and ≈ $2.45M (AWS), so core scheduling saves about a third. If every agent task got
a microVM for 10 minutes (no escalation), it would be ≈ $5.0M/month. **Escalating only on
need is worth about 13×.**

## Build plan

| Phase | Scope | Proof | Spend |
|---|---|---|---|
| **P0** | `SandboxProvider` trait, manifest and router in `zoen-agentd`, `Fake` and `Gvisor` backends, budgets, `zoen-egress` skeleton (allowlist, credential injection, approval on unlisted hosts, metadata-only log) | journey: a T0 tool runs in WASM; a tool declaring `microvm` without a Grant raises an `AgentRequest`; with a Grant it runs in gVisor | none |
| **P1** | `zoen-sandboxd` + `zoen-guestd`, template build from OCI, snapshot restore with userfaultfd, nftables + egress proxy, suspend, resume, fork; measure start time and density on the box | journey on the box's `/dev/kvm`: acquire under 1 s from a warm pool, suspend and resume keep files, the VM cannot reach the metadata IP or the relay | none |
| P1 as built (2026-10-08) | jailer + cgroup limits, template snapshot per shape, warm pool, suspend/resume (restore once), vsock egress instead of tap + nftables, per-lease CA; still to do: OCI template builder, userfaultfd, diff snapshots, per-lease disk instead of a tmpfs `/work`, fork, density | journeys pass on the box and in CI (GitHub runners have `/dev/kvm`); numbers in the research note §4 | none |
| **P2** | browser template, `zoen-browserd`, live view with encrypted frames, handoff `AgentRequest`, SwiftUI live card and takeover | journey: agent fills a form; handoff for a login; the model's transcript has no password; measured MB per session | none |
| P2 as built (2026-10-09) | browser image (Alpine Chromium), Chromium in the template over a CDP pipe, the four model tools, egress with Web Bot Auth and the sandbox root, live view sealed to the device, takeover with sealed input; not yet: handoff card and SwiftUI live card | `journey_browser` passes on the box and in CI: the agent shops through the proxy as a signed agent, can't type the password, Ana signs in from her "phone", the model never sees what she typed; numbers in the research note §5 | none |
| **P3** | `FlyMachines` backend and staging app on its own private network | the P0–P2 journeys pass against staging | about $22/month; asked for when P3 starts; needs Enzo's `fly auth login` |
| P3 as built (2026-10-09) | `sandbox::fly::FlyMachinesProvider`: one Fly Machine (Firecracker) per lease, created on acquire and destroyed on release; exec and files go through the Machines exec API; suspend/resume map to machine suspend/start. App `zoen-staging-sandbox` on its own private network (`--network zoen-sandbox`), so guests can't reach relay/pg/fdb over 6PN; agentd uses a deploy token scoped to that app. IaC: `infra/fly/deploy.sh sandbox` (not part of `all`). Egress allowlist on Fly not enforced yet (moves to P4) | `journey_fly` (skips unless `ZOEN_FLY_SANDBOX_APP` + token are set; `ZOEN_REQUIRE_FLY=1` forces) | on demand: shared-cpu-1x 256 MB only while a lease is open, idle $0; well under US$5/month at staging volume. **App not created yet: waiting for Enzo's go-ahead** |
| **P4** | scheduler with FoundationDB leases, warm pools, core-scheduling cookies, OpenTofu module for the bare-metal pool, load test to 1,000 concurrent sandboxes | measured density, core-scheduling overhead and cost per sandbox-hour replace the estimates | one or two bare-metal hosts; asked for when P4 starts |
| **P5** | GPU tier, more regions, Web Bot Auth registration, "route through my device" exit (section 6c: macOS helper first, then iOS in-app) | journey: an allowlisted request leaves through the owner's Mac; another owner's ticket is refused; device offline falls back | later |

## Consequences
- We own a security boundary. This needs a STRIDE review before P3, fuzzing of the vsock and
  egress-proxy parsers, fast guest-kernel updates, and a public description of the isolation
  model.
- Agents get a real computer only when a tool says so, so the common case stays cheap and
  private.
- A provider (E2B and Browserbase have free tiers) can still be plugged in behind the trait
  for comparison, without changing app code.

## Decisions taken (Enzo, 2026-10-08)
1. SMT stays on with core scheduling; `smt=off` pool as the fallback for high-risk tenants
   (section 6b).
2. v1 has no residential proxies and no CAPTCHA-solving services: human takeover and signed
   agents (section 7).
3. Spend is asked for per phase, when that phase starts (P3 staging, P4 bare metal).
4. The egress proxy is a first-class component (section 6).
5. "Route through my device" is designed now (section 6c) and built later; off by default.
