[Native surface index](../expo-native-surfaces.md).

## Bounded v84 voice audition and imported-WAV recovery

The isolated preserving update recovers the same failed 24-second WAV draft without
reimport. Playback continues through compact → detail → compact, with native clocks
6 → 12 → paused 18 seconds; paused rate 1.5 survives expansion. A paused original
12-second tap remains paused, and after cutting the selected 6.01–10-second range,
original 12 maps to edited 8 seconds. Play from selection advances from its first
retained frame; cutting stops the old preview. Restore and Undo/Redo stay paused.
Cancel while playing followed by cold return leaves no draft or new Send and
retains earlier photo/video/v82 voice history.

Separate host AVFoundation tests cover full/cut PCM16 mono WAV at 8/16/24/44.1 kHz,
exact decoded frames/rate, retained/excluded tones, fades, waveform and cleanup.
The compiled old production fails the same assertions at its fixed-bitrate writer
with NSError 560226676. Failed v83 remains diagnostic, never promoted as main;
this host proof is separate from v84 simulator recovery. Human v82 → v84 retains
four Home cards, recipe 6/8, six-page chat history at 100%, larger text and old
11-second Delivered voice (opened at 2, paused at 7 seconds), with empty composer
and closed keyboard; no human content/calendar/preference changes.

Evidence: `acceptance.json` → `evidence.nativeVoiceAuditionV84` and
`/private/tmp/zoen-voice-audition-qa-v84/runtime.json`. The 65 isolated/18 human
observations, 38 copied PNGs and retired 30/9-step recordings are one-use diagnostics,
not formal replay/E2E. The 26.4-second video shows native clock continuity, without
captured audio-output, perceptual-quality or timing/performance claims. No new Send,
recipient, microphone/ASR, fresh VoiceOver or physical-device acceptance is implied.
Prior v82 pending-trim and dedicated v82a offline proofs retain their exact attribution;
unchanged Rust 463/463 remains attributed to `6c338bcf`.

**Historical accepted-v82/v82a preface (verbatim):**

Main and human accepted artifacts remain **v82**, source `c71383a7`, with 832 frozen
inputs. A separate signed **v82a-offline** artifact now passes the bounded fresh-device
voice/photo/video offline Send → offline cold → automatic reconnect → independent
three-ID/byte-digest retention → online cold → explicit Read journey. Its 18782 relay
environment is dedicated; the existing v82 installs are unchanged. Same-source
checks retain their attribution, with no new code/tests or full product-parity claim.
Earlier preparation-only statements below retain their original v82 attribution.

**Historical accepted-v82 preface (verbatim):**

Current signed checkpoint, 18 October: **v82**, source `c71383a7`, is preserving-installed
and exactly verified on both iPhone QA simulators with 832 frozen inputs. Pending
trim survives blocked Undo and cold restart; explicit Discard/history/Apply and
one compact Send/cold same-ID retention pass. Human Home/history/Delivered voice
retention passes. App 669/669, TypeScript/Oxlint/Oxfmt and all three exact-source
Expo jobs pass separately. Unchanged Rust 463/463 retains `6c338bcf` attribution.
Offline v82a remains preparation only; full product parity remains open.

**Historical accepted-v81d preface (verbatim):**

Current signed checkpoint, 18 October: **v81d**, source `aeb4127f`, is preserving-installed
and exactly verified on both iPhone QA simulators with 832 frozen inputs. Recovered
edited-voice Send/cold and new photo/video caption-draft cold/send/cold pass;
human Home/history/Delivered voice retention passes. App 666/666,
TypeScript/Oxlint/Oxfmt and all three exact-source Expo jobs pass separately.
Unchanged Rust 463/463 retains `6c338bcf` attribution. V81b/v81c remain partial
diagnostics; independent recipient/offline media and full product parity remain open.

**Historical accepted-v80 preface (verbatim):**

Current signed checkpoint, 18 October: **v80**, source `131e81de`, is preserving-installed
and exactly verified on both iPhone QA simulators with 822 frozen inputs. Bounded
native fullscreen/Details, opaque consent dismissal and item-sheet-to-OS-calendar
prompt sequencing pass; both accounts retain their data. App 629/629,
TypeScript/Oxlint/Oxfmt and all three exact-source Expo jobs pass. Preappearance/
queued native races, fresh HTML keyboard/VoiceOver/physical/performance checks and
full product parity remain open.

**Historical accepted-v79 preface (verbatim):**

Current signed checkpoint, 18 October: **v79**, source `03dd599a`, is preserving-installed
and exactly verified on both iPhone QA simulators with 815 frozen inputs. Explicit
Schedule availability passes actual isolated calendar Deny/Allow Full Access, fixture
conflicts, removal and day-change checks; human Home/history/plan/poll retention passes.
App 612/612, Schedule/real production bundle 12/12, scoped source checks and all three
exact-source Expo CI jobs pass, separately from the manual native evidence.
Native all-day/DST/fault cases and the observed scrolling/consent-legibility follow-ups
remain open; full product parity is incomplete.

**Historical accepted-v78 preface (verbatim):**

Current signed checkpoint, 18 October: **v78**, source `46811171`, is preserving-installed
and exactly verified on both iPhone QA simulators with 797 frozen inputs. Actual
public iOS 27 VoiceOver speech navigation and the seven-category native audit pass
on the selected conversation; human Home/history/Delivered voice retention passes.
App 603/603, TypeScript/Oxlint/Oxfmt and exact-source Expo client/release-web-e2e CI
pass. V77 denial/source audio/calendar checks and its failed audit remain separately
attributed; full product parity remains open.

**Historical accepted-v75 preface (verbatim):**

Current signed checkpoint, 18 October: **v75**, source `c33f2760`, is preserving-installed
and exactly verified on both iPhone QA simulators with 795 frozen inputs. Bounded
native audio trim/discard/apply, one compact Send/cold return and human retention
pass. App 571/571 plus TypeScript/Oxlint/Oxfmt and source-head Expo CI pass. Native
VoiceOver and recorder-fault checks remain source-only; the unchanged Rust core
retains its v74 444/444 proof. Full product parity remains open.

Dedicated signed **v76** adds bounded ordinary-text offline/cold/reconnect and
independent exact-one/Read evidence on a fresh simulator with a separate 18776
relay environment. Accepted human and existing isolated pointers remain **v75**;
their artifacts and all earlier evidence keep their original attribution.

**Historical accepted-v74 preface (verbatim):**

Current signed checkpoint, 18 October: **v74**, source `43ef6b84`, is preserving-installed
on both iPhone QA simulators with 794 frozen inputs and reveal logging disabled.
Bounded fitting in the same editor, Edit/Cancel, canonical quote freshness,
activity-note search and human retention pass. App 547/547 plus TypeScript/Oxlint/Oxfmt
and native Rust 444/444 pass. Earlier v68 draft/peer/media and v73 positioning proofs
retain original attribution; full product parity remains open.

**Historical accepted-v73 preface (verbatim):**

Current signed checkpoint, 18 October: **v73**, source `04451232`, is preserving-installed
and exactly verified on both iPhone QA simulators with 789 frozen inputs and reveal logging
disabled. Bounded first-result/arrow/drag search positioning and human retention
pass; app530/530, TypeScript/Oxlint/Oxfmt and focused22/22 pass. The unchanged v67
native core retains its441-test proof. Completed v68 draft/peer/media evidence and
all older gaps keep their original attribution; full product parity remains open.

**Historical accepted-v68 preface (verbatim):**

Current signed checkpoint, 18 October: **v68**, source `90d67d87`, is preserving-installed
and exactly verified on both iPhone QA simulators with 788 frozen inputs. Bounded
normal draft/thread/quote/rejection/cold/peer/Read and human-retention results are
below. App checks pass 508/508 plus TypeScript/Oxlint/Oxfmt; unchanged v67 core
retains its 441/441 and scoped strict Clippy proof. Three visual follow-ups and
full product acceptance remain open; no current-head hosted-CI claim is made.

**Historical v66 preface (verbatim):**

Current signed checkpoint, 18 October: **v66**, source `71487c34`, is preserving-installed
on both iPhone QA simulators. Its 780 frozen inputs, exact executable/JavaScript and
strict signatures are verified before and after native acceptance. The bounded
attachment, caption, video-preview, thread and recipient journeys are documented below.
Application checks pass 482/482 and the full FFI suite passes 427/427, with TypeScript,
Oxlint, Oxfmt and relevant strict Clippy. Full product acceptance remains incomplete.
The earlier checkpoint summaries and pending observations below retain their historical
artifact attribution; they are not statements about the currently installed v66.

Source audit: 2026-10-18. Current acceptance priority is iPhone; Web and Android follow later. The signed ARM64
iOS simulator Release has compiled the widget extension, Live Activities and App
Intents metadata. Signed v7 accepts the redacted Portuguese Widget gallery preview. Signed v40
accepts a private Live Activity on the Dynamic Island and Lock Screen, warm and process-terminated
cold compact taps, Lock Screen navigation and explicit ending on the isolated QA simulator.
Signed v50 accepts the bounded conversation-card journeys and preserving simulator
delivery described below; its installed hashes and strict signatures match the archive.
Signed v51 adds bounded isolated reaction-participant, Unicode emoji and text-forwarding
acceptance. Signed v52 accepts the subsequent native contrast polish, skin-tone reaction
and cold retention. Signed v53 adds bounded cross-chat edited-voice and photo forwarding,
native playback/QuickLook and cold retention on the isolated simulator. Signed v56
established preserving installs on both QA simulators and bounded native composer-frame,
Menu/search/send and retention acceptance. Human delivery retains Home, history, its existing
audio/receipt and an empty composer with focus only and no content mutations. Earlier v55
profile/overlay/Rust, v54 thread/gesture, v53 forwarding and v52 emoji-sheet evidence keep
their original attribution. Signed v58 adds bounded isolated PDF/photo preview, Share →
Save to Files, reopen and cold media/caption retention acceptance. Signed v59 adds
native PDF Markup saved as a checked canonical version, original-version history and
cold edited-content retention on isolated QA. Signed v60 retires queued editors
before graceful close and repeats native Markup as v3, retaining both edits and
v1/v2/v3 history after cold restart. Both QA simulators have verified preserving
v60 installs; human Home/history/idle voice and native dropdown remain. Its prior
delivered messages temporarily project Sent/one check after cold reconnect,
including the settled approximately-30-second observation; later v60 preflight
recovers Delivered through the relay. Signed v61 persists accepted peer receipts
and restores them before initial snapshots. Its isolated native text journey
shows Delivered → Read → full cold Read after the volatile peer stops; both human
cold starts retain existing text/voice Delivered. Both preserving v61 installs,
exact executables/JavaScript and strict signatures are verified. Native QA keeps
a healthy relay; offline text/voice proof comes separately from source tests.
The historical v60 PDF-version event remains locally Enviando at that inspection;
recipient delivery and edited-byte peer proof remain pending.
Selected widget content, Siri and canonical-version-update-to-OS-tap remain pending.
Installed packages and successful compilation do not establish those journeys.
