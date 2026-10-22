[Research index](agent-sandbox.md).

## 4. Where KVM is available

| Host | KVM for our own VMs | Source |
|---|---|---|
| AWS `*.metal` | yes, bare metal | [A1] |
| AWS virtual instances | since Feb 2026 on C8i/M8i/R8i, since Jun 2026 also C7i/M7i/R7i and others; Intel only; AWS still recommends metal for latency-sensitive work | [A2] [A3] [A4] |
| Google Compute Engine | nested virtualization on Intel VMs (Haswell+), not ARM, AMD only on N4D | [GC1] |
| Azure | nested virtualization on Dv5 and others | [AZ1] |
| Hetzner dedicated (AX) | yes, bare metal | [H1] |
| Fly.io Machines | **no** nested virtualization | [FL3] |
| The box (this dev machine) | yes, nested (the box is itself a VM); the `box` user is in the `kvm` group | measured |

### Measured on the box (P1, 2026-10-08)

`scripts/firecracker.sh measure 20`: Firecracker v1.17.0, 6.1 guest kernel, 2 vCPUs and
128 MiB per VM, alpine root filesystem, shell script as init. The box is a VM itself (nested
KVM, 8 vCPUs, other agents' builds running), so these are upper bounds; bare metal is faster.

| What | Measured | What it means |
|---|---|---|
| Cold boot, process start to init running | p50 940 ms (733–1088 ms, 20 runs; an idle box gave 672–760 ms) | too slow to do per call; we boot once per template |
| Full snapshot of a running VM | 292 ms, 128 MiB memory file | paid once per template build |
| Restore, process start to snapshot loaded and running | p50 9.7 ms | |
| Restore, process start to the guest answering | p50 17.8 ms (12.5–36.4 ms) | about 50× faster than booting; this is what a user waits for |

The restore number uses the memory file mapped privately (pages load on first touch), not
userfaultfd; userfaultfd matters when memory files live on remote storage.

Through `zoen-sandboxd` (jailer, cgroups, vsock; `cargo test -p zoen-sandboxd`, 2 vCPUs,
256 MiB, same box):

| What | Measured |
|---|---|
| Template build, once per shape (cold boot to `zoen-guestd` + full snapshot) | 1.8–2.6 s |
| Acquire from the warm pool (resume + fresh entropy and clock) | 22–38 ms |
| Exec round trip (`true`) in a warm VM | p50 3.2–3.5 ms |
| HTTPS call with key injection from inside the VM (curl → vsock → proxy → origin) | p50 18 ms, first 71 ms |
| Suspend (full snapshot of 256 MiB) / resume | 0.6–1.4 s / 54–61 ms |
| CPU quota: guest burning 2 vCPUs under a one-core quota | 1.01–1.02 cores used, throttled |
| VMM memory charged to its cgroup, idle small VM | 12–17 MiB of a 320 MiB cap |
