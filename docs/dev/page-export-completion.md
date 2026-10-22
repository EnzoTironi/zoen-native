# Page export completion

`zoen page export-all CHAT --dir DIR` exports the pages in this device's
projection after online catch-up has completed. A connection being online is
insufficient: startup can return from `wait_until_idle` at the existing command
deadline while still catching up. Pages whose creation events have not arrived
are absent from `items()` and cannot be counted as "still arriving".

The export command checks the existing `wait_until_settled` contract with a zero
additional wait before enumerating or writing files. An unfinished online sync
returns a nonzero status and writes no export files. General startup, the
command's timeout, retry policy and sync state are unchanged. This check includes
the existing profile, MLS and outbox conditions; it introduces no new completion
authority.

`zoen --offline page export-all CHAT --dir DIR` deliberately exports the known
local cache without connecting. It prints a local-only notice on stderr while
preserving the existing page/byte/missing summary on stdout. This may be a subset
of remote history. Known pages whose contents remain unavailable still contribute
to the existing "still arriving" count.

This is an observed caught-up snapshot, not a globally atomic backup against
future concurrent writes. The separate account `backup` commands are unchanged.

`crates/zoen-cli/tests/export_all_admission.rs` uses a real Client, a fresh
synthetic certified account and a signature-checked loopback WS fixture. It holds
a genuine signed second Page event and `SyncDone`: the original CLI successfully
printed one page and zero pending pages. The fixed command refuses before any
file write. Releasing the event and `SyncDone` exports both original Markdown
files byte-for-byte. An explicit offline control exports only the cached page
without a connection. This causal case does not establish the cause of PR222's
original Linux CI transfer failure, which remains unreproduced.
