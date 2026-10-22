[Research index](agent-sandbox.md).

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
