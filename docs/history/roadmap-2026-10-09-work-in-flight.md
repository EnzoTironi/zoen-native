# Roadmap snapshot · work in flight

[October 9 roadmap](roadmap-2026-10-09.md)

## Work in flight

Six reviewed feature PRs are recorded as merged. The table distinguishes those integrations,
passing checks and the remaining native/UI proof. A green check or merge does not
establish product completeness.

| PR | Merge / reviewed head | Scope | Integration status / remaining proof |
|---|---|---|---|
| [33](https://github.com/EnzoTironi/zoen-native/pull/33) | Merged `551394c1` | Pruning, linking, enrollment, encrypted history | Integrated; native linking and repeatable complete M2 journeys remain gates |
| [34](https://github.com/EnzoTironi/zoen-native/pull/34) | GitHub records merge `2d39ab28` | Backup, authorized recovery enrollment, bounded storage | Missing from examined main tree; reviewed PR 38 restores it. Native recovery and recovery without a surviving authorized MLS peer remain gates |
| [35](https://github.com/EnzoTironi/zoen-native/pull/35) | Merged `7b3c548` | Real WASM sandbox | [Linux CI at that merge passes](https://github.com/EnzoTironi/zoen-native/actions/runs/37948425509) |
| [36](https://github.com/EnzoTironi/zoen-native/pull/36) | Merged `334ea050` | Fly Machines sandbox provider | Integrated; paid staging and production egress proof remain gates |
| [37](https://github.com/EnzoTironi/zoen-native/pull/37) | Merged `17999988` | E2B research | Integrated research; the live computer still needs implementation |
| [38](https://github.com/EnzoTironi/zoen-native/pull/38) | `08124f0` | Chat reading position, unread indicator and restored reviewed backup integration | [Full Linux CI passes](https://github.com/EnzoTironi/zoen-native/actions/runs/37973200217); combined native UI proof with PR 44 remains pending |
| [39](https://github.com/EnzoTironi/zoen-native/pull/39) | Documentation/CI draft | Current roadmap evidence, all seven completion gates and Postgres CI image source | Links, diff and chart checks pass; full Linux CI at `1210b2b` passes with the verified Docker ECR mirror; updated evidence awaits fresh-head checks and integration |
| [40](https://github.com/EnzoTironi/zoen-native/pull/40) | Merged `076d69e3` | Welcome ordering, refill, catch-up and stale-rejection fixes | Integrated; controlled regressions pass; historical stock-7 cause remains unproven and repeatability remains required |
| [41](https://github.com/EnzoTironi/zoen-native/pull/41) | Separate draft | Native Android client | Owned by another active chat; untouched by this audit; owner supplies live sync/media evidence |
| [42](https://github.com/EnzoTironi/zoen-native/pull/42) | `d9ebb3e` | Temporary combined validation PR | Full Linux CI passes; 41 local FFI tests and focused CLI/FFI Clippy pass; unmerged, close after original feature integration |
| [43](https://github.com/EnzoTironi/zoen-native/pull/43) | `eff2eadd`, draft | Renewable persisted bucket ownership, fencing and bounded forwarding | Earlier `3fe1ef5` full CI passes. Five fixes are source-reviewed; owner reports 15 ownership assertion groups passing. Public-ingress deadline and timeout delivery gaps, fresh CI, workload and authenticated TLS proof remain |
| [44](https://github.com/EnzoTironi/zoen-native/pull/44) | `159a2f2` stacked on PR 38 | One Chats inbox, native header/pins/palette, Cards/List notifications and reusable web shell | Functional head `836910c` passes full Linux CI, iOS/macOS builds, 13 web checks and Mac/browser journeys. Latest native wording/catalog checks pass; fresh-head CI and iOS interaction proof remain. Web data remains a local sample preview |
| [45](https://github.com/EnzoTironi/zoen-native/pull/45) | `dd75e8f`, draft, currently stacked on PR 42 | Recovery after every original MLS device is lost | Owner reports 102 layer tests, 21 backup journeys, ten real-store contracts and Clippy passing at `59f14de`. Full CI at `dd75e8f` fails photo, refill and Welcome journeys; five source-review fixes and native proof remain. Retarget to actual main after PR 38 restores backup |

Backup uses ADR 0046 and linking uses ADR 0045. Existing backup migration bytes were
preserved; later migrations extend generations, object versions and package idempotency.
The backup restoration must be verified in the final main tree. Native chat/UI proof and
the remaining full-product gates are separate from the backend merge records.

Peerless recovery review at `dd75e8f` found five required fixes: authenticate external
UpdatePath credentials against the current roster and envelope device; exclude legacy
unsigned pruning headers from recovery authority; preserve a recoverable handshake when
confirmation or marker writes fail; remove device-local recovery markers during erase;
and bound retry state when a recovery reference is superseded. The owner is repairing
these before publishing a new reviewed head. Its [full CI run](https://github.com/EnzoTironi/zoen-native/actions/runs/37990340036)
passes the 21 backup journeys but fails photo blob counting, key-package refill and the
Welcome-ordering fixture. The existing duplication/secrecy, refill and ordering guarantees
remain required. The source-review fixes, causal regressions and fresh full CI precede
integration; encrypted local recovery tests do not complete native recovery or scale proof.

### Verified building blocks and unresolved failures

On `d9ebb3e`, **all 41 local FFI tests pass**, as do focused CLI and FFI Clippy checks.
Controlled regressions verify three client behaviors:

| Behavior | Old behavior | Fixed behavior |
|---|---|---|
| Initial catch-up before offline writes | Real-session test fails because a write overtakes catch-up | Queued plaintext and MLS maintenance wait for the first `SyncDone`; messages use the caught-up epoch and remain readable |
| Late stale rejection after incremental catch-up | Compiled socket regression stalls after 12.57 seconds | Rejection uses the epoch actually sent; exact newer ciphertext is retained, retried and read by the peer; a genuine current-epoch block remains intact |
| Slow maintenance and socket progress | Old interval fails the controlled notice test in 4 of 5 runs | Maintenance paced after each pass passes 5 of 5 runs |

The earlier `c7b1378` combined source has [passing full Linux CI](https://github.com/EnzoTironi/zoen-native/actions/runs/37953112924).
Its verification round also passed 85 local core tests (FFI 37, log 22, MLS 9, protocol 17),
19 backup journeys, 4 CLI revocation journeys, 1 connection journey, 3 key-package delivery
journeys, 3 wire journeys, 10 FoundationDB tests and 7 Postgres/socket authorization tests.
The newer combined `d9ebb3e` now also passes full Linux CI. The local results above remain
bound to their original verification rounds; CI does not replace release/native proof.

**The earlier local M2 round passed 8 of 10 journeys.** The offline-message journey left
a `ProfileKeyShared` acknowledgement pending after the user message reached recipient
history; the refill journey timed out earlier while adding Bruno to group 42. These local
failures remain part of the evidence even though that combined head passed Linux CI.
The new complete suites must establish repeatable recovery, delivery and refill.

[PR 40 CI at `fb5f822` failed live refill](https://github.com/EnzoTironi/zoen-native/actions/runs/37950442153):
stock 7, one Low notice enqueued, no client receipt, 15 pending and no rate limit.
The new controlled regressions prove specific ordering and scheduler hazards; they do
not establish the cause of that historical stock-7 failure. The reviewed backend PRs
are merged and combined Linux CI passes; repeatable release journeys, native proof and
the current UI verification remain required.

The candidate includes Welcome-before-message ordering, attaching a connecting device
before its stock query, notices ordered after successful package publication, and
idempotent package publication. The live-refill journey still requires exactly 32 packages
within its original 20-second polling window. Its stock-7 failure has not been waived.

### Device and recovery boundaries

[PR 33](https://github.com/EnzoTironi/zoen-native/pull/33) now requires explicit enrollment
for a new device on an existing account. Linking verifies the root certificate while an
active sponsor authorizes enrollment and box delivery in one Postgres transaction.
Protocol 4 refuses older clients before authentication so a version-3 sponsor cannot
report a successful link that leaves its target unenrolled. The upgrade preserves stored
history, local event metadata and the existing login signing domain; it does not require
wiping the database. See [ADR 0045](../adr/0045-linking-devices.md).

Revocation rejects fresh requests and new deliveries for the revoked key. Work admitted
before the revocation commit can finish, including an already queued FoundationDB write;
Postgres revocation and the log append are separate transaction domains. The copied root
secret alone can no longer enroll another key at this relay. Valid backup recovery
authority can still authorize a fresh key, and keys enrolled before revocation stay
active until each is revoked. Those authorities need their own lifecycle. The durable
authorization checks and bounded Postgres pools also add costs that must be measured
under reconnect storms and sustained delivery before production-cell readiness.

Sealed v4 entries bind the surviving pruned header and use a separate signature domain
to prevent replay as a legacy stub. Full legacy v3 entries retain their authenticated
bytes and stay unpruned. Previously persisted legacy stubs authenticate their original
opaque digest; an upgrade cannot reconstruct trust in headers whose signed bytes were
already discarded. Existing chains and outboxes are preserved, rather than reset.

[PR 34](https://github.com/EnzoTironi/zoen-native/pull/34) adds generation-bound recovery
enrollment with a root certificate, proof from the new device and valid backup authority
in one transaction. Restore installs that same enrolled key and retains durable MLS join
requests; a surviving authorized peer must still add the leaf. Immutable object keys on
every upload attempt and an atomic pointer change preserve the previous backup when a
replacement fails. Database locks, object operations and full requests have bounded
waits; uncertain or failed cleanup still needs an operations-tested garbage collector.
See [ADR 0046](https://github.com/EnzoTironi/zoen-native/blob/08124f05bb4fbd6632df5f59eabc284831e36d6e/docs/adr/0046-encrypted-backup.md).

**Production password recovery remains default off.** A vault key alone does not enable
it. The explicit development opt-in is for tests and development only. Public activation
requires independent, identity-bound recovery authorization before OPRF evaluation or
destructive counters, plus a real HSM/enclave implementation with proven key destruction
and counters across nodes. Removing a sealed key from Postgres does not destroy old
copies. Recovery-key mode remains available; native linking/recovery UI and recovery
without a surviving authorized MLS peer are still unfinished.

### Native and parallel work

Earlier [PR 38 native recordings](https://github.com/EnzoTironi/zoen-native/pull/38#issuecomment-6080457241)
cover three local timeline stories at `9ab80ca`. They were attached with `gh --attach`.
Five journeys now cover the expanded behavior. [Linux CI at `c836d5d3` passes](https://github.com/EnzoTironi/zoen-native/actions/runs/37963699446),
but the fresh native trial hung in the first fixture and was interrupted; it is not a
five-journey pass. Main-thread samples repeatedly passed through an invisible SwiftUI
native search controller. Deferring inactive Search initialization subsequently passed a
controlled AtTheEnd journey. The broader fresh native trial still recorded render/query
stalls and failed scroll movement, and UI tooling detected the Mac locked. A temporary
Activity rendering treatment was restored; the current source retains only the reviewed
Search gate. Unlock and a rebuilt baseline are required before further diagnosis. Old
recordings do not validate the changed source.
Simulator timeline stories also do not measure live relay delivery or production capacity.

The Grok computer also holds an unpublished `ux/audit-p2` patch across 14 Swift files.
The original patch remains saved and unimported; it still needs review, simulator
validation and a PR. The conversation confirms that some native flows are designed UI
awaiting their backing implementation, including the displayed agent browser. Track
these flows against their product completion gates.

[PR 41](https://github.com/EnzoTironi/zoen-native/pull/41) is the separate native Android
workstream. Its active chat owns validation and integration. A client implementation or
local planner does not by itself complete live cross-device delivery, production media
or the durable agent runtime.

Current UI work removes the global desktop top bar, shares the mobile floating chat
header, fixes the pin strip outside the timeline on a fully transparent background, and
extends content to the window top. The whole Mac app uses the iOS palette; the React web
shell uses the same palettes and side navigation. These changes are still undergoing
verification and need fresh visual evidence at their final source head. The independent
web shell/demo still has no authenticated web client. Live encrypted web sync,
onboarding, permissions and supported-version journeys remain part of gate 4.
