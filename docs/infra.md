# Infrastructure

Everything is code under `infra/`; nothing secret is in git.

| path | what |
|---|---|
| `infra/fly/` | staging on Fly.io gru (ADR 0015): `relay.toml`, `postgres.toml`, `fdb.toml`, the two Dockerfiles, `deploy.sh` |
| `infra/fdb/` | entrypoints: the staging `fdbserver` (configures itself once) and the client side (writable cluster file from `FDB_CLUSTER` or a mounted copy) |
| `infra/tofu/modules/edge-dns` | Cloudflare CNAMEs for subdomains only (refuses the zone root) |
| `infra/tofu/envs/staging` | `relay`, `api`, `media`, `id`.tryzoen.com → `zoen-staging-relay.fly.dev` |
| `infra/k8s/base` | relay Deployment, Postgres StatefulSet, `FoundationDBCluster`, Services (the same manifests for local and, later, AWS) |
| `infra/k8s/operators/foundationdb` | fdb-kubernetes-operator v2.37.0 (CRDs + controller), applied before the base |
| `infra/k8s/overlays/local` | the k3d overlay |

## Local cluster (k3d on the box)

```sh
scripts/local-cluster.sh up        # cluster, relay image from this checkout, Postgres, relay
scripts/local-cluster.sh journey   # port-forward, then scripts/journey-remote.sh through the cluster
scripts/local-cluster.sh telemetry # relay logs and traces reach the collector, also after it moves pods
scripts/local-cluster.sh down
```

Why k3d and these flags: kind's control plane can't start on a Docker using the vfs storage
driver (the box). k3d runs with `--snapshotter=fuse-overlayfs` (layers shared on an
overlay-backed host; the earlier `native` snapshotter copies every layer in full and filled
the disk with 14 GB for Postgres, the relay and the FDB operator; the whole cell now takes
4 GB). The k3s image ships no libfuse, so the node gets the upstream static `fuse-overlayfs`
(version and SHA-256 pinned in the script, cached under `.tools/`) and `infra/k3d/mount.fuse3`,
a ten-line helper that hands containerd's mount to it, both mounted read-only, and
`--flannel-backend=host-gw` (the box kernel has no vxlan). kube-proxy runs in `nftables`
mode: its default iptables mode balances a Service over several pods with `-m statistic`, the
box kernel has no `xt_statistic`, and because `iptables-restore` is atomic one such rule (NATS,
kube-dns) failed every sync, freezing all Services at the pods that existed first. A rolled
collector then silently stopped receiving relay telemetry; `scripts/local-cluster.sh telemetry`
now proves delivery before and after rolling the collector. Eviction starts at 2 GiB free
(not 5% of a disk the node shares with the rest of the machine). `K3D_FIX_DNS=0` keeps
Docker's embedded DNS.

One-time box fix, already applied: an older Docker had left `iptables-legacy` rules with a
`FORWARD DROP` policy that only allowed `docker0`, so containers on user-defined bridges
(k3d's network) had no egress or DNS. Allowed with:

```sh
sudo iptables-legacy -I FORWARD 1 -i br-+ -j ACCEPT
sudo iptables-legacy -I FORWARD 1 -o br-+ -m conntrack --ctstate RELATED,ESTABLISHED -j ACCEPT
sudo iptables-legacy -t nat -A POSTROUTING -s 172.16.0.0/12 ! -o docker0 -j MASQUERADE
```

Proof (2026-10-08): `scripts/journey-remote.sh` passed through the cluster: two accounts,
handle lookup, a DM both ways, an encrypted photo through the blob store, chains verified.
`kubeconform -strict` validates the rendered overlay (6 resources).

FoundationDB runs under fdb-kubernetes-operator (S3): `local-cluster.sh up` applies the
operator, waits for the `FoundationDBCluster` to report available, then rolls the relay, which
mounts the operator's `zoen-fdb-config` ConfigMap. NATS (S5) runs as a 3-server StatefulSet
(`nats.yaml`).

The OpenTelemetry Collector (S7, ADR 0021) is the cell's only telemetry egress:
`otel-collector.yaml` (2 replicas, contrib 0.162.0 pinned by digest, namespace-scoped pod
discovery) with its config in `otel-collector.config.yaml`, rendered into a hashed ConfigMap
so a config change rolls the pods. Relays send OTLP/HTTP to `otel-collector:4318`, sampled at
10%, and the collector scrapes their `metrics` port. Its `debug` sink is what overlays
replace with a real backend. To run the same collector on the box:
`.tools/otelcol/otelcol-contrib --config=infra/k8s/base/otel-collector.config.yaml
--config=.dev/otel/local.yaml` (the local file swaps pod discovery for a static target; see
roda-shots/real-s7/collector-proof.sh).

## FoundationDB in development

`scripts/fdb.sh up` installs FoundationDB 7.3 under `.tools/fdb` (no root; Linux .deb or the
macOS .pkg, extracted) and runs one `fdbserver` on 127.0.0.1:4500 with data in `.dev/fdb`;
`eval "$(scripts/fdb.sh env)"` exports the cluster file and library paths builds and tests
need. `dev-stack.sh` and `journey-sim.sh` call it. Building the relay needs libclang
(bindgen): `apt install libclang-dev` on Linux, Xcode's on macOS.

If every append suddenly hangs, check the disk: FoundationDB's ratekeeper stops admitting
writes when a storage server has less than about 5% free (`fdbcli --exec status` shows it
under "Operating space"). Cargo's `target/debug/incremental` is the usual culprit on the box.

## Staging (Fly + Cloudflare)

```sh
infra/fly/deploy.sh                       # on the Mac, where fly is logged in
cd infra/tofu/envs/staging && tofu apply  # with CLOUDFLARE_API_TOKEN in the environment
scripts/journey-remote.sh https://relay.tryzoen.com
```

The deploy script creates missing apps, the Postgres and FoundationDB volumes, generated
secrets (straight into `fly secrets`) and the Tigris bucket, then deploys and adds
certificates for the four hosts. FoundationDB is private (`zoen-staging-fdb.internal:4500`)
and configures itself on first boot.
Cost: ADR 0015.

## Shared simulator

One iOS simulator is shared by every worker on the Mac. A run that installs the UI-test
runner kills any other runner mid-bootstrap ("Test crashed with signal kill before establishing
connection"). `scripts/journey-sim.sh` holds `/tmp/zoen-simulator.lock` (a directory, `mkdir`
is atomic; `owner` names the holder) for the whole build-and-test run and refuses to start while
any `xcodebuild` is running. Anything else that runs `xcodebuild test` on the shared simulator
should take the same lock: `mkdir /tmp/zoen-simulator.lock` before, `rm -rf` after.

## CI (GitHub Actions)

`.github/workflows/ci.yml` runs on every push to `main` and `real/**` and on pull requests, on
`ubuntu-24.04` only (no macOS minutes): `cargo fmt --check`, `cargo clippy --workspace
--all-targets -D warnings`, and `cargo test --workspace`, which includes every box journey against
a real relay, Postgres 17 (container), FoundationDB (`scripts/fdb.sh up`) and NATS
(`scripts/nats.sh up`). Actions are pinned to commit SHAs; the toolchain is pinned to 1.99.0. The
simulator journey stays on the Mac (`scripts/journey-sim.sh`).
