[Native surface index](../expo-native-surfaces.md).

## Unread snapshot refresh — signed v64, bounded native acceptance

Signed v64 source `8dd0b6ca` changes the chat's mark-read call to
`ClientStore.request`, so a successful mutation also refreshes the canonical
snapshot used by Chats. Native geometry and the Rust core remain unchanged from
v63. The application check passes all 440 tests, TypeScript, Oxlint and Oxfmt.
An external negative control removing the actual Store's post-mutation refresh
fails with unread 1 instead of 0; it establishes the Store contract rather than
a compiled-route proof.

The existing `History download QA1018v63c` fixture starts at unread 1 after a cold
v64 launch. Its synthetic peer is already stopped with volatile keys discarded;
Files-only v63 preview had never entered this chat. One ordinary tap opens the
correct native conversation. Its canonical file and version event reach 100%
above the composer, with empty input, closed keyboard and idle voice. Back once
returns to the same visible Chats row without unread, and the previously visible
unread-1 label is hidden. There is no restart between the before and after checks.
This resolves the bounded stale-snapshot observation recorded in v63. The only
mutation is the normal mark-read operation; no new content, account or preference
change is performed.

The signed Release has 774 inputs, manifest SHA256
`c73228c0ea01388179fd4aa0119ea9d8b1231c75c1f39ce403330757a51a63e2`.
The source manifest is verified after build and after all runtime checks; installed
executable/JavaScript and strict signature are verified before and after both
isolated and human journeys. Both QA simulators now run the exact signed v64
archive. V63 first-file and uncached-history proofs retain their own
signed-artifact attribution.

The human simulator receives a preserving install without uninstall, reset or
erasure. Its first ordinary v64 cold launch retains four Home cards, recipe 6/8
and the existing Bilingual conversation. The native recipe/Zo/trip rail and
timeline at 100% remain; existing own text/events and the last voice retain
Delivered. The composer stays empty, keyboard closed and audio idle. No typing,
send, import, recording, playback, reaction, restoration, account or preference
change is performed. Its nine-step diagnostic has two cross-tree warnings and
zero unmet waits; the original is retained and the export retired. This is not
a second-cold, offline, physical-device or provider acceptance.

The 14-step diagnostic has two cross-tree warnings and zero unmet waits. Its
visible-row/hidden-old-label pair bounds the observation, but the raw row target,
heading-tree mismatch and consumed one-use fixture make portable replay unsafe.
The original diagnostic is retained and the export is retired; no formal replay
is claimed. Native storage failure/account replacement, physical iPhone,
providers, broad accessibility/performance and full comparative parity remain
pending.

Evidence: `/private/tmp/zoen-unread-qa/v64/runtime.json`,
`/private/tmp/zoen-unread-qa/v64/human-runtime.json`, its 21-file evidence manifest,
`/private/tmp/zoen-iphone-release-v64-verification.json`,
`/private/tmp/zoen-v64-unread-app-check.log` and
`/private/tmp/zoen-unread-v64-proposal/freeze.json`.
