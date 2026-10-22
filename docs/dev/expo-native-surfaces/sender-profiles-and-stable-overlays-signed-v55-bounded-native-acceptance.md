[Native surface index](../expo-native-surfaces.md).

## Sender profiles and stable overlays — signed v55, bounded native acceptance

Source `c6f40aa0` makes incoming avatars and group sender names separate profile targets.
Each tap rereads the canonical message and opens its current human or agent person profile,
with duplicate, focus, account/device/mode and membership guards. Direct threads omit
redundant sender names. Signed v54 on isolated QA accepted all four selected name/avatar
paths, group/direct thread labels, reply/thread gestures, incoming JPEG QuickLook, two-second
WAV completion, and profile-return draft/scroll retention. These observations retain their
v54 attribution: `/private/tmp/zoen-author-profile-qa/v54/native-observations.json`.
V54 was not delivered to the human simulator.

That run exposed two defects corrected in `3771804a`. Default FlashList identity anchoring
moved the inbox header when activity reordered conversations; Chats now disables it while
message history keeps its own anchoring. Popovers previously measured the lifted bubble
before the keyboard closed. They now await native dismissal and measure the current row;
recycle, unmount, stale access, new typing and superseding actions cancel presentation.
Incoming media metadata also follows card width.

An unavailable initial catch-up could leave an authenticated socket open without SyncDone.
The client now reconnects for that pre-catch-up error and resets retry backoff only after
SyncDone. Post-catch-up errors, terminal handshake refusals and bounded rate hints retain
their prior behavior. No protocol, public API or generated binding changed. The source
passes 415 app tests, including 67 actual compiled-hook cases, TypeScript/Oxlint/Oxfmt,
394 FFI tests including all 18 network cases, and strict client-core all-target Clippy.
Removing either the keyboard-dismiss await or the catch-up recovery arm fails its negative
control. The 26 MLS, 11 shared-type and 56 browser-core results remain attributed to v53.

Signed simulator Release v55 has 766 frozen inputs, source SHA256
`0c4bd222511fd2dbcef9ebaca1e42ee3e25d2cf209dd9f9720cdaf30ab070f95`.
Preserving install and exact installed executable/JavaScript/strict signature passed on
both QA simulators before and after UI; the frozen source manifest remained stable. Actual isolated checks accepted:

- Cold Home retained three pinned cards plus the QA countdown. Incoming timestamps align
  to media width, and avatars meet bubble bottoms.
- Human-avatar profile entry and Conversar created a native direct conversation. Exactly
  one synthetic `QA v55 inbox reorder.` message was sent and delivered to all; the composer
  emptied. Tapping that bubble with the keyboard open closed the keyboard and presented
  quick reactions immediately above it at y0.736, correcting v54's displaced y0.485.
- Returning to Chats after the new direct row appeared retained heading y0.093 and cards
  y0.162. Agent-name profile entry was canonical; the incoming WAV completed `0:02 / 0:02`.
  JPEG QuickLook opened and closed after a normal image tap exposed its hidden controls.
- Cold Home/Chats/group retained four incoming texts, one photo and one voice, latest
  history at 100%, an empty composer and the new direct conversation row.

Human E2B's preserving update and normal restart retained four Home cards and Recipe's 6/8.
Bilingual QA retained latest history at 100%, existing audio/delivered receipt bottom0.854
above inputtop0.897 (about36pt), and an empty composer. No typing, content, preference,
send or account mutations were performed there. Evidence is
`/private/tmp/zoen-author-profile-qa/v55/runtime.json`; the build receipt is
`/private/tmp/zoen-iphone-release-v55-verification.json`.

The isolated 28-step diagnostic contains three cross-tree warnings and one failed wait
for a hidden QuickLook Close control. Tapping the image exposed the native filename and
Close, which then worked. The human four-step diagnostic has one cross-tree warning.
These are bounded observations, not formal portable replays. Original diagnostic YAML is
retained; only the two task-owned unsafe executable flows were retired. Both scoped Argent
device services stopped. The fixed-scope helper exited successfully and discarded volatile
state; the healthy local relay and FoundationDB were retained.

Fixture diagnostics also retain their limits. Compiler-cache cleanup recovered local
FoundationDB's disk reserve; only the relay restarted with the same configuration, without
database/account reset. An overstrict full-name assertion on the agent side returned a
false negative because human profile keys were intentionally not shared with that agent;
four canonical incoming messages reached the native account and latched accepted IDs
prevented duplicates. This is not a clean helper-pass claim.

Native video/document forwarding, induced races, multi-person reactions, supported-iPhone
Speech, physical/provider entry, broad accessibility/performance and comparative parity
remain pending. Historical remote head `24cc5a99791f00ca72cfb5d1b1f9363af7b9ebed` is not
green: [Android28](https://github.com/EnzoTironi/zoen-native/actions/runs/39094801446/job/117578937321)
timed out at `PageDraftRecoveryJourneyTest.kt:104` waiting for resumed editing after draft
discard to advance the page version. Android35 passed; no retry establishes a flake.
That CI does not certify v55, and other platforms remain deferred.
