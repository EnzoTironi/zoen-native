# Preserve admitted input at the managed pause boundary

## Causal regression

A browser input event can arrive just before close or export. TextAgent queues
its egui Event::Text immediately. AppRunner then collects RawInput and invokes
Shell.logic before Shell.ui. The old adapter drained freeze(true) in logic,
setting Session.managed_frozen before the actual Word UI could consume that same
frame. Command policy refused those already-received characters. The host then
saved and acknowledged an incomplete document.

The original immediate-flush failure was intermittent. Waiting for animation
frames before flush masked it. The committed browser regression instead calls
the actual public pause ABI in the final trusted input event's bubble phase,
after the real TextAgent handler has queued the character and before the next
engine frame. It uses genuine browser keys and the genuine verified engine; no
synthetic positive input, forced focus, DOM value setter, fake clock or fabricated
Word document is involved.

The reusable helper is test/input-flush-assertions.mjs. With the actual managed
Word page already focused, the proof types `Word browser`, Enter, then calls:

```js
const pause = await pauseOnLastTypedCharacter(page, "Second lin", "e");
await page.evaluate(() => window.probe.flush());
await page.evaluate(() => window.probe.share("docx"));
```

The existing host export stream and independent genuine DOCX reader must observe
exactly `Word browser\nSecond line`. The helper itself does not invent a save
receipt, snapshot or canonical head.

Old package RED, repeated with the committed helper:

- The final input was trusted, data `e`, type insertText, composing false.
- The public pause acknowledged frozen=true.
- Export contained only `Word browser\n`; the second line was lost.
- No page or console error explained the loss.

## Adapter fix

The bounded FIFO now drains open/layout/unfreeze and other existing operations
in BeforeInput. A pause for an open, visible document drains in AfterInput,
after viewport::show has run the actual Word UI. Existing human-interaction
checks then see the current composition/gesture state. Canonical authority,
proposal IDs, retries, receipts, engine commands and limits are unchanged.

A hidden AppRunner logic pass does not consume RawInput. Such a pause refuses
with human_interaction rather than acknowledging a snapshot that excludes queued
input. This is a source-level fail-closed path; real background/iPhone behavior
remains to be qualified. Other editors require their own causal regression before
adopting the ordering change. No upstream engine or shared scheduler was rewritten.

## Qualification

External evidence root:
`/Users/enzotironi/.codex/builds/zoen-word-input-flush-v574`.

| Evidence | Result |
| --- | --- |
| lifecycle-control-result.json / webkit-close-fixed-01 | Old engine plus Root host lifecycle correction: full browser PASS |
| webkit-same-event-pause-red-03 | Actual final trusted input / immediate pause loses text |
| webkit-committed-regression-red-04 | Same causal RED using the committed regression helper |
| input-pause-preparation-05 | Fresh normal producer: 792 files compared; only API, shell and provenance changed |
| input-pause-wasm-clippy-06 | All-target WASM Clippy passed, 3.17 seconds |
| input-pause-wasm-build-07 | Normal release passed, 193.75 seconds |
| webkit-input-pause-green-08 | Same helper and actual host export/reopen: PASS |
| miniapps-word-resource-10 | Actual configured resource: 5 tests passed, 0 ignored |

The GREEN run also types `blocked after pause` after the acknowledged freeze;
those characters do not enter the document. It then verifies exact DOCX and PDF
text, responsive portrait/landscape/keyboard-height painting without version
writes, and a fresh renderer reopening the accepted version 2 with zero writes.
Accepted DOCX: 6437 bytes, SHA-256
47ac291cc53bc1e864a8a7243ed9da6cdc722666c9751fd2418f5c26247d39ef.
Page errors and console errors are empty, including close/reload teardown.

The underlying Core read/CAS is synthetic in this browser component proof. Real
IME/OS keyboard, background transitions, iPhone close/export and durable Core
acceptance remain separate. No offscreen animation-frame or background execution behavior on an iPhone is
assumed. A refused pause must preserve the pending draft for recovery/retry. No
native acceptance is claimed.

## Actual package

Producer source: e447b80ee3d0cd2d528e4c8120e709184c98a8b1.
Compiled prepared source: 53b666362b9a7c8229a208a846dd74acb1d1cf3c.
Its 33 exact producer inputs remain unchanged after integrating the frozen host
source. The WASIp2 worker component and ABI are unchanged.

| Asset | Bytes | SHA-256 |
| --- | ---: | --- |
| wordcraft.js | 80480 | da14faca74ed3ffae283ffdac1c40674ac049ccae80e631ed9be60502d360016 |
| wordcraft_bg.wasm | 46351299 | e3144f368ba317d2e565a8cd258f11ad0f4d92995c7b3911b219604a7e8edb8b |
| wordcraft.html | 624230 | d2fc071964c9cfa9df087eb76ad19dc384be063a33f45d080457c724481e4027 |

The ordinary resource producer validates the actual assets against the supplied
operator runtime under the new input-flush574 URL, retaining previous packages.
No service, live route, shared target, simulator or native SDK is modified here.
