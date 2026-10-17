# Initial history after device linking

A linked account and a completed history transfer are separate durable states. A new
device writes `link.initial_history_pending` before publishing its linked account.
Closing the app, entering the app before history arrives, an offline source device,
or a transfer timeout cannot turn that state into an empty completed history.

`history_status` reads the native device database and active receiver:

```ts
{
  initial_pending: boolean,
  manifest_received: boolean,
  chunks_received: number,
  chunks_total: number,
  receive_active: boolean
}
```

The absence of conversations is not a completion signal. Before the authenticated
manifest arrives, both chunk counters are zero while `initial_pending` remains true.
`receive_history` reuses the retained manifest and verified encrypted chunks after a
restart. Its timeout covers the entire operation, with a maximum of 120 seconds per
attempt. Manifests, chunks, assembled bytes and event counts have explicit bounds.

After download, the receiver waits for relay synchronization. Every history event must
match an existing authenticated relay position; missing records, duplicate positions,
wrong inner signatures and mismatched opened records fail without importing a partial
batch. Importing the opened records, recording a completion receipt, removing the
pending marker and cleaning the local transfer journal share one SQLite transaction.
Relay deletion happens afterward as best-effort cleanup. Replaying a completed receive
returns the durable receipt, including after a process restart, without consuming a
second mailbox or creating a false zero-message result.

Native receiver cancellation uses an explicit Rust watch channel and a single-receiver
guard. `stop_sync` cancels waits and downloads; a cancelled or timed-out attempt retains
the pending marker and resumable journal. Swift Task cancellation by itself is not the
transfer's cancellation authority.

After the source receives the relay's manifest acknowledgement, it sends an encrypted
`DeviceMsg::HistoryReady` hint on the existing linked-device channel. Native receivers
accept it only for their device, from a known authenticated primary peer that is not
revoked, while the durable pending marker exists. The hint carries no content or
completion assertion. Dropping it does not discard the pending transfer.

The iOS session patch adds a bounded automatic pump after open/link, on foreground,
on a transition to an online synchronized connection, and on the authenticated hint.
It serializes receivers and preserves account/generation guards. External wakes racing
the end of a pump are coalesced into a later bounded attempt rather than dropped. The
Swift integration is applied against the regenerated ABI; syntax/source lifecycle
checks are separate from the native runtime journey, which remains pending on the
relinked binary.

Five Rust regression tests passed under `link_api::initial_history_tests`: link then
cold reopen before history with identical vault keys; refusal of missing/forged history
without partial import; durable import/receipt across reopen before relay cleanup/UI
acknowledgement; cancellation/deadline/single-receiver behavior; and authenticated,
recipient-bound, primary-only, pending-only HistoryReady handling. These are real Rust
and SQLite/HPKE tests. They do not establish a completed UI pairing or relay transfer
between running apps.
