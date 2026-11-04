# Shared incurred-result retention

Part of [agent continuity #327](https://github.com/EnzoTironi/zoen-native/issues/327), using the [existing shared checkpoint](shared-runtime-continuity.md). Enzo owns product direction; Codex root owns this technical implementation. This is accounting/result retention, not activation of public model execution, publication, refunds or complete deployment restore.

## Original result before database work

An authenticated chat Capture binds the frozen request, context, price and response. Before SQL/FDB work, the existing independent anchor writer durably retains its original encrypted bytes and opaque IDs under a content address. Files use the existing private directory, writer ownership, temporary-file/fsync/rename/directory-sync pattern. Exact retries synchronize the same original; they never replace it. These files are inert evidence, not another ledger, provider queue or live admission. No prompt, response text or credential is written in plaintext.

Original staging works before reconciliation, including when another transition is pending or a bank is unavailable. If the host cannot acknowledge this staging, the existing FDB evidence/transfer path is attempted independently. Once the stores are available, `reconcile` derives unfinished work from originals absent from the committed capture inventory, plus that existing FDB fallback queue. No separate completion flag is stored. Completed original files remain retained; automatic retention/GC and production scale remain open work.

The original-store profile is bounded at 512 files and 512 KiB per opaque record. Chat ciphertext remains bounded by the existing 98,304-byte limit. The capture inventory allows 2 MiB of actual encrypted evidence; ordinary shared transitions reserve one subject and the maximum allowed result size for each admitted attempt without a capture. Other activity cannot consume this in-flight capacity after a provider call has begun. These are explicit single-host operational bounds, not measured scale readiness. Larger final chat-Capture encodings and matching gateway response-bound coverage remain required before enabling productive dispatch; reserving this bounded profile does not certify arbitrary provider response sizes.

## One financial transition

Capture reuses the original financial settlement transaction, journal, postings, evidence, disposition and outbox tables. It validates the existing SQL claim/witness, actual FDB admitted nonce and frozen Binding. It does not require a current execution grant or enabled current policy: already incurred expense must survive later revocation. This grants no further action or output permission. Known usage settles the actual frozen price once, including expense above a reservation; unknown usage retains the claim/hold and original evidence without inventing cost or release.

PREPARED binds the retained original's content address, exact evidence reference and financial before/after digests. SQL commits its evidence/financial rows and detector atomically. FDB commits the existing evidence value, a reference index, and exact detector/history afterward. The index is not another result or balance store. General financial outbox acknowledgement remains a subsequent unqualified transition and is parked by shared reconciliation; it cannot execute the unmanaged writer path.

| Retained state | Accounting recovery |
| --- | --- |
| Original only, no PREPARED | Derive normal capture work from the authenticated original and actual original admission |
| PREPARED, both previous | Retain/settle that exact incurred result after checking actual admission; never call the model |
| SQL next / FDB previous | Observe exact SQL witness/evidence/disposition; retain the same original FDB bytes and history |
| Both next or lost final ACK | Compare actual rows and original history; finish observation without another charge |
| SQL previous / FDB next | Refuse contradictory order; do not fabricate missing financial history |
| Clean markers but missing/changed actual evidence | Refuse; detector agreement does not authorize repairing an undeclared rollback |

The independent original's content hash and existing custody authentication both remain required. Missing/corrupt originals, exhausted profile capacity, unavailable independent staging plus unavailable fallback, pending negative resolution, and full SQL/FDB/cofre restore/takeover remain explicit operational work. No absence proves non-admission or refund.

## Automated evidence

The Runtime cohort uses actual retained OpenMLS/native extraction, ordinary shared one-shot admission and one explicit loopback HTTP provider call per case. It covers normal capture, original-only interruption, four SQL/FDB/ACK cuts, late device revocation, unknown usage, a closed actual SQL pool, a broken/restored actual FDB pairing and unavailable/restored original storage with FDB fallback. Cold reconciliation preserves exact ciphertext, one claim/witness, one charge or retained unknown hold, and one HTTP send. An actual older SQL backup containing admission but no capture refuses despite overlaid current markers. Missing actual FDB evidence and tampered ciphertext also refuse.

Anchor tests cover write/file-sync/directory-sync acknowledgement failures, idempotent retry and cold reopen without checkpoint movement, changed bytes, oversized records and symlink/path rejection. Automated source/service tests do not substitute for Enzo's later product/device acceptance. Public productive execution, output, terminal wake consumption, release/negative resolution and full deployment restore/takeover remain closed pending qualification.
