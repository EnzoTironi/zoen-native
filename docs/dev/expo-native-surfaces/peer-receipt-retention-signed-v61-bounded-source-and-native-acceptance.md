[Native surface index](../expo-native-surfaces.md).

## Peer receipt retention — signed v61, bounded source and native acceptance

Source `d722963055a324ce0bc2c18571d1d38d66804ed3` persists confirmed peer
Delivered/Read watermarks before exposing higher ticks and restores them before
initial snapshots. The device cache preserves exact unsigned 64-bit sequences
and independent monotonic maxima per conversation and peer. Stale/duplicate marks
do not write. Only confirmed membership admits marks; optimistic membership does
not authorize them. Storage failure keeps the prior published state and follows
the existing reconnect/catch-up path. Pending/failed precedence and the existing
all-current-members receipt aggregation policy are unchanged.

A bounded socket-local buffer retains legitimate early receipts until canonical
membership arrives. Canonical events, SyncDone and authenticated linked-device
Page-import recovery flush newly admissible marks. Unscoped partial SyncDone does
not discard another conversation’s waiting marks; overflow retries the connection.
A genuine Page-import/approval regression proves that recovery can release a
waiting receipt without another event. Own-identity read propagation is retained.

The final positive source passes all 414 FFI tests in 132.00 seconds, all 15 Store
tests and 428 application tests with TypeScript, Oxlint and Oxfmt. Eighteen focused
cases comprise 13 receipt/recovery, two Store and three existing demo/read-replay/
group regressions. Relevant strict Rust Clippy passes. Removing only cold
restoration reproduces Delivered → Sent; removing only the recovery flush accepts
the genuine Page import but leaves no receipt update. Both negative controls fail
as intended, followed by exact positive source restoration. Source logs:
`/private/tmp/zoen-peer-receipts-v61/freeze.json`,
`/private/tmp/zoen-v61-ffi-full-final-tests.log`,
`/private/tmp/zoen-v61-store-full-tests.log` and
`/private/tmp/zoen-v61-app-check.log`. Offline cold-open text and voice proof belongs
to these source tests; it is separate from the native healthy-relay journey below.

The relay returns typed Unavailable when fetching durable receipt rows fails or
exceeds five seconds, before sending events or SyncDone. All 11 tests in the
affected authenticated real-WebSocket/Postgres suite and relevant strict Clippy
pass. Actual SQL failure, a locked SELECT deadline, a Ping/Pong no-SyncDone barrier,
healthy empty-room success and exact Delivered/Read replay after reconnect are
covered. Restoring the old swallowed query error fails the negative control.
Evidence: `/private/tmp/zoen-relay-receipts-v61/checks.json` and
`full-session-revocation.log` in that directory. This uses a separate owned
Postgres instance, stopped with its data preserved. The running application relay
is unchanged; no new warm/post-Sync client retry policy is claimed.

Normal device SQLite migration creates the derived cache; local erasure clears
it. Backups intentionally exclude it, so new/restored devices hydrate receipts
from the relay. A narrow transactional migration retains the old demo receipt
blob through canonical admission. Prelaunch NativeImage validation remains strict:
retained capsules with the pre-v61 SQLite schema fail closed as Unsupported. No
broad capsule migration, weakened validator, reset or deletion of existing user
data occurs. No protocol, DTO, public UniFFI API, Swift source or JavaScript source
change is introduced; the native Rust Release artifact is regenerated.

The signed ARM64 simulator Release has 772 frozen inputs, source SHA256
`601337666d7db0db320ad84159a14545f32b79e56f4d812dbe32530ab256a5ab`,
executable SHA256 `8925c83bb15d94ae66f278bf44b1372d560f55c1c7892bf9b86288ebbfbb6381`
and app-tree SHA256 `8cb50433a7e092c74f3b8360a91e6977919bb741c99b922233fd733abeae5a2f`.
The unchanged 6,531,413-byte JavaScript bundle has SHA256
`53110a02c3ec3bea963f8ed8712c3892136a39ed0363e5de2b90aabdf582aaa5`.
Exact preserving installs, executable/JavaScript and strict signatures pass before
and after UI on both QA simulators. The frozen manifest is verified after both
runtimes; the archive’s strict signature is verified. Build receipt:
`/private/tmp/zoen-iphone-release-v61-verification.json`.

On isolated QA, a fresh disposable public-API peer creates the exact two-person
E2E group `sp_01m56wgvght8v7hgkcb2y562gj` and one seed message. One native composer
send of `QA v61: native receipt persistence.` creates
`01M56WJH9028HHKB7H0BC08XR1`, canonical sequence 9. The exact own bubble shows
Entregue a todos, then Lida por todos after the peer’s explicit read. The peer
stops successfully and discards its volatile keys before the full native restart.
Home → Chats → the same group retains both messages and the exact own Read bubble.
The healthy application relay remains available; this is neither native offline
nor newly sent voice cold proof. Final composer is empty, keyboard closed and
audio idle. There is no attachment import, capture, reaction, restoration or
existing native account/preference mutation. Evidence:
`/private/tmp/zoen-peer-receipts-qa/v61/runtime.json`. The 21-step diagnostic has four
cross-tree warnings and zero unmet waits. Its one-use external-peer-assisted
export is retired, without formal replay.

The historical v60 immediate/approximately-30-second Delivered → Sent observation
above remains exact. Read-only v60 preflight at 2026-10-18T06:47Z and the fresh
immediate pre-install check subsequently show existing Bilingual text and voice
Entregue a todos again. This establishes later relay recovery, not permanent loss.
The fresh human preflight is idle with an empty composer and closed keyboard.
After the preserving v61 install, Home retains four miniapps and recipe 6/8;
Chats → Bilingual retains history, carousel, the existing idle voice and prior own
text/voice Delivered. A second full restart through Home and Chats retains the
same Delivered states. The native attachment dropdown opens all four actions and
dismisses without selection. There is no typing, send, import, capture, reaction,
restoration, preference or account mutation. Final composer is empty with keyboard
closed and audio idle. Evidence:
`/private/tmp/zoen-peer-receipts-qa/v61/human-runtime.json`. Its 18-step diagnostic has
two cross-tree warnings and zero unmet waits. Raw menu opening and the incorrectly
serialized underlying-message dismissal target make its export unsafe for formal
replay; the actual tap dismisses the visible menu without a reaction. The export
is retired. Native tool services stop only for the two used simulators.

The 31-file evidence inventory is
`/private/tmp/zoen-peer-receipts-qa/v61/evidence-manifest.json`. Historical v60
artifact objects are retained exactly under `previousControlledV60IphoneArtifact`
and `previousHumanV60IphoneArtifact`; dedicated PDF close/Markup and older evidence
remain unchanged. Old volatile watermarks cannot be invented by the new cache.
Independent edited-PDF byte/recipient proof, induced native failure journeys,
physical-device behavior, providers/secure new-device recovery, broad accessibility/
performance and full comparative parity remain pending. Web and Android runtime
acceptance are deferred; PR 136 remains open and draft.
