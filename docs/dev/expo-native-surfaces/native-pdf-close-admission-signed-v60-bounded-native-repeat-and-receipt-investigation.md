[Native surface index](../expo-native-surfaces.md).

## Native PDF close admission — signed v60, bounded native repeat and receipt investigation

Source `6a70227d8a580b188bdd3e18af893fe235d55350` closes the queued-editor
admission window during ordinary account close. `prepareForClose(before:)`
invalidates the generation admission floor and checks the active editor in one
MainActor turn. An active editor still retains its lifecycle wait; a registered
preview waiting to open cannot present after close prepares. Ten focused native
scenarios pass, including active-editor waiting and queued-editor retirement.
Disabling only the admission gate produces the expected late-editor failure.
Strict Swift 6 UIKit typecheck and all 428 application tests with TypeScript,
Oxlint and Oxfmt pass. Logs:
`/private/tmp/zoen-pdf-edit-close-v60/{positive,negative,uikit-typecheck}.log` and
`/private/tmp/zoen-v60-pdf-close-app-check.log`. These establish source ordering,
not an induced native account-close/erasure journey. Rust and the JavaScript
bundle are unchanged from v59; the 401/401 FFI and strict Rust Clippy evidence
retains its v59 attribution, with no fresh Rust-run claim.

The signed ARM64 simulator Release has 771 frozen inputs and source SHA256
`9df4fad29e5826be2f0848cdb8c985274e19ee6364837cb73f6b55151f11763f`.
Preserving installs, exact executable/JavaScript and strict signature pass before
and after UI on both QA simulators. On isolated QA, the same Forward QA PDF opens
v2 with the prior title underline. One additional native Markup subtitle underline
saves the existing file as v3, now 14.5 KB. Files/history shows exactly v3 Anotações,
v2 Anotações and v1 Criado. After a full process restart, the ordinary same chat
PDF retains both underlines, first-page total 53 and the full second-page table
and total 161. The final chat has one existing PDF card and two version events,
with an empty composer, closed keyboard and idle audio. Original v1's unmarked,
readonly preview retains its separate v59 attribution.

The only content mutation is one additional synthetic PDF version, event
`01M56TJKG0B1P7WF49T103KE8V`; no new attachment send, import, capture, reaction,
restoration, account or preference change occurs. The prior v59 event now projects
Enviada, while the new v60 event remains Enviando at final inspection. No recipient
delivery or independent edited-byte peer download/hash is established. Evidence:
`/private/tmp/zoen-pdf-edit-qa/v60/runtime.json` and
`/private/tmp/zoen-iphone-release-v60-verification.json`. The 31-step diagnostic has
five cross-tree warnings, zero unmet waits, a raw Files-row target and semantically
anchored ink/swipe gestures. The unsafe export is retired; no formal replay claim.

Human preflight on v59 has the existing delivered Bilingual voice, unchanged
history, empty composer, closed keyboard and idle audio. The exact signed v60
archive is preserving-installed and verified. Cold Home retains four miniapps
and recipe 6/8; Bilingual retains history, carousel and the existing idle voice
card. The native attachment dropdown opens all four actions and dismisses without
selection. There is no typing, send, import, capture, reaction, restoration,
account or preference mutation. The final composer stays empty with keyboard
closed and audio idle. However, preflight Entregue a todos on the voice and prior
own messages becomes Enviada/one check after cold reconnect and remains on the
settled final screen about 30 seconds later. Preserving delivered-state stability
is not claimed. This is an observed current issue, not proof that the v60 close
change introduced it. Evidence:
`/private/tmp/zoen-pdf-edit-qa/v60/human-runtime.json`. Its ten-step diagnostic has
one Home-heading cross-tree warning and zero unmet waits. Dropdown opening retains
a raw point; the exported dismissal targets the underlying message incorrectly,
while the actual tap dismisses the menu without a reaction. The unsafe export is
retired, without formal replay.

Read-only receipt investigation remains open. Preliminary source review finds
pre-existing volatile `NetState.receipts`; missing member watermarks default to
zero before relay receipt replay. Rust is unchanged from v59, and the live cause,
recovery and correction have not been established. History/content retention
does not establish receipt-state stability. No receipt fix or full product
acceptance is claimed.

Actual induced native close/erase/conflict/storage/access failures, independent
edited-byte peer proof, physical-device behavior, broad accessibility/performance
and comparative parity remain pending. Unsaved edits have in-session recovery
only; process-death draft recovery is not implemented. V58/v59 evidence and prior
artifact records remain unchanged.
