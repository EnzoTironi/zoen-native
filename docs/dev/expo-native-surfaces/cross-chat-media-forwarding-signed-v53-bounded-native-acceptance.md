[Native surface index](../expo-native-surfaces.md).

## Cross-chat media forwarding — signed v53, bounded native acceptance

Source `dc973219` copies the current signed source file into a fresh destination Item,
with a new file key and newly encrypted chunks. Source membership and destination
writer permission are checked before writes; bounded file and thumbnail reads verify
their declared lengths and hashes. Ordinary attachments preserve their caption, name
and current thumbnail. Voice copies retain canonical edited metadata and clear source
reply/thread references, including legacy Android marker references. Their signed
`VoiceMeta.forwarded` flag produces one implicit voice message; ordinary files use
ItemCreated plus a forwarded MessagePosted. The optional flag defaults to false and
is omitted when false; no new event kind or UniFFI API is introduced.

The shared savepoint commits file keys, chunks, upload queue and message together;
failure restores storage and the in-memory projection. Regressions cover current edited
bytes and invalidated metadata, source edit/delete independence, stripped reply/thread
bindings, fresh encryption, destination-only recipient decryption, missing/corrupt/bounded
source data, permissions, injected rollback and single-entry cold replay. Unsupported
cross-chat miniapp references and unresolved legacy voice markers fail before writes.
These are source proofs, distinct from the installed observations below.

Full source checks passed 392 FFI, 26 MLS, 11 shared-type and 56 browser-core tests,
with zero failures; doc-test suites contained no tests. Strict Clippy passed for all
four crates and all targets. The application passed 348 tests, TypeScript, Oxlint and
Oxfmt; exported client types and the JavaScript bundle remain unchanged. Logs are
`/private/tmp/zoen-media-forward-v53-rust-tests.log`,
`/private/tmp/zoen-media-forward-v53-clippy.log`,
`/private/tmp/zoen-media-forward-v53-app-check.log` and
`/private/tmp/zoen-media-forward-v53-client-types.log`.

The rebuilt simulator-only native artifact has 766 frozen inputs and source SHA256
`8d8d34474a81f0433d0a36a03486bb80a33ba2787b9e57180a143e124d587ff6`.
Exact installed executable/JavaScript hashes and strict signature passed on both QA simulators.
Actual ordinary native UI on the isolated account accepted:

- Forwarding the existing edited four-second voice from Message Studio QA once to
  the self-only `Forward QA 1018` conversation. Play changed to Pausar, then completed
  at `0:04 / 0:04`.
- Forwarding the source thread photo `Zoen-QA-photo.jpg` to the destination's main
  conversation. Native QuickLook opened the full photo and closed successfully.
- Cold restart through Home and Chats retained exactly one forwarded voice and one
  forwarded photo, with no inherited thread badge, an empty composer and 100% latest
  history position.

Human E2B received a preserving v53 install after a fresh known-idle Bilingual QA
preflight with an empty draft. Installation exited successfully. Normal restart retained
four Home cards and Recipe's 6/8 progress; Chats to Bilingual QA reached 100% with its
existing audio/delivered receipt ending at 0.854 above input top 0.897, approximately
36 points clear. The composer remained empty, and no mutations were performed. Its
six-step diagnostic retains one failed initial `Fixados` section wait: that Home shows
`Miniapps` without a pinned section, and the corrected `Miniapps` wait passed. No other
wait failed. This is bounded delivery evidence, without a formal replay claim. Captures
are `human-final-ax.txt` and `human-final.png` in the v53 evidence directory below.

Captures and records are under `/private/tmp/zoen-media-forward-qa/v53/`; the build
receipt is `/private/tmp/zoen-iphone-release-v53-verification.json`. The 28-step diagnostic
retains an obsolete Home-heading failed wait (`Feito para ficar.`, while the current
heading is `Início`); a corrected live `Fixados` wait passed. It also contains a native
QuickLook cross-tree warning and brittle exported tap selectors. The original diagnostic
is retained and its executable flow has been retired. This is bounded manual observation,
not a passing formal replay.

Destination-only peer decryption is deterministic regression evidence, not a new native
peer export. Native video/document forwarding, induced storage/access races, multi-person
reaction details, group sender profile entry, supported-device Speech, physical/provider
journeys, broader VoiceOver/Dynamic Type/performance and full comparative parity remain
pending at that checkpoint; selected sender-profile entry subsequently passed the bounded
v54/v55 checks below. Cross-chat miniapp cloning remains unsupported. V29 independent
media, v38 keyboard and v40 Live Activity evidence retain their original artifact attribution.
