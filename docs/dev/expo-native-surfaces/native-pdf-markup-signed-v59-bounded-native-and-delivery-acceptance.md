[Native surface index](../expo-native-surfaces.md).

## Native PDF Markup — signed v59, bounded native and delivery acceptance

Source `2a87b75098e90fc3859963dd9219ab24421f7745`, including implementation
`2692c714`, enables native Quick Look Markup for the exact current writable PDF.
Eligibility comes from the staged native item, version, SHA, engine, account,
device and current membership. Quick Look's edited URL is copied synchronously
before its delegate returns. Each queued save checks the last accepted version
and SHA under one core lock, using the existing encrypted `ItemVersioned`
transaction. A stale base cannot replace unseen changes. Original history,
canonical metadata and the separate message caption remain intact.

Failed saves retain their owned copy for native Retry/Discard while the session
is alive. A cleanup failure after an accepted save retries deletion without
creating another version. Retrying an older save does not silently acknowledge
a newer edit whose capture failed. Normal account close fences new work and
waits for recovery; explicit erasure/module destruction drains workers and
discards temporary edits. Unsaved draft recovery after process death is not
implemented.

The full FFI suite passes 401/401 in 225.22 seconds, including seven checked-save
regressions for stale version/hash, permissions, voice ownership, rollback/retry,
metadata/caption/history preservation, cold original/new bytes and session-language
errors. Strict FFI lib/tests Clippy passes. All 428 application tests, TypeScript,
Oxlint and Oxfmt pass. Nine native edit-owner scenarios and eleven readonly
lifetime/filesystem scenarios pass, with strict Swift 6/5 UIKit typechecks.
Expected-base and newer-capture-loss negative controls fail as intended. These
source proofs remain separate from actual native UI evidence. Logs:
`/private/tmp/zoen-file-edit-ffi-full-tests.log`,
`/private/tmp/zoen-file-checked-rust-clippy.log`,
`/private/tmp/zoen-file-edit-app-check-final.log` and
`/private/tmp/zoen-pdf-edit-owner/{pdf-edit-tests,readonly-tests,negative-base,negative-capture}.log`.

The signed ARM64 simulator Release has 771 frozen inputs and source SHA256
`d4437b74dfd3ddc381fd4337f8d154e3c2a129ef2345395b4e22ee489b15ba0d`.
Preserving installs and exact executable/JavaScript/strict-signature verification
pass before and after UI on both QA simulators. On isolated QA, the ordinary
existing Forward QA 1018 PDF opens with total 53. Native Markup draws an underline
below the first-page title, and the native checkmark saves it. The same attachment
card changes from 4 KB to 8.6 KB; one Anotações version event appears, with no new
attachment send. Reopening the same PDF shows the saved underline. The Files tab
identifies v2, and its history shows exactly v2 Anotações and v1 Criado. Selecting
v1 previews the unmarked original without editing controls; no restoration occurs.
After a full process restart, the current PDF retains the underline and complete
second-page table/total 161. Viewers close back to an empty composer, closed
keyboard and idle audio.

This journey makes exactly one synthetic PDF-version mutation, event
`01M56SXF6Z6V8VZFSMQ3WQHB9E`. It remains locally Enviando; no remote receipt,
independent edited-byte peer download, hash or reread is claimed. No new attachment
send, import, capture, reaction, restoration, account or preference mutation occurs.
Evidence: `/private/tmp/zoen-pdf-edit-qa/v59/runtime.json` and
`/private/tmp/zoen-iphone-release-v59-verification.json`. The 42-step diagnostic has
six cross-tree warnings and one wrong Home-heading wait, corrected using the
actual heading. It includes a raw Files-row target and a semantically anchored
ink gesture. The unsafe replay export is retired; no formal replay is claimed.

Human delivery starts from a fresh Bilingual/empty/keyboard-closed/audio-idle
preflight. The exact signed archive is preserving-installed and verified. Cold
Home retains four miniapp cards and recipe 6/8; Chats and Bilingual retain history,
miniapp carousel and the existing delivered voice. The native attachment dropdown
opens Arquivo, Importar áudio, Câmera and Fotos e vídeos, then dismisses without
selection. No typing, send, import, capture, reaction, restoration, preference or
account mutation occurs. The final composer remains empty with keyboard closed
and audio idle. Evidence: `/private/tmp/zoen-pdf-edit-qa/v59/human-runtime.json`.
Its eleven-step diagnostic has two cross-tree warnings and one wrong Fixados
heading wait, corrected to Miniapps. Dropdown opening retains a raw point; the
export incorrectly serializes dismissal as an underlying message, while the live
tap dismisses the menu and the final tree has no reaction. The unsafe export is
retired, with no formal replay claim.

Physical-device behavior, induced native UI conflict/storage/permission failure,
active-viewer account close/erasure, independent edited-byte peer proof, broad
VoiceOver/Dynamic Type/performance and full comparative parity remain pending.
V58 evidence and artifacts remain unchanged under their historical records.
