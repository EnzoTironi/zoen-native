[Native surface index](../expo-native-surfaces.md).

## Message care — signed v52, bounded native acceptance and delivery

Source `8905754e` changes a reaction-chip tap from an immediate mutation to canonical
participant details. The native sheet shows the selected emoji's people and identifies
the current person as You / Você. Adding, replacing or removing a reaction requires an
explicit action. Participant details and full emoji entry share one sheet, retaining
the current message, owner and access checks across the transition.

The emoji field accepts a single Unicode emoji sequence through the system keyboard
or paste, including flags, skin tones and joined sequences. The OSS `emoji-regex` parser
preserves the selected sequence and enforces the core's 32-byte UTF-8 annotation limit;
it does not split a reaction into code points or silently truncate it. The existing
quick palette remains available.

Forwarding now captures identity, device, mode and source message, lists only current
member destinations and rereads the canonical source message before issuing the command.
A synchronous latch prevents duplicate callbacks within the current focus. Source and
target membership are rechecked before delivery and completion. Back, unmount, refocus
and access changes retire the navigation intent; a forward already accepted by the
core continues, while its late completion cannot navigate or update an obsolete view.
Twenty-five focused tests cover delayed preflight/completion, duplicate taps, owner and
membership changes, canonical deletion and retries. Three emoji tests cover complete
Unicode sequences and the byte bound. These are source proofs, not native induced-delay
acceptance. Source `ef7acda9` passes the final 348-test check with TypeScript, Oxlint and
Oxfmt (`/private/tmp/zoen-message-care-v52-app-check.log`).

Signed isolated v51, built from `8905754e` with 769 frozen inputs, passed installed
executable/JavaScript hash and strict-signature checks. Actual ordinary native UI accepted:

- Tapping the existing own 👍 chip opened its canonical participant name and Você without
  changing the reaction. Choosing another reaction switched that same sheet to emoji entry.
- Pasting 🇧🇷 through the native TextField's ordinary Colar action and selecting Reagir
  replaced 👍 with the flag chip in the conversation.
- Tapping the flag chip and explicitly choosing Remove returned to the conversation with
  no reaction chip.
- Long press, Forward and destination selection created one new visible forwarded text
  message `01M56G8580P27Y5XNWSJ4V0YK0` from source
  `01M56FKDP28Q8SEK4BZYDEABDY` in the new self-only `Forward QA 1018` conversation.
  No other members or contacts were selected. This establishes the bounded text-forward
  journey, without claiming cross-chat media or independent peer-byte verification.

V51 also exposed poor native emoji-field contrast and white text on the lime filled
button. Source `ef7acda9` gives the field explicit theme ink, placeholder and surface,
and the filled action its semantic contrast color. Signed v52 from that source has 769
frozen inputs; exact installed executable/JavaScript hashes and strict signature passed
on the isolated simulator. Actual bounded native checks accepted:

- Text tap opened the quick palette; More waited for the real UIKit dismissal before
  opening the native emoji sheet. The semantic field surface, placeholder, 22-point typed
  text and filled-button contrast rendered correctly.
- Invalid `hi` exposed the unsupported-input error. Clearing it, then using the ordinary
  system Colar action to paste 👍🏽 and selecting Reagir produced that own skin-tone chip.
- Tapping the chip opened its canonical people and You / Você. Closing the sheet left
  the reaction unchanged.
- Cold restart, Home, Chats and `Forward QA 1018` retained the exact single forwarded
  message `01M56G8580P27Y5XNWSJ4V0YK0`. Returning to Message Studio retained its original
  root message and 👍🏽, opened at 100%, and kept the composer empty.

The 34-step v52 diagnostic has eight cross-tree warnings and no failed waits. It is a
bounded native observation, without a polished formal-replay claim. V52 captures and
records are under `/private/tmp/zoen-message-care-qa/v52/`; build verification is
`/private/tmp/zoen-iphone-release-v52-verification.json`.

Human E2B received v52 after a fresh known-idle, empty-draft preflight on v50. Preserving
installation exited successfully, and exact installed executable/JavaScript hashes and
strict signature passed. Normal restart retained four Home cards and Recipe's 6/8 progress.
Chats to Bilingual QA reached the latest history at 100%; the existing audio/receipt ended
at 0.854 above input top 0.897, approximately 36 points clear. Its audio actions, More and
native emoji sheet opened, then closed without typing, reacting, sending, importing or
changing preferences or account data. Return retained 100%, an empty composer and the
audio clearance. The 12-step human diagnostic has three cross-tree warnings and no failed
waits; it is bounded delivery evidence rather than a clean formal replay. At that
checkpoint, both simulators ran accepted v52; earlier v50 delivery retains its original
historical attribution.

Native captures and accessibility records are under `/private/tmp/zoen-message-care-qa/v51/`,
including `reaction-participants.png`, `flag-input-before-polish.png`, `forward-success.png`
and their text records. The 28-step diagnostic in `flow-summary.txt` retains raw steps and
cross-tree warnings; it is not a clean formal replay. The signed build receipt is
`/private/tmp/zoen-iphone-release-v51-verification.json`. Deliberately delayed native
forwarding races remain untested; their 25 regressions are source proof. At v52,
cross-chat voice forwarding was still unsupported by the core; the subsequent v53
implementation and bounded acceptance are recorded below. Successful supported-device Speech,
group-message sender profile entry, VoiceOver, performance, broader message journeys and
complete Swift/WhatsApp/Slack parity remain pending.
