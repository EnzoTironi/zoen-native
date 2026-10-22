[Native surface index](../expo-native-surfaces.md).

## Voice editing checkpoint

The immutable original-frame edit model and session coordinator pass 40 focused
tests covering trim/cuts, restoration, transcript rebasing, undo/redo, obsolete
exports, cancellation and cleanup ownership. The actual macOS AVFoundation harness
encodes and decodes exactly66,150 retained frames at44.1kHz (1,500ms): synthetic
440/880Hz regions survive, the removed1,200Hz region does not, and contained15ms
fades preserve frame counts and original hashes. The real Swift module compiled in
signed iOS v17 and updated v21/v22. Current v39 app source7e068acf passes TypeScript, Oxlint,
Oxfmt and260/260 tests, including16 owner-lifecycle,6 scoped-search and9 temporary
conversation lifecycle regressions in addition to the40 edit/session/suggestion tests.

The 40 pure checks comprise 18 edit, 14 session and 8 suggestion tests. Explicit
word remove/restore commands are idempotent against the current edit: a queued tap
after an accepted batch cannot restore audio accidentally or add an empty undo step. Native
regressions also clamp extreme/invalid Speech times before numeric conversion and
bound analysis to a duration-aware 60–900 seconds. Text fragments may be phrases;
controls retain their exact ranges and never invent inner word times. Optional
hesitation cuts require complete retained fragments; pause proposals use intact
original-adjacent timing gaps over 1.2 seconds and retain 300 milliseconds. Such
gaps may contain other sounds. Nothing is removed automatically, proposals are
revalidated and each accepted batch is one undo step.

Signed v22 imported the known synthetic 5.54-second Portuguese audio through the
ordinary file picker. Its attachment chooser used a sheet with a finished-dismissal
callback before presenting another picker. Signed v29 replaces the initial iOS
chooser with a native SwiftUI Menu anchored to +, keeping the existing permissions,
file picker, review, encrypted send and draft-target guards. The root-reviewed v29
menu and ordinary audio import passed; subsequent native media pickers and detailed
editing retain their appropriate platform presentations.

Current source defaults to a compact audio strip above the normal input. Dragging
up or its accessible expand control opens the detailed editor; collapsing preserves
the same draft/edits, while explicit discard releases owned files. Recording and
preview retain their send/cleanup fences. No transcription or removal starts
automatically. Controlled signed v29 passed visible 0.98-second start trim and Apply
to 4.56 seconds from the 5.54-second fixture, Undo/Redo, compact/detail retention,
native preview control and one compact edited-voice Send. Independent current signed
metadata/decryption/full AAC decoding passed: 4,557 milliseconds and 200,985 sample
frames/4.557482993 seconds at44.1kHz, with original hash unchanged. No human listening
or transcript access was performed. On-device recognition, waveform/word gestures,
thread audio and full comparative Swift parity are not
established. Historical v27/v28 trim/Undo input targeting remained inconclusive;
no separate Slider correction is inferred from the later controlled pass.

Signed v39 keeps the default audio strip compact. The expanded editor now uses
44-point native icon buttons for Undo/Redo, with full accessible labels, and a
native disclosure for the four precision sliders, closed initially. Pending Apply
and actual selection Cut/Restore actions remain outside that disclosure. The
transcription section precedes optional suggestions. Recognizer fragments wrap as
native controls using only original exact ranges; phrase fragments are not split
into invented word times, and text-only results explain the missing timing.

Actual controlled v39 accepted the ordinary native Menu/system Files import, showed
the waveform, transcription and Send within the initial detail viewport, expanded
the precision controls, applied a5.54-to4.73-second trim, undid to5.54 and redid to4.73.
Collapsing and dragging upward preserved that edit and session. Native preview
Play/Pause and explicit discard passed, with no sends or microphone capture.
An explicit local Speech request returned unavailable on the simulator; the generic
English/Portuguese capability copy is accurate. Successful recognition, timed-fragment
layout/touch and physical tactile quality remain unproven. The6 new app regressions
also cover swipe threshold rearming and live reduced-motion feedback. No additional
receiver-byte or full-decode proof is claimed; original v29 evidence remains scoped
to its original messages. Runtime receipt and named screenshots:
`/private/tmp/zoen-message-polish-qa/v39/`.

The signed749-input v39 archive was preserving-installed and exact executable,
JavaScript and strict signature-verified on the isolated simulator and the simulator
used by the user. The latter retained four Home cards,6-of-8progress and conversation
history; fresh chat reached100%, and its last audio/receipt cleared the composer.
Its native attachment Menu opened/dismissed without selecting a resource. Normal
keyboard focus and interactive hide passed on isolated v39; the more extensive v38
motion/reversal/rotation video evidence keeps its original attribution. Full iPhone
product, supported-device Speech/provider, accessibility and performance acceptance
remain pending.
