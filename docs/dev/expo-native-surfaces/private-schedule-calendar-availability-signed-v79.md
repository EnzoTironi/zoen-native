[Native surface index](../expo-native-surfaces.md).

## Private Schedule calendar availability — signed v79

V79 source `03dd599abd2f74a1f031f5312e033fc6de6403c4` adds an explicit EN/PT-BR
**Check my availability** action. Results stay in memory for the selected day/timezone;
no automatic read or shared Schedule write occurs. Full EventKit access is the OS
permission; the adapter gives the miniapp only busy intervals. Names/details are not
shown in its result UI; this is source/privacy evidence, not intercepted RPC proof.

On isolated QA, per-item **Allow once** precedes actual OS **Deny**, yielding a localized
error without a filter. Only Zoen calendar permission is reset as setup; after restart,
actual **Allow Full Access** marks the owned Monday 11:00–12:00 fixture at 11:00/11:30.
A busy tap cannot select; 09:00 selects without submitting a booking. Removing the check
restores slots and keeps that valid pick. Tuesday clears it and remains idle until an
explicit new query finds no conflicts with business times, not an entirely free day.
The synthetic calendar and catalog-created Schedule remain for reproduction.

Both public executable/JavaScript hashes and strict signatures pass before/after their
preserving v78 → v79 installs. Isolated Read history/old 0:01 Sent voice and the new
pinned/inline Schedule remain. Human Home 4 cards / recipe 6/8, existing BRL plan v3 /
R$123,45, closed poll v3 / 1 versus 0, six-page **chat history** at 100%, old 0:11
Delivered voice and empty larger-text composer/closed keyboard remain unchanged;
no human calendar reads, permission requests or content/account/preference mutations.

App 612/612 plus TypeScript/Oxlint/Oxfmt, Schedule/real production HTML 12/12,
adapter 20/20 (a subset of 612) and scoped Rust Schedule 8/8 / final catalog 2/2 pass.
Submit/generation mutants fail; an actual production-bundle negative catches duplicate
React. That initial candidate was superseded before iOS Release. Prior full Rust
444/444 keeps v74 attribution. Exact-source Expo `schedule-miniapp`, `client` and
`release-web-e2e` jobs pass separately; they do not establish manual native acceptance.

Native all-day/DST/no-calendar and fault integration remain source-only, with no native
booking/calendar.add, Push/providers/physical-device or full-parity claim. Existing
nested miniapp scrolling and low-legibility transparent consent need measured UI work.
The 25-step native and 8-step human recordings retain capture/conversion/wait warnings
as retired one-use diagnostics, not formal replay/E2E. Six scoped tooling services stop;
relay PID 19737 and other accounts remain unchanged.

Evidence: `/private/tmp/zoen-calendar-availability-qa-v79/runtime.json` and
`/private/tmp/zoen-iphone-release-v79-verification.json`; source receipts are copied into
the same QA root. Its verifier copy retains the immutable pre-QA build attribution.
Earlier accessibility, media, peer, draft and v76 offline proofs remain artifact-specific.
