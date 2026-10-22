[Research index](agent-sandbox.md).

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
