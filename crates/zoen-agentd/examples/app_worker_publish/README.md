# Publish a managed App worker

This offline operator entry point signs the existing `ToolManifest` format for
any managed App's WASIp2 component. It creates a **new signed manifest file**;
it does not create an identity, install a worker, enroll an agent, add a trusted
publisher, grant artifact access, alter a budget, or contact a provider/service.

Build the normal example, then invoke the resulting executable:

```sh
cargo build -p zoen-agentd --example app_worker_publish
/path/to/app_worker_publish \
  /absolute/component.wasm \
  /absolute/unsigned-manifest.json \
  /existing/operator-custody/seed-file \
  EXPECTED_PUBLIC_PUBLISHER_HEX \
  /absolute/new-signed-manifest.json
```

`EXPECTED_PUBLIC_PUBLISHER_HEX` is the **existing public** 32-byte Ed25519 publisher
in 64 hexadecimal characters. The seed file is the existing raw 32-byte Ed25519
seed, owned by the invoking Unix user, mode `0600`, regular and not a symlink.
The command opens it with `NOFOLLOW` and zeros its temporary seed buffer; it never
prints the seed or its hash. Do not provide a seed as a command argument, convert
it through a model, place it in a fixture, or create a replacement publisher.
The command must be run by the authorized custodian using its normal internal
key path; a model only needs the public interface and resulting receipt.

The unsigned JSON uses the public `ToolManifest` fields:

```json
{
  "id": "your-app-canonical-v1",
  "version": "your-pinned-revision",
  "summary": "Your managed App worker",
  "needs": { "kind": "wasm" },
  "egress": [],
  "secrets": [],
  "component_sha256": "sha256-of-the-exact-component-file"
}
```

The command reads the actual component (maximum 128 MiB), checks its component
model header, hashes the exact bytes and requires the manifest's digest to match.
The header check does not claim full WASI validation: the normal runtime loader
still validates/compiles the component and enforces its App/revision binding.
Unsigned and signed JSON are each bounded to the normal loader's 64 KiB. `needs=wasm` and empty egress/secrets are
required for this managed App path; the existing validator handles manifest rules.
After signing through `ToolManifest::sign`, it verifies both the signature and the
serialized roundtrip through `SignedManifest::verify` with the expected public key.

The destination must be an absolute path in an existing directory and must not
already exist (including a dangling symlink). The complete JSON is staged in that
same directory, synced and published atomically without replacement. Failures
remove staging and never expose a partial signed manifest or replace existing
bytes. The receipt contains public ID/revision/publisher, component byte count/hash,
output path/hash, and `registered:false`. Preserve the signed manifest and exact
component bytes together; a later replacement of component bytes is rejected by
the normal loader. Publication alone does not change any runtime registration.

Use the existing managed App runtime configuration with this output and its
matching publisher/component, preserving the current account, agent, definition,
trust, budgets and scopes. For PDF, see the normal
[deployment instructions](../../../../miniapps/pdfcraft-ui/DEPLOYMENT.md).
No fixed Word/Photo/PDF helper, private-key export, or new key manager is needed.
