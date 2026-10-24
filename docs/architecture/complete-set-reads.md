# Complete device and agent-state reads (#217)

## Contract and compatibility

`complete-set-reads-v1` is an additive negotiated capability. `CompleteRead`
request tag 24 and `CompletePage` reply tag 16 are distinct from the existing
complete `Devices` and `AgentState` variants. Their meaning is unchanged.
An empty token begins a read; a 256-bit random token identifies only that
connection's retained read. Every page binds identity, requesting device, kind,
ordinal, cumulative count and a rolling SHA-256 commitment. The first agent page
includes the immutable definition/status header. A complete terminal is explicit;
a full last page is followed by an empty terminal. No partial-success variant is
passed to an existing whole-list consumer.

Ordering is unique `device COLLATE "C"` / `space COLLATE "C"`, never offset or
creation timestamps. A transaction that starts before a page and commits behind
its keyset cannot change that read. The page hash detects mixing/reordering; it
is not a new signature, permission or authority. The existing authenticated
connection and certified-device admission remain required.

An old client or old relay uses the existing whole-list variants. The new relay
assembles them through the same bounded snapshot path, returns one complete
result only within the existing transport ceiling, and otherwise returns an
explicit `complete_read_profile_exceeded` error. There is no successful truncation,
empty-on-error or invented cursor for an omitted space. The public Rust
compatibility helpers are also finite complete-or-error APIs, not unlimited ones.

## Independent finite budgets

| Stage | Operational ceiling |
| --- | --- |
| New SQL result page | 128 rows; selected device key 64 bytes / cursor key 128 bytes |
| Agent definition selected from SQL | 65,536 bytes, otherwise explicit failure |
| FDB membership page | At most 128 keys, one range call at the same read version |
| FDB range target | 64 KiB **soft** driver target; not a hard single-KV/RSS limit |
| Encoded CompletePage | At most 128 KiB, verified independently of row count |
| Total completed directory | At most 4,096 device rows |
| Total current membership / returned cursors | At most 16,384 spaces / cursors |
| Total encoded pages retained by one assembler | At most 4 MiB |
| Existing single legacy response | At most the existing 1 MiB `MAX_FRAME`; assembly reserves 1 KiB for outer headers and checks final encoded frame |
| Global retained snapshots | Four permits per process, one snapshot per authenticated connection |
| Native transport | One active paged read + at most 32 deferred read requests; existing overall waiter limits still apply |
| Legacy direct Rust helper query | Device LIMIT 4,097 / cursor LIMIT 16,385; over-profile returns error |

The service creates one shared AppState. Its new lazy dedicated PG pool has max
four connections and cannot consume ordinary, admission or delivery pool slots.
The semaphore is process-global, including independent AppState instances.
Embedding multiple AppStates must additionally budget each state's idle pool
connections; this is not a claim that every process can create unlimited pools.
The operational connection envelope must include these four connections beside
the existing ordinary/authentication/delivery pools.

The temporary membership relation and its index are O(N), bounded by 16,384
keys, and are derived storage inside the retained transaction. Client final
Vec/map/set reconstruction remains O(N), explicitly bounded above. Page limits
alone do not make whole-list consumers constant-memory. Protobuf conversions,
rolling-digest copies and frame/queue buffers are separate bounded allocations;
these numbers are not a whole-process RSS guarantee. Oversized/corrupt old rows
fail rather than disappear from the set.

The server retains a snapshot for at most 30 seconds after its construction;
periodic five-second session refresh also drops abandoned snapshots. SQL idle
transaction safety is 35 seconds, statement timeout four seconds, pool acquisition
one second, and the existing request-handler budget bounds construction. Native
and browser reads keep their original fixed 15-second transport deadline; it is
not renewed for each page. A native silent missing page closes that connection
and settles the read instead of retaining its lane forever. Browser expiry sends
a cancellation for the accepted token before beginning another correlation.
No failed mutation is automatically retried by this read protocol.

## Snapshot and current admission

Devices uses a PostgreSQL REPEATABLE READ transaction and keyset pages of both
active and revoked history. Revocations committed during the read do not rewrite
its earlier pages. A subsequent complete read observes the new state.

AgentState first captures one FoundationDB read version, then opens the paired PG
snapshot. Bounded FDB pages materialize the membership index at exactly that
version into an indexed transaction-local temporary relation. Cursors join that
relation, and definition/status/cursors use the same SQL snapshot. Expiration of
the FDB version is an explicit failed read; it is never refreshed mid-read. This
is a defined ordered pair of snapshots, **not atomic SQL/FDB consistency** and not
permission to execute a tool against historical membership.

Each page/terminal first goes through fresh durable requesting-device admission
outside the retained snapshot; the actual outgoing socket writer repeats its
existing device check. Agent pages additionally compare current owner, claimed
device, status and exact definition/version against the retained SQL header. A
change refuses the read. Tool/financial execution still requires its existing
current authorization.

The temporary relation is not authority and never replaces source membership.
A read no longer deletes cursors for spaces omitted by its membership snapshot:
that could clobber a concurrent higher Advance. Such cursors remain physically
stored but are filtered from the current complete projection. Any future garbage
collection needs its own safe generation/membership contract; a read or timestamp
alone cannot justify deletion. The ordinary AgentState heartbeat remains an
explicit SQL write. An unknown Begin acknowledgement does not establish rollback
and must not authorize an automatic mutation retry.

Cancel, completion, disconnect, timeout and page/build error drop the retained
SQLx transaction and global permit. The transaction owns its temporary table;
RAII rollback releases it. Tokens cannot move between sockets, accounts, devices,
kinds or ordinals. An unrelated stale token cannot cancel someone else's read.

## Consumer installation

The shared protocol assembler stages pages only in memory, checks owner/device,
kind/token, ordering/uniqueness, ordinal, count, rolling digest and all profile
ceilings, and poisons itself on error. `finish` requires a complete terminal.

Native Net translates negotiated Devices/State reads into this assembler. Only
the final legacy-shaped value reaches Client/Engine/startup. Account/device,
relay and current Net channel are rechecked at the public Client completion
boundary. Both active/revoked cache writes and MLS sibling updates use the existing
Engine savepoint and rollback/reload path; neither set is independently installed.

Browser Core stages pages outside SavedClient. Bootstrap/reconnect drops staging,
unique per-read correlations reject old delayed terminal pages, and the actual
connection's negotiated capability is supplied by the host before bootstrap.
Only a completed revoked set enters the existing encrypted saved snapshot and
persist-before-outgoing path. Errors preserve the previous full saved directory.

Host startup still requires a complete successful AgentState before it can mark
readiness or seed genuinely absent spaces. Missing, foreign, cancelled or failed
pages never reach that seeding loop. New pages do not change handled cursors,
approve a task, grant money or call a provider.

## Qualification scope

Source includes codec/assembler limits and binding tests; real local WebSocket
native staging/cancel/silent-timeout tests; browser staging/cold/late-correlation/
expiry tests; real PostgreSQL device snapshot/late-commit/revoke/resource/expiry
sockets; and a real PostgreSQL/FDB/CLI journey creating 257 actual MLS groups and
existing cursors through public APIs, interleaving Advance and membership changes
between pages, and inspecting the full result with the normal CLI. Synthetic
socket fixtures separately prove incomplete State never seeds omitted cursors or
publishes host readiness.

At the source-writing checkpoint, only the focused web connection/message tests
and TypeScript checks have run. Rust compilation, native/relay/CLI execution and
genuine WASM binding validation remain pending. Receipts must name the immutable
source executed; written tests and documentation are not runtime acceptance.
