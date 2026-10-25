# PdfCraft operator configuration

The UI resource and headless worker use the existing managed App deployment paths.
Neither configuration creates an account, enrolls an agent, grants document access,
changes its definition/budget or imports another application's storage.

## UI resource

Run `build-bootstrap.mjs ABS_OUTPUT` and
`build-resource-manifest.mjs RUNTIME_JSON SEALED_ASSETS OUTPUT_JSON` with the real
sealed JS/WASM package. The latter delegates to the shared producer, verifies
actual bytes/hashes, and binds the configured manifest to the embedded HTML.
Provide its contents as `ZOEN_PDFCRAFT_RESOURCE_MANIFEST_JSON` to both the normal
native build and the agent-host CLI build. This is compile-time catalog/resource
configuration: setting only the worker's runtime environment does not enable
creation/tool discovery in a CLI compiled with the unavailable default.
The committed default has no runtime package and leaves discovery off.
The final cohort handoff supplies the exact local manifests and immutable route
map; only the designated operator installs those routes in its existing HTTPS
server. Preparing a manifest is not serving or accepting it on an iPhone.

## Headless worker

The existing public types are `zoen_agentd::{ToolManifest, SignedManifest, Needs}`
in [manifest.rs](../../crates/zoen-agentd/src/manifest.rs). A PDF payload has:

```json
{
  "id": "pdfcraft-canonical-v1",
  "version": "d445410807e7eefbe5b96f9662ab963ae8dd2a3a",
  "summary": "Pinned managed PdfCraft canonical worker",
  "needs": { "kind": "wasm" },
  "egress": [],
  "secrets": [],
  "component_sha256": "3a21a67265c3b9125fa4c2e45c2216bd6e710dc6fe58e937eeb60d820fd44bd4"
}
```

The designated operator uses its existing publisher and the normal
`ToolManifest::validate` / `ToolManifest::sign` API. That API signs the protocol's
domain-separated canonical serialization and returns `SignedManifest` containing
`manifest`, `publisher` and `signature`. There is no separate `sign-pdfcraft` CLI
command. This integration does not read publisher private keys, manufacture an
operator signature, or promote the synthetic test publisher to trusted status.

The existing CLI loader in [deckcraft.rs](../../crates/zoen-cli/src/deckcraft.rs)
accepts these three operator environment values together:

- `ZOEN_PDFCRAFT_WORKER_COMPONENT`: absolute path to the sealed WASIp2 component.
- `ZOEN_PDFCRAFT_WORKER_MANIFEST`: absolute path to its operator-signed JSON.
- `ZOEN_PDFCRAFT_WORKER_PUBLISHER`: the trusted publisher's 32-byte public key as
  64 hexadecimal characters, matching `SignedManifest.publisher`.

Apply them to the **existing agent's** normal `zoen --home EXISTING_HOME agent-host`
launch, preserving its profile, owner, identity/device, relay, provider, Gate,
definition, budgets and other worker configuration. Partial configuration fails.
`configured_for` calls `DeckcraftWorker::load_for` or `load_for_cached`, checking
publisher/signature, exact App ID/revision/component digest and empty egress/secrets.
The common worker set retains all five fixed Apps and refuses duplicate bindings.
The existing optional `ZOEN_APP_WORKER_CACHE_DIRECTORY` remains operator-owned;
no document, model argument or guest may supply it.

The retained component's qualification and Cargo.lock remain bound to its original
producer receipt. Browser-only changes do not require rebuilding or re-signing it.
Changing the signed manifest changes the executor binding; preserve its accepted
identity across normal restarts so a pending approval is never silently rebound.

## Limits and acceptance

The common executor accepts 512 KiB of serialized PDF file bytes and 64 KiB of
command metadata, with 256 MiB memory, 2 billion fuel and a 5-second execution wall
limit. Compilation/startup are separate; use the normal private compilation cache.
The App's distinct 8 MiB / 32-page admission is not a larger agent execution limit.
Document JavaScript is disabled; engine/profile refusal never normalizes an
unsupported document silently. A valid signature alone grants no artifact access:
native owner/agent/source/device, permission, exact head/intent and CAS checks still
run through the existing Core and CLI consumers.

The iPhone acceptance remains creation from UI, manual text, real agent request,
explicit approval, same-session update/Undo, cold recovery and chosen-version
export. Host tests, controlled CLI fixtures and browser proofs are separate evidence.
