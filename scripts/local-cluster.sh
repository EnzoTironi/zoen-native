#!/usr/bin/env bash
# The local Kubernetes proof (ADR 0012): k3d, the same manifests as staging and prod, the
# relay image built from this checkout, then the two-client journey through the cluster.
#   scripts/local-cluster.sh up        create the cluster, build, deploy
#   scripts/local-cluster.sh journey   port-forward the relay and run scripts/journey-remote.sh
#   scripts/local-cluster.sh down
# k3d flags: fuse-overlayfs shares image layers where the host is itself an overlay (the box);
# the native snapshotter would copy every layer in full and needed 14 GB for three
# workloads. The k3s image has no libfuse, so the node gets the upstream static
# fuse-overlayfs (checksum-pinned) and infra/k3d/mount.fuse3. host-gw flannel needs no
# vxlan module, so this runs inside other containers too.
set -euo pipefail
cd "$(dirname "$0")/.."
CLUSTER=zoen
NS=zoen
PORT="${ZOEN_LOCAL_PORT:-18787}"
FUSE_OVERLAYFS_VERSION=v1.18
FUSE_OVERLAYFS_SHA256=56b0ae0aeb8abb308b068af2f137ed8d1bd239f4f27e21672ff0def861eea1e8
FUSE_OVERLAYFS="$PWD/.tools/fuse-overlayfs/$FUSE_OVERLAYFS_VERSION/fuse-overlayfs"

fuse_overlayfs() {
  [[ -x "$FUSE_OVERLAYFS" ]] && return
  mkdir -p "$(dirname "$FUSE_OVERLAYFS")"
  curl -fsSL -o "$FUSE_OVERLAYFS.part" \
    "https://github.com/containers/fuse-overlayfs/releases/download/$FUSE_OVERLAYFS_VERSION/fuse-overlayfs-x86_64"
  echo "$FUSE_OVERLAYFS_SHA256  $FUSE_OVERLAYFS.part" | sha256sum -c --quiet
  chmod +x "$FUSE_OVERLAYFS.part"
  mv "$FUSE_OVERLAYFS.part" "$FUSE_OVERLAYFS"
}

up() {
  if ! k3d cluster list -o json | grep -q "\"name\": *\"$CLUSTER\""; then
    fuse_overlayfs
    K3D_FIX_DNS=0 k3d cluster create "$CLUSTER" --servers 1 --agents 0 \
      --volume "$FUSE_OVERLAYFS:/usr/local/bin/fuse-overlayfs:ro@server:0" \
      --volume "$PWD/infra/k3d/mount.fuse3:/usr/local/bin/mount.fuse3:ro@server:0" \
      --k3s-arg "--snapshotter=fuse-overlayfs@server:0" --k3s-arg "--disable=traefik@server:0" \
      --k3s-arg "--flannel-backend=host-gw@server:0" --wait --timeout 400s
  fi
  # Reach the API server on the node's address, which works even where published ports don't.
  local ip; ip="$(docker inspect -f '{{range .NetworkSettings.Networks}}{{.IPAddress}}{{end}}' "k3d-$CLUSTER-server-0")"
  kubectl config set-cluster "k3d-$CLUSTER" --server="https://$ip:6443" --insecure-skip-tls-verify=true >/dev/null
  kubectl config unset "clusters.k3d-$CLUSTER.certificate-authority-data" >/dev/null
  kubectl config use-context "k3d-$CLUSTER" >/dev/null

  docker build -q -f infra/fly/relay.Dockerfile -t zoen-relay:dev . >/dev/null
  k3d image import -c "$CLUSTER" zoen-relay:dev >/dev/null

  kubectl get ns "$NS" >/dev/null 2>&1 || kubectl create ns "$NS" >/dev/null
  if ! kubectl -n "$NS" get secret postgres >/dev/null 2>&1; then
    local pw; pw="$(openssl rand -hex 24)"
    kubectl -n "$NS" create secret generic postgres --from-literal=password="$pw" >/dev/null
    kubectl -n "$NS" create secret generic relay --from-literal=database-url="postgres://zoen:$pw@postgres-0.postgres:5432/zoen_relay" >/dev/null
  fi
  if [ -z "$(kubectl -n "$NS" get secret relay -o jsonpath='{.data.log-pseudonym-key}')" ]; then
    kubectl -n "$NS" patch secret relay --type merge \
      -p "{\"stringData\":{\"log-pseudonym-key\":\"$(openssl rand -hex 32)\"}}" >/dev/null
  fi
  kubectl apply --server-side -k infra/k8s/operators/foundationdb >/dev/null
  kubectl wait --for condition=established crd/foundationdbclusters.apps.foundationdb.org --timeout=60s >/dev/null
  kubectl -n "$NS" rollout status deploy/fdb-kubernetes-operator-controller-manager --timeout=300s
  kubectl apply -k infra/k8s/overlays/local >/dev/null
  # The relay can't start until the operator has published the cluster file.
  kubectl -n "$NS" wait foundationdbcluster/zoen-fdb --for=jsonpath='{.status.health.available}'=true --timeout=600s
  kubectl -n "$NS" rollout restart deploy/relay >/dev/null
  kubectl -n "$NS" rollout status statefulset/postgres --timeout=300s
  kubectl -n "$NS" rollout status deploy/relay --timeout=300s
}

journey() {
  kubectl -n "$NS" port-forward svc/relay "$PORT:80" >/tmp/zoen-port-forward.log 2>&1 &
  pf=$!
  trap 'kill $pf 2>/dev/null || true' EXIT
  for _ in $(seq 1 50); do curl -sf "http://127.0.0.1:$PORT/healthz" >/dev/null && break; sleep 0.2; done
  scripts/journey-remote.sh "http://127.0.0.1:$PORT"
}

case "${1:-up}" in
  up) up ;;
  journey) journey ;;
  down) k3d cluster delete "$CLUSTER" ;;
  *) echo "usage: $0 [up|journey|down]"; exit 2 ;;
esac
