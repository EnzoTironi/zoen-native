# Network teardown and immediate store reopen

`stop_sync` releases the network's Engine ownership before returning. After the last Core owner is destroyed, the same SQLite store can reopen immediately. The release does not depend on network timeouts or Tokio task destruction.

## Why background runtime shutdown was insufficient

The dispatcher and media worker previously held their own `Arc<Mutex<Engine>>`. `Runtime::shutdown_background` canceled their work but returned before their futures dropped those owners. Core destruction could therefore return while SQLite still held its exclusive lock. A weak strong count reaching zero also precedes completion of the last value's destructor, so weak observation cannot prove SQLite closure.

The F6 regression repeats 32 real stores. Each sample first reopens without sync, then creates an account, starts sync without a listener, stops twice, destroys Core, and immediately reopens. The original production source failed 31 of 32 sync samples with `database is locked`. All 32 no-sync controls opened. No retry or delay participates in this test.

## One revocable owner for every network task

`Net` stores an `EngineLease`. The dispatcher and media worker clone that lease instead of cloning the Engine owner. The lease contains `Mutex<Option<Arc<Mutex<Engine>>>>`.

`EngineLease::with` holds the lease mutex and borrows Engine for a synchronous closure. Its closure returns owned values. Engine references and mutex guards never leave the closure. Listener calls and network awaits run after the closure returns. Network awaits retain payloads such as envelopes, ciphertext, and signer copies, which do not own SQLite.

`Net::drop` requests Stop, begins background runtime shutdown, and calls `EngineLease::revoke`. Revocation takes the optional Engine owner under the lease mutex and explicitly drops that owner before returning. The mutex acquisition proves all earlier network Engine closures finished. The empty Option prevents later closures from acquiring Engine. The explicit drop completes release of the network owner even if canceled futures remain alive.

A revoked session exits as Stop. A revoked media worker returns. A revoked flush returns `offline`, and an Engine-dependent connection notification is suppressed. Revocation never substitutes a successful mutation or fabricated payload. Both mutexes recover poisoned contents so a panic cannot silently retain the network owner.

## Callbacks and blocking behavior

Listeners remain synchronous and retain their existing order. No listener invocation holds the lease mutex, Engine mutex, or a status watch borrow. A listener can call `stop_sync` on the network runtime without waiting for its own future or joining its runtime.

`stop_sync` removes Net while holding Core's network mutex, then releases that mutex. It clears poke under the Engine mutex and releases that mutex before dropping Net. Lease access locks the lease before Engine. Teardown does not acquire Core's network mutex.

Teardown may wait for synchronous SQL or MLS work already in progress, including another API caller holding Engine. It does not wait for socket connection, socket reads or writes, an HTTP response, or a listener callback. No constant latency guarantee is implied. Runtime resource cleanup and already executing callbacks may finish in the background. A callback or API caller retaining a legitimate Core owner still prevents final Engine destruction until that owner is released.

Durable outbox, pending records, ciphertext uploads, and checkpoints remain untouched by revocation. Startup still calls `mls_claims_disconnected` because cancellation may bypass normal session cleanup. The existing HTTP timeouts, reconnect policy, and publish window remain unchanged.

## Regression coverage

The Core regressions cover repeated explicit stop, destruction without stop, no listener, an unrelated Tokio runtime, and a real synchronous network callback that stops and destroys its final Core owner. Immediate reopen is followed by a SQLite write or read. There is no weak-count synchronization, sleep, storage retry, or larger timeout.

The lease regressions hold a real Engine closure under channel control and verify revocation cannot complete while its mutex is held. A controlled panic poisons both mutexes before a successful revocation and immediate reopen.

The media regression creates a genuine account and encrypted attachment. Its local relay verifies the device's authentication signature, reaches Ready, and accepts the media HTTP PUT without answering it. Net destruction then immediately reopens the real store and checks that the queued ciphertext remains available. This covers the second Engine ownership path during a blocked HTTP operation.

These Rust tests establish native ownership behavior. Android recovery and fixture validation remain separate device evidence before integration.
