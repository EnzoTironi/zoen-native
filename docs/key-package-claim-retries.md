# Durable MLS key-package claims

A single-use package is removed when the relay commits a claim, before its reply reaches
the client. Previously a socket loss or CLI shutdown in that interval caused the same
group reconciliation to claim again. The second claim consumed another package. The
client now persists one operation and its target/epoch/membership intent before sending,
and retries that operation after reconnect or process restart.

## Protocol and rollout

`ClaimKeyPackages` remains request tag 12. Its repeated identity strings retain field 1;
optional `operation_id` is field 2. A request without the new field retains its legacy
wire bytes and behavior. The protocol version stays 4.

The new core requires the explicit `key-package-claim-receipts` capability from the
**Challenge of the connected relay** before issuing a claim for a membership addition.
An omitted capability, including an older Hello/Challenge, defaults to unsupported.
Protocol 4 alone is insufficient. The same Challenge must include a valid optional
PostgreSQL `server_time_ms` (field 5). The client activates that socket-local sample only
after authentication reaches Ready. Pong retains message tag 11 and optionally carries
a refreshed clock in nested field 1; legacy empty Pongs remain byte-compatible. A later
Pong cannot enable a capability or clock missing from the initial Challenge.

Claim IDs use the exact database timestamp plus independent cryptographic randomness,
without the device-wall-clock clamp used for event IDs. Neither wall time nor elapsed
time advances a sample. Monotonic age only invalidates a sample once its 24-hour window
passes; fresh IDs then wait for a valid Pong or reconnect. Saved IDs replay unchanged.
Clock reads have the existing five-second database bound. Missing or malformed clock
advertisements leave claims disabled. Ordinary sync and removals continue, while additions remain saved
and surface an upgrade error instead of assuming an older relay will honor field 2.

Deploy migration `0024_key_package_claim_receipts.sql` with the relay implementation
before advertising the capability. Startup runs the new migration; earlier applied
migrations are unchanged. Deploying the new client first will defer additions on an old
relay. An old client on a new relay still has destructive, non-idempotent legacy claims.

## Transaction and retry boundary

The receipt primary key is `(source_identity, source_device, operation_id)`. Both source
values come from the authenticated session, never a request field. Targets must be
canonical lowercase 64-character identity keys, and are sorted
and deduplicated. Reusing a scoped operation with different targets is refused before
consumption; reordering or repeating the same targets replays the same result.

One PostgreSQL transaction holds a device-scoped advisory lock, removes the packages,
and inserts the encoded reply. Simultaneous retries serialize and get the identical
signed package bytes. Receipt insertion or capacity failure rolls back the removal.
Existing request-admission and per-frame delivery authorization still apply: work
admitted before revocation may complete; new requests and receipt delivery are fenced.

On the device, `mls.claim:<space>` in SQLite contains the canonical operation, device,
epoch, exact membership intent, the sorted distinct target batch (at most 50 identities),
and eventually the returned packages. Larger groups confirm one batch before claiming
the remaining identities under a new operation; unrequested members are not treated as
missing-package failures. The reply is saved
before MLS staging. MLS state, staged commit/Welcome, and claim retirement share one
SQLite transaction. A staging failure retries the cached reply; a socket loss retries the
same operation. A changed intent creates a new operation. A completed empty reply is
retired so the unchanged five-second retry can claim newly published packages.

## Retention and capacity

An operation is a canonical 26-character uppercase ULID. Prefixes, aliases, overflowing
leading bits, future timestamps, and operations at least 24 hours old are refused. The
server checks the database clock after acquiring the claim lock. The expiry is derived
from the operation's creation time; waiting or replay never extends it. After GC, an
expired ID cannot become a fresh destructive claim.

Each source device retains at most 4096 live receipts and 8 MiB of encoded replies.
Existing receipts remain replayable at capacity. A fresh claim exceeding either bound
is refused, preserving stock. Expired receipts for an active source are deleted under
its claim lock. A background worker deletes at most 256 indexed expired rows per minute
per relay node, using `SKIP LOCKED`. These bounds are correctness and resource guards;
they do not prove billion-user storage capacity or cleanup throughput. Production load
tests must measure receipt rate, retained bytes, lock waits, and GC backlog.

## Completion and evidence

The online CLI group command now requires actual settled sync before printing success.
Explicit `--offline` still creates and queues a local group for later synchronization. An
empty event outbox is insufficient while a claim, scheduled MLS retry, commit/Welcome,
or recovery upload is unfinished. The default CLI deadline remains eight seconds;
expiration returns a failure and preserves local work. Public Apple/Android idle-wait
ABI behavior is unchanged; the stricter CLI method is Rust-only.

The relay low-water threshold remains 8, the client stock target remains 32, and the
existing refill journey still requires 32 within 20 seconds. No quota or timeout was
increased. Test transport faults are confined to the journey harness.

The new regressions cover a real committed reply lost across restart, an unissued
request at the CLI deadline, a scheduled retry outside the event outbox, capability
absence and invalid advertised clocks, a phone event clock 60 seconds ahead, 51-member
batching, independent chat removals during capability fallback, changed/canonical targets, identity/device scoping and revocation, operation
expiry, simultaneous retries, count/byte saturation rollback, bounded GC, and a genuine
MLS Welcome plus readable subsequent message.

Historical failures remain separate:

- Main `e051f97`, CI `38055889469`, finished at **31**: live Low(7), generation of 25,
  successful publication, and a later blob upload. A blob upload does not prove durable
  claim or MLS completion. The log lacks per-claim IDs, so it cannot establish which
  operation was retried or prove reply loss caused that run.
- PR38 `38058275281` and PR44 `38058379030` finished at **8**, with a ready watcher and
  no live Low. Those logs require verifying whether the last producer's claim completed;
  a persisted receipt cannot issue work after an early process exit.
- Older **7** failures without client Low/generation, and the separate interleaved-log
  startup false negative, are not explained by the two cases above.

The regression demonstrates a concrete production defect. Closing the historical
refill gate requires fresh combined runtime and CI evidence; this document makes no
claim that all three failure histories have the same cause.
