[Native surface index](../expo-native-surfaces.md).

## Message interaction checkpoint — signed v29, bounded runtime passes

The previously verified signed v29 artifact was built from source checkpoint `d450e37d`.
Its signed Release build, strict signature verification and preserving public
Xcode installation on isolated
388CA passed. Installed executable/JavaScript and the archived app match the stable
742-input artifact. Exact hashes appear in
`apps/zoen/acceptance.json → previousControlledV29IphoneArtifact`; earlier v27/v28
artifacts remain separately recorded. Restart retained the certified synthetic
`qa_msg_1017d` profile and group. During the historical v29 delivery, human-operated
E2B received the same verified archive through a preserving update after the user
confirmed it was idle. Fresh preflight showed Chats without an active composer, capture or modal. Installed
executable/JavaScript and strict signature checks passed; normal restart retained
four canonical Home cards/progress and the Bilingual QA conversation/history. The
anchored + Menu opened/dismissed without selection, imports, sends or account
mutations. One stale-transport Chats tap was explicitly not delivered; scoped E2B
transport recovery required no app reset. The v26 and v29 human artifacts remain
historical evidence after v32 delivery.
Personal B9 is untouched. The earlier v29 delivery evidence is
`/private/tmp/zoen-iphone-release-v29-verification.json → human_installation` and
`/private/tmp/zoen-message-polish-qa/v29/human-native-menu.png`. This bounded delivery
check does not replace isolated media acceptance or full product acceptance.

Controlled v29 establishes these bounded results:

- The + opens an anchored native SwiftUI Menu with Photos and videos, Camera,
  Import audio and File. The initial attachment chooser is no longer a modal.
  Import audio opens the ordinary Files picker, and the known synthetic 5.54-second
  fixture enters the compact 0:05 dock.
- The detailed native editor opens. A start trim of 0.98 seconds and Apply show
  edited 4.56 seconds; Undo restores 5.54 seconds and Redo restores 4.56 seconds.
  Collapse shows compact 0:04, and reexpansion retains 4.56 seconds. Preview changes
  the native control from Play to Pause; that state alone does not establish completed
  playback or independent decoding.
- One Send creates compact 0:04 main-conversation message
  `01M563DF8196SHDFMV81RYCYKF` with one Sent check. The ID comes from read-only native
  UIView inspection. Canonical edited metadata and independent decrypted voice
  decoding also passed: current signed v1
  contains 129,409 bytes, canonical 4,557 milliseconds and SHA256
  `bb338b462571c16cb028f9cce2b35fe8f03c0bfbce111e0122e235de4b4de9bd`.
  Full AAC mono 44.1kHz decode yields 200,985 sample frames/4.557482993 seconds,
  0.483 milliseconds from native metadata; the original fixture hash is unchanged.
  This establishes decode/playability, without human listening or transcription.
- One known synthetic photo reply `01M563VC1D3NBEANE3B1H7PY7Q` to the edited voice
  was sent once. The native image card and one-reply count are visible. A normally
  linked independent peer verified signed current v1 item
  `it_01m563vc1cz9bfgqq8dgz5jsws`, exact `in_thread` binding to the voice root,
  and decrypted JPEG 8,530 bytes/SHA256
  `9d1e61801b349f3308d543998db013ff8220e713fd5977ad35f8ae7844fb1b56`.
  Strict full decode passes 1200×800/one RGB frame; the original fixture is unchanged.
  Tapping the image opens native QuickLook and closes normally without reactions.
  This proves the selected thread-photo journey, not all thread media. No microphone
  or human media was used.

The actual runtime record and named root-reviewed images are under
`/private/tmp/zoen-message-polish-qa/v29/`: `runtime.json`,
`attachment-native-menu.png`, `attachment-native-menu-settled.png` and
`voice-send-once.png`, alongside the individual trim/Apply/Undo/Redo captures.
`voice-peer-export.json` and `voice-peer-decode.json` establish the independent
current-version voice proof. `thread-photo-peer-metadata.json`,
`thread-photo-peer-export.json` and `thread-photo-peer-decode.json` establish the
selected reply and full JPEG decode; `thread-photo-send-once.png` and
`thread-photo-quicklook.png` capture its native card and preview.

The earlier controlled v28 run establishes native video tap and vertical scrolling
without parent reactions, a visible anchored reaction capsule/action plate, long
press without UIKit Copy, and correct quoted Reply after menu dismissal. Native
video controls appeared; exact seek correctness was not verified. Precise voice
trim/Undo remained inconclusive because screenshot/AX targets and queued touches
did not reliably agree. The read-only transport audit compared public-Xcode and
Argent captures 35 milliseconds apart with zero differing pixels out of 3,162,132;
it established no wrong-device or stale-capture cause. These observations supersede
the earlier whole-v28-runtime pending label, without claiming complete acceptance.
Evidence remains `/private/tmp/zoen-message-polish-qa/v28/runtime.json` and
`transport-audit/comparison.json` beneath that directory. No separate Slider fix is
inferred from the historical inconclusive inputs.

Controlled v27 established LEFT reply without reactions, RIGHT exact thread, text
tap reactions, long press without UIKit Copy and post-dismiss quoted Reply. A known
synthetic photo and video each became one chat message; an independent normally
linked peer verified signed logs, decrypted current File versions, matched exact
hashes and fully decoded both. The JPEG is 8,530 bytes/1200×800; H264 MP4 is
13,609 bytes/640×426/3.000 seconds. Photo QuickLook passed. Historical v27 failures
were video taps also opening reactions, invisible action glass and a handled
concurrent-device rejection shown as raw `stale_epoch`. No edited voice was sent
in that run. These failures keep their v27 attribution.

The v27 peer evidence is `photo-peer-export.json`, `photo-peer-decode.json`,
`video-peer-export.json` and `video-peer-decode.json` under
`/private/tmp/zoen-message-polish-qa/v27/`. Those files supersede its runtime record's
earlier pending-helper marker. Earlier v20/v22/v23 observations retain their
original attribution; v26 failures are not retroactively reported as passing.

The v29 full app check passed TypeScript, Oxlint, Oxfmt and 239/239 tests
(`/private/tmp/zoen-v29-final-app-check.log`). Eleven new shared playback-owner tests
cover explicit intent, late/native resume refusal, pause acknowledgements, rapid
replay and disposed-player ownership. Mounted audio/video share playback ownership;
native audio failure exposes localized Retry while retaining its decrypted lease.
The frozen v28 native production core passed 378/378 and MLS18/18 plus strict Clippy. Its
16-test real WebSocket suite covers handled commit reconciliation without listener
errors, permanent application refusal with failed delivery, and rollback/retry plus
a localized notice after injected storage failure. Exact refusal diagnostics stay
in the outbox/log. v29 adds no wire event or UniFFI ABI change.

After the v29 build, test-only commit `e7097337` changes the `cfg(test)` module of
`crates/client-core/src/net.rs` to model old-epoch application refusal before the winning
commit arrives during retry. Focused net16/16 passes
(`/private/tmp/zoen-v29-net-test-ordering.log`). Independent comparison confirms this
is the only changed input among 742 and its production prefix is byte-identical to
the built source. Installed v29 production is unchanged; no runtime rebuild is
needed. Because the manifest includes Rust tests, its immutable hash identifies
the `d450e37d` artifact rather than the later whole workspace. Full 378+18/Clippy
evidence belongs to those frozen inputs. Exact comparison evidence is
`/private/tmp/zoen-message-polish-qa/v29/post-artifact-test-delta.json`.

Canonical atomic `send_file_message` and durable exact-intent receipts retain six
Rust regressions for destinations, denied/failed writes, cold deduplication and
encrypted chunk replay. Body text avoids UIKit selection menus; native voice
sliders use a continuous 0…1 domain rounded only at the original-frame boundary.
The authenticated exact-device recovery gate preserves the original first DM
through live Welcome/context ordering and cold reopen; fresh-device recovery
remains supported. These source proofs do not replace installed acceptance.

VoiceOver, native media arbitration runtime, waveform/word editing, async drafts,
remaining thread media and comparative quality
remain pending. No full iMessage, WhatsApp, Slack or prior Swift parity is claimed.
