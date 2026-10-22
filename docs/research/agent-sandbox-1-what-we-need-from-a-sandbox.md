[Research index](agent-sandbox.md).

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
