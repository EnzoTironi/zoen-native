[Native surface index](../expo-native-surfaces.md).

## Bounded v85 private PDF edit recovery

The existing v2 PDF in History download QA1018v63b receives one black ink stroke.
An actual resolved breakpoint stops the isolated app after synchronous encrypted
retention and before canonical save; the process is killed while stopped. Cold
reopen recovers the same stroke with “Alterações recuperadas”. “Guardar para depois”
returns to chat and leaves exactly v1/v2. A second cold reopen retains the private
copy; explicit “Salvar alterações” produces one Anotações event, Enviando → Enviada.
The next cold history contains exactly v1/v2/v3; the canonical viewer keeps the
stroke and closes without a recovery choice.

Human v84 → v85 retains four Home cards, Plantain 6/8 (inner 75%, version 16),
Zo sleep 100%, larger text and six-page Bilingual chat history at 100%. Its old
11-second Delivered voice plays from native 0:00 to 0:11, then explicit Pause leaves
the Play control at 0:11/0:11. The composer stays empty; no human content is changed.

Evidence: `acceptance.json` → `evidence.nativePdfEditDraftRecoveryV85` and
`/private/tmp/zoen-pdf-edit-journal-qa-v85/native-acceptance.json`.
The 35 isolated/9 human observations, 46 copied PNGs and retired 59/16-step recordings
are bounded one-use diagnostics, not formal replay/E2E. Separate Swift journal/owner
and four core-process phases test FIFO, exact retry, integrity and conflict behavior.
Recovery starts only after Quick Look delivers a complete copy and retention ACKs;
undelivered strokes, native conflict/MIME-changed recovery UI, backup/restore UI,
physical-device, recipient/edited-byte and performance claims remain open.

**Historical accepted-v84 preface (verbatim):**

Current signed checkpoint, 18 October: **v84**, source `e35d2fb3`, is preserving-installed
and publicly verified on both iPhone QA simulators with 835 frozen inputs. Shared
voice audition and recovery of the retained imported WAV pass bounded native checks;
human Home/history/Delivered voice retention passes. App 696/696,
TypeScript/Oxlint/Oxfmt and all three exact-source Expo jobs pass separately.
Full product parity remains open.
