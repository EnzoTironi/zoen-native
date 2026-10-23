# Page export completion

`zoen page export-all CHAT --dir DIR` exports the pages in this device's
projection after online catch-up has completed. A connection being online is
insufficient: startup can return from `wait_until_idle` at the existing command
deadline while still catching up. Pages whose creation events have not arrived
are absent from `items()` and cannot be counted as "still arriving".

The export command checks the existing `wait_until_settled` contract before
enumerating or writing files. It consumes only time remaining on the original
startup-wait deadline. Startup can finish early, then a new MLS notice can require
work before export begins. A zero-budget second check incorrectly refused that
work immediately even when the command still had time available.

The deadline is captured once. An expired deadline, unfinished sync or ACK
observed beyond that deadline returns a nonzero status before any export file is
written. The command never restarts its 8-second default or explicit timeout.
General startup's first wait argument, publication/retry policies, per-page
content waits and sync state are unchanged. All existing profile, MLS and outbox
conditions remain required; there is no new completion authority. The Core's
strict wait uses the actual final observation that ended its wait, rather than
rechecking mutable MLS state after that wait has already completed.

`zoen --offline page export-all CHAT --dir DIR` deliberately exports the known
local cache without connecting. It prints a local-only notice on stderr while
preserving the existing page/byte/missing summary on stdout. This may be a subset
of remote history. Known pages whose contents remain unavailable still contribute
to the existing "still arriving" count.

This is a caught-up observation before enumeration, not an atomic projection
snapshot or globally atomic backup against future concurrent writes. Events can
still arrive while files are written. The separate account `backup` commands are
unchanged.

`crates/zoen-cli/tests/export_all_admission.rs` uses a real Client, a fresh
synthetic certified account and a signature-checked loopback WS fixture. It holds
a genuine signed second Page event and `SyncDone`: the original CLI successfully
printed one page and zero pending pages. The fixed command refuses before any
file write. Releasing the event and `SyncDone` exports both original Markdown
files byte-for-byte. An explicit offline control exports only the cached page
without a connection. This causal case does not establish the cause of PR222's
original Linux CI transfer failure, which remains unreproduced.

`crates/zoen-cli/src/commands/export_admission/tests.rs` exercises the real export
branch with a real Client and certified loopback WS. After initial readiness, a
low-stock notice causes a real key-package publication whose ACK is held. The
legacy zero-budget gate fails immediately; using the remaining original budget
waits for the ACK and exports the exact UTF-8 page. Late ACK, missing ACK and
expired-budget controls write no files; offline export remains a separate cache
operation. Existing durable MLS recovery/restart and pending-retry regressions
remain required. The normal device profile and selected device-plus-portable
profile are distinct qualification scopes, not a full workspace CI result.

PR254's Linux failures showed only the final zero-budget observation with
`mls_ready=false`, after the startup wait had observed completion. They did not
identify which MLS subcondition changed. The controlled top-up reproduces one
causal mechanism; it does not prove that the historical CI received that notice.
