# Native server, local environment

This runs the existing Rust server against real PostgreSQL 17, FoundationDB 7.3.71 and SeaweedFS S3 storage. No server API or database implementation is replaced. Only the relay's HTTP/WebSocket listener is published, on `127.0.0.1:8880`. Database, S3, storage administration, NATS and metrics remain on private Docker networks.

Requirements on Linux or macOS: Docker Engine with Compose 2.24.4 or later, Buildx, Git and Python 3.9+. On macOS, start OrbStack first; these tools select its context explicitly. The FDB distribution is AMD64, so image builds and the two native server containers use AMD64 even on Apple Silicon. The build uses two compiler jobs, a 4-GiB/two-CPU builder and at least 5 GiB of free host disk. The first build may take several minutes.

```sh
python3 infra/local/build.py --proof

docker pull postgres:17-bookworm
docker pull chrislusf/seaweedfs:4.48@sha256:4e61d15fd35994cb1e43e1e553dff106794841fd9a99ade2fc8c8bfce4d7872d
docker pull debian:bookworm-slim

python3 infra/local/manage.py init
python3 infra/local/manage.py pin
python3 infra/local/manage.py up
python3 infra/local/manage.py status
```

Use `docker --context orbstack` for the three explicit pull commands on macOS. Building snapshots only Rust manifests/sources, the UniFFI tool and native container infrastructure into `.dev/local/build-*/context`; it does not send the full checkout, `.git`, application dependencies or session configuration to Docker. Build logs, source hashes and resulting image IDs are retained alongside that context. A private owner marker binds the reusable builder to its exact Docker engine/container and resource limits. An exclusive build lock prevents concurrent compiler jobs; the builder stops afterward while its compiler cache is retained. Compiler/log/disk/deadline guard failures fail the build.

`init` creates per-instance credentials and a recovery-vault key under the Git-ignored `.dev/local/default`, with directory mode 0700 and private files mode 0600. It never replaces an existing instance. `pin` resolves every installed service and archive-tool image to its exact local image ID before containers are created. Compose cannot pull replacement images implicitly. Keep the resulting owner file, secrets and backup together; losing the vault key prevents recovery decryption.

The native server applies its own SQL migrations and verifies FDB ownership on startup. `up` requires healthy dependencies and the actual native `/readyz` response; `/healthz` alone is insufficient. Connect clients to base URL `http://127.0.0.1:8880` (`ws://127.0.0.1:8880/v1/sync` is the wire endpoint). The browser uses the public native protocol and signed upload paths. Provider sign-in is a separate validation from a synthetic public-client account journey.

## Stop, restart and restore

```sh
python3 infra/local/manage.py drain
python3 infra/local/manage.py up
python3 infra/local/manage.py restart
python3 infra/local/manage.py backup
python3 infra/local/manage.py restore --backup /absolute/path/to/the/owned/backup --instance restored --port 8890
python3 infra/local/manage.py down
```

`drain` sends the native relay its normal graceful shutdown signal and leaves dependencies running. `restart` stops all owned services before restarting them. The FDB entrypoint forwards shutdown to its real server process and waits. `down` removes only containers/networks belonging to that instance and retains all data volumes. No command deletes persistent volumes.

Backup is a **cold snapshot**: all instance services stop before PostgreSQL, FDB and S3 volumes are archived, then the original stack resumes. The manifest records exact image IDs, data hashes and all three private environment-file hashes. Restore checks those files and the availability of every exact image **before creating resources**. It creates a new named instance and fresh volumes; it cannot overwrite an existing instance. It restores original credentials/vault keys and starts the same binary/database/storage versions. The restored copy is isolated for verification; choose one authoritative environment before directing real clients to it, since independently serving both copies would fork history. Missing images cause failure rather than an upgrade or download. Retain the required image artifacts (or export them explicitly) before pruning Docker images. Backups contain private credentials and data; they stay in the private ignored local directory and are not cloud backups.

To update deliberately, first create a cold backup, then run `down` (which keeps volumes), `pin --relay-image NEW_VERIFIED_TAG --fdb-image EXACT_FDB_TAG`, and `up`. The new images must already be built/pulled and qualified. `pin` refuses an instance with existing containers, including stopped containers; no running binary changes silently. For rollback, restore the pre-update backup into a **new** instance on a free port using the manifest's exact historical images. Reusing old data with an arbitrary older binary is not a supported rollback.

Ownership is checked against both the private instance identity and Docker labels before lifecycle operations. Ports 8790, 5547 and 8089 are reserved for existing development/user resources and cannot be chosen. Use another instance name and port for independent environments. Do not manually attach foreign containers or volumes to an owned Compose project.

## Optional second relay

After the single-node environment is healthy:

```sh
docker pull nats:2.15.0-alpine
python3 infra/local/manage.py cluster
python3 infra/local/manage.py status
```

This restarts the first relay with its existing native NATS bus and adds the identical second relay on `127.0.0.1:8881`. Both use the same PostgreSQL/FDB/S3 authority. The bus and its volume are private and included in cold backups. This is useful for cross-process delivery and fault tests. A single-machine PG/FDB/storage stack is not a high-availability deployment or evidence of production capacity.

## Apple and Google sign-in

The local server initially has no provider bindings. The canonical public client IDs are in `apps/zoen/config/identity-providers.json`, and the Apple bundle audience is in `apps/zoen/app.json`. To exercise real provider tokens, configure the corresponding `ZOEN_AUTH_*_AUDIENCE` / `ZOEN_AUTH_*_PRESENTER` fields in the instance's private `relay.env`, following `infra/fly/relay.toml`. Set exact browser origins in `ZOEN_AUTH_WEB_ORIGINS`, for example `http://localhost:8097,http://127.0.0.1:8097`; HTTP is allowed only for loopback. Recreate the owned relay through `up` after changing its environment. Google must also authorize the selected browser origin in its own console. Apple Web requires its separate Services ID/domain setup. No provider secret, fake token, JWKS override or permissive auth option is added by this environment. Real Apple/Google confirmation remains a separate pending UI/provider test until performed.

## Verification

```sh
python3 -m unittest discover -s infra/local -p 'test_*.py' -v
```

Run the real S3 conditional-write and storage restart check after building with `--proof`:

```sh
python3 infra/local/verify-s3.py
```

It races two different payloads using the native `object_store::PutMode::Create`, requires one winner, verifies repeated writes cannot replace it, checks exact bytes/hash and then verifies the same object after a real storage stop/start. The probe runs inside the owned private network; S3 credentials are not passed in arguments. Keep its private output directory to check the same object after a full stack restart or the restore drill:

```sh
python3 infra/local/verify-s3.py --instance restored --retained /absolute/path/to/original/proofs/s3-owner
```

The unit tests are integrity/ownership checks, not a substitute for live validation. Acceptance requires actual native client WebSocket receipts, encrypted attachment/recovery round trips, concurrent S3 create-only immutability and a real restart/restore drill. Retain exact image/source IDs and failures alongside successful receipts. A local pass does not establish HA, capacity, physical-device recovery or completed provider sign-in.
