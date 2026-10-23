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
