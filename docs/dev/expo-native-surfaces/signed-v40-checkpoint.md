[Native surface index](../expo-native-surfaces.md).

## Signed v40 checkpoint

Home favorites now support a native iOS organizer with drag handles, card-menu ordering and
pinning directly from miniapp detail. Ordered ID-only choices retain the captured account,
device and mode, validate the visible permutation when queued work executes, and preserve
the last committed order on save failure. Actual isolated drag, menu movement, detail pin
and cold retention passed; independent conversation-rail pinning was outside this checkpoint.

Live Activities store a random session link in their existing owner-scoped preference.
Expo SDK58 keeps the start URL in immutable ActivityKit attributes; content updates cannot
repair a version-bound URL. `zoen.openActivity` therefore rereads the canonical item and
admits only the still-running selected session with the same item/end and exact current
membership. Widget and shortcut links keep their strict version checks. Cold activity links
wait for preference restoration; legacy selections without a session link retire without
discarding unrelated widget/shortcut choices. Production provider, route and Siri access
share the same membership projection, excluding cached non-member conversations.

Signed Release v40 source `5ac2cb7a` has 755 frozen inputs and passes 296 application tests,
TypeScript, Oxlint and Oxfmt. It is archived and preserving-installed on both QA simulators;
installed executable/JavaScript hashes and strict signatures match. The Live Activity check
created one synthetic countdown through the ordinary catalog, accepted the normal OS prompt
only on the disposable simulator, and kept private content hidden. It did not donate widget
or Siri content. Same-end version updates and access/session races have deterministic
production-helper/adapter coverage; they were not exercised as OS mutation-to-tap journeys.
Actual successful supported-device Speech, accessibility/performance and full parity remain
pending. Evidence: `/private/tmp/zoen-home-qa/v40/runtime.json` and
`/private/tmp/zoen-iphone-release-v40-verification.json`.
