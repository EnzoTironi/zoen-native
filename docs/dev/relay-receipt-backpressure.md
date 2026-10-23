# Receipt backlog and overload

The relay retains at most 65,536 pending `(Space, identity)` receipt rows and
8 MiB of logical key bytes. Bytes count the Space and identity for every row,
including repeated Space names. This bounds retained keys and row count, not
allocator overhead or total process RSS. A serialized flush can hold one equally
bounded snapshot; SQL writes use the existing 2,000-row chunks.

A receipt for an existing pending row can still raise its unsigned delivered/read
watermark when the backlog is full. New rows that exceed either bound are refused
without evicting any admitted mark. First zero marks still create rows. The normal
Session returns `Unavailable`, then closes before forwarding a refused receipt.
The content-free `zoen_relay_receipts_overloaded_total` counter records this case.
If the connection cannot deliver that error, closure remains the recovery signal.

A flush leaves pending rows visible until the actual SQL statement succeeds. It
removes a row only if its current mark still equals the persisted snapshot. A
higher mark received during the write remains pending for the next flush. Failed
chunks and uncertain results retain their marks; repeating PostgreSQL's monotonic
upsert cannot lower stored values. Two flush calls serialize on the existing
flusher mutex.

After authenticated catch-up, the native Client rebuilds delivered/read receipts
from its own history at `SyncDone`, even when no new remote event arrived. Thus a
refused receipt is replayable after reconnect and restored capacity. Membership,
certified-device admission, current delivery authorization and receipt audiences
are unchanged. No receipt acknowledgment, new authority or durable spill store is
introduced. Pending memory remains process-local; this change does not promise
persistence across relay process loss while PostgreSQL is unavailable.

## Verification

With `ZOEN_TEST_PG` pointing only to a disposable PostgreSQL, the affected tests are:

```sh
cargo test --locked -p zoen-relay --lib receipts::tests:: -- --test-threads=1
cargo test --locked -p zoen-relay --test receipt_watermarks --test receipt_pressure --test session_revocation -- --test-threads=1
```

`receipt_pressure` uses real authenticated WebSockets, normal SQL registration and
real SQL failure/recovery. Its explicit synthetic LogStore roster isolates the
membership seam; it is not a joined native Client/FoundationDB acceptance test.
Existing native receipt-replay behavior is reused, rather than implemented by the
fixture.

`receipt_outage` is ignored by default. Its opt-in controller must own the selected
PostgreSQL process and provide a fresh private `ZOEN_TEST_PG_OUTAGE_DIR`. The test
writes `request-stop`; the controller stops its own PostgreSQL and writes `stopped`.
For `request-start`, it restarts that same data directory and writes `restarted`.
The host endpoint must stay identical across restart. The test separately waits
for an actual SQL connection through that endpoint, then checks a single recovery
flush. It cannot select or stop a service itself. Never point the controller at an
existing development/production datastore.

These checks cover bounded admission, unsigned high-watermarks, partial-chunk
failure, actual blocked writes, concurrent flushes, SQL outage/recovery, refusal
before fanout, reconnect and receipt replay. Issue [#110](https://github.com/EnzoTironi/zoen-native/issues/110)
remains open for fanout byte budgets, cross-node overflow recovery and measured
membership transaction reduction.
