# Runtime relay transport binding

The certified Runtime device must connect to the same relay cell and SQL
directory used by its retained native execution. A configured URL, a matching
Challenge relay name, or a source lookup after publication does not establish
this before effects. This transport check is independent of owner approval,
finance admission, native device leases and restore continuity.

## Connection

The Runtime reads its original SQL namespace/witness and its actual conflicted
FDB deployment/source markers. It creates a random 32-byte challenge and derives
a private HMAC-SHA256 expectation over a versioned tuple containing namespace,
challenge, cell, relay, agent and device. The original pairing witness never
crosses the wire. The MAC uses the already-retained pairing, not a new journal.

The existing Hello/Auth connection negotiates `runtime-relay-binding-v1`. Its
request contains only namespace and challenge. The relay reads the deployment
witness from its own SQL directory, checks the agent and current device, and
asks its actual FdbLog to check both original markers against the cell encoded
by that log. Unsupported stores and missing/mismatching markers refuse.

Only an exact reply on the same socket exposes a bound output transport. A
different challenge, namespace, pairing, cell, relay or actor fails. The object
has no reconnect operation and retains that socket for publication. HTTPS/WSS
remains required outside explicit numeric loopback development endpoints.

## HTTP uploads

A reverse proxy can send WebSocket and HTTP paths to different services. Bound
PUT therefore includes the complete proof in `x-zoen-runtime-binding` and signs
a separate `zoen-runtime-blob-put/1` message containing the entire binding,
content hash, timestamp and relay. The actual HTTP handler rechecks its own
SQL/FDB pair before quota charging or object-store writes.

Stripping the header cannot downgrade this signature into the legacy PUT
domain. Malformed/oversized headers, changed proof fields, mismatched stores
and revoked devices refuse. Ordinary messaging keeps its existing signed PUT
format and policy. Existing create-if-absent blob semantics and hash validation
remain unchanged. A valid proof grants no independent permission to upload:
the Runtime effect path still requires its native/owner/finance checks.

## Evidence and limits

Issue #371 tracks qualification. Source includes protocol bounds and field
binding tests, signature downgrade tests, a connection negotiation/replay test,
and a real SQL/FDB/WS/HTTP/object-store scenario inside the existing Image
journey. Test implementation is not a passing result; exact commands, source,
failures and results belong to the issue after execution.

Matching store markers cannot distinguish a coordinated rollback of both
stores. This code does not mint production continuity, a fresh output namespace
or an old-writer retirement witness. Those barriers remain closed under #366.
Runtime review publication also needs this helper when integrated into #369;
this increment connects retained output delivery first. No paid provider call
or iPhone journey is accepted by these transport checks.

## Operator registry for managed Images

Managed devices must be registered outside SQL/FDB restore state. At startup,
`serve` and `build` read the optional **operator-only** environment variable
`ZOEN_MANAGED_DEVICE_CONFIG`. Its value names an absolute private JSON file in a
private directory. The loader reads once, caps it at 64 KiB/256 devices, rejects
unknown fields, relative/traversing paths, symlinked config/parent and public
permissions, and reports generic errors without echoing contents or witness.
Omitting the variable preserves the ordinary startup. An explicitly invalid file
fails startup; it never falls back to an empty registry.

The shape below is illustrative; the existing private pairing must be supplied
by the operator, not copied from a network request or invented for a retry:

```json
{
  "devices": [{
    "deployment": {
      "namespace": "existing-runtime-namespace",
      "witness": "<private existing SQL/FDB pairing witness>",
      "source": "existing-relay-cell",
      "agent": "existing-certified-agent",
      "device": "<existing lowercase 64-hex certified device>"
    },
    "anchor_directory": "/absolute/existing/private/independent-anchor"
  }]
}
```

Programmatic hosts may instead call `ManagedDevices::open`, then
`build_with_managed` / `serve_with_managed`; tests serving an existing FdbLog may
configure it once with `configure_managed_devices`. Config/AppState do not acquire
new fields. The registry lives in that log's immutable OnceLock and cannot be
removed by restoring SQL/FDB rows. Duplicate namespace/device, different source
cell or a missing/replaced anchor refuse. No method bootstraps a replacement
anchor. Initial managed operation is single-host: forwarding configuration is
refused, and a managed permit never crosses the ordinary cell RPC.

Startup order is: initialize the certified dedicated Runtime device and original
paired namespace; bootstrap/open its independent anchor; stop the relay; write
the private operator config; restart the actual relay with that config **before
any managed effect**. Keep both the config and anchor outside coordinated
SQL/FDB backup/restore. The Runtime production-admission guard remains closed
until its separate requirements are met; setting this environment variable does
not open that guard.

A registered managed device must prove its managed checkpoint on the existing
socket. A failed rebind clears the retained proof. Every publication independently
rechecks SQL/FDB and the live original anchor history. An ordinary Publish without
this binding is refused, including ordinary routed/fenced log entry points. A
fresh external consumption moves its linear permit into the real queued append.
Consumed history permits only a read of the exact original event; missing evidence
returns uncertainty, never another append, fresh permit or replacement client ID.
Later unrelated checkpoints do not invent or erase the original retained record.

HTTP PUT from the registered device requires the binding even with a valid legacy
signature. Upload checks require the exact current checkpoint and hold its OS
generation guard through the real object-store task. Cancelling the handler drops
the caller's JoinHandle, not the task's guard. Ordinary devices keep their current
WS/HTTP behavior. These source contracts and written tests do not constitute a
passing native/service qualification; #386/#394 record executed evidence and
#366/#345 retain product/iPhone acceptance.
