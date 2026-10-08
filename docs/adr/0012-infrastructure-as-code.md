# ADR 0012: Infrastructure as code (OpenTofu, Kubernetes, GitOps)

Status: accepted (tools chosen by Enzo); vendor recommendation awaiting his account and budget.

## Decision
- OpenTofu for cloud resources, modules per component (network, cluster, postgres, storage,
  dns-cdn), environments `local`, `staging`, `prod`, remote state with locking. The vendor
  lives behind the modules.
- Kubernetes workloads with Kustomize under `infra/k8s` (base plus overlays per environment).
  Kustomize over Helm for our own services: plain manifests, no templating language; Helm only
  for third-party charts (NATS, the OpenTelemetry collector), rendered through Kustomize.
- GitOps with Argo CD: an app-of-apps per environment, a UI for drift and rollbacks, and
  wide adoption with the FDB operator and NATS charts. Flux is lighter but has no UI.
- FoundationDB via the fdb-kubernetes-operator; NATS via its Helm chart; OpenTelemetry
  collector; Linkerd for mTLS.
- Secrets: External Secrets Operator against the cloud secret manager in staging and prod;
  SOPS with age for `local`. Keys never in git.
- CI: GitHub Actions for Rust tests, journeys, multi-arch images, `tofu fmt`, `tofu validate`,
  plan checks, tflint and kubeconform.

## Vendor recommendation
AWS in sa-east-1 (São Paulo): EKS, i4i instances with local NVMe for FoundationDB, RDS
Postgres, and Cloudflare for DNS, CDN, DDoS protection and R2 object storage (no egress
fees, which matters because media egress is the largest cost line). GCP has a São Paulo
region too, but AWS has the deeper bench of NVMe instance types there. No cloud `apply` until
Enzo approves an account and budget.

## Local proof
The box has no Docker, so kind or k3d can't run there yet. Until a container runtime is
available, the manifests are validated statically (kubeconform, kustomize build) and the same
binaries run natively against a local fdbserver and NATS server.

## At 1B users
About 1,800 nodes per the cost model, spread over cells; each cell is one Kustomize overlay
instance, so adding capacity is adding a cell overlay, not editing manifests.
