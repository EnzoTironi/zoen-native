# One local URL for two native processes

The optional gateway is standard HAProxy 3.4.7, pinned to the verified official OCI index. It forwards HTTP and WebSockets to the existing Rust servers. PostgreSQL, FDB, S3 and NATS retain their existing authority; the gateway adds no authentication or protocol implementation.

After the two-node environment is healthy, explicitly obtain the pinned image and enable the gateway:

```sh
docker --context orbstack pull mirror.gcr.io/library/haproxy@sha256:251da69821897ec8c4819f12b1babbc4b71f562943addc9b9eb14bb6ece40dc6
python3 infra/local/manage.py frontdoor
python3 infra/local/manage.py status
```

Omit `--context orbstack` on Linux. The mirror serves the exact official index digest; no image is built or pulled implicitly. Activation checks ownership, both native listeners, all six existing healthy services, the installed image, the actual HAProxy parser and the free loopback port before recording the enabled state. It starts only `frontdoor` with `--no-deps`, then verifies the six native container IDs, process IDs, image IDs and configuration hashes are unchanged.

New local accounts and browser pairing can use base `http://127.0.0.1:8882` and wire endpoint `ws://127.0.0.1:8882/v1/sync`. The default port is the instance's base port plus two; select another unprotected port explicitly with `frontdoor --frontdoor-port PORT`. Existing accounts configured for 8880 remain on that URL. No IdentityBox, device certificate, SQLite or browser storage is rewritten to bypass relay equality.

The configuration is copied into the private instance directory, made read-only and hash-pinned. The container cannot observe edits to the repository template. The listener is loopback-only; the Unix statistics socket is private and read-only. Traffic logs contain only request kind, fixed upstream name, status and counters, without paths, query strings, identities or headers.

If startup returns an ambiguous error, the exact enabled descriptor, image and configuration remain recorded. Retry the same `frontdoor` command; it validates that state rather than rolling it back or adopting unknown resources. To remove only this component while retaining the six native services:

```sh
python3 infra/local/manage.py frontdoor --disable
```

Normal `drain`, `restart`, `backup` and `down` stop the gateway before the native services or network. No data volume is added. Cold backups include its exact image ID and configuration bytes/hash. Restore validates these and the new listener port before creating resources, then uses the same image/configuration. A clone restored on 8890 defaults to gateway 8892; use `restore --frontdoor-port PORT` when needed. The original listener is not moved or replaced. Older snapshots without a gateway remain valid and disabled.

Deliberate `pin`, after `down` has removed all instance containers, updates the declared standard image and repository configuration together. Retain the original cold backup and required exact image artifacts for rollback. `up` never updates the pin automatically.

The real-client proof demonstrated different WebSocket upstreams, live encrypted message delivery and Delivered/Read without reconnect, plus encrypted media/recovery. A separate gateway-enabled cold restore now passes with two relays, four fresh data volumes and the exact historical images, vault keys and configuration. A device created and publicly linked after restore decrypts the retained history and downloads the exact 1,049,393-byte attachment. Stateless native GET also verifies the restored ciphertext. The original development stack remains untouched; see `postMergeCheckpoint.localMultiRelay` in the [acceptance record](../../apps/zoen/acceptance.json) for the retained receipts.

Port checks permit a stopped listener's remaining `TIME_WAIT` connections and still reject an active listener, including one using `SO_REUSEADDR`. They never use `SO_REUSEPORT`. This is a preflight rather than an atomic reservation: Docker still refuses a port claimed before container startup. The actual backup restart exposed this case; both the regression and the repeated backup/restore pass. The 26 lifecycle/ownership tests complement these runtime proofs. This remains a local configuration, without public TLS, WAN exposure, HA, scale or power-loss acceptance.
