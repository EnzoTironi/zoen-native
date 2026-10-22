[Native surface index](../expo-native-surfaces.md).

## Additional media checks — signed v56 and isolated v57

On signed v56, ordinary forwarding creates one video message in Forward QA 1018.
Native AVKit opens inline and fullscreen, pauses, seeks from 0 to 2 seconds while
paused, and resumes to 3 seconds with zero remaining. Player touches do not open
message reactions. This is bounded native playback evidence, with earlier peer
hash/decode proof retaining its v27 attribution. Cold retention was not checked
in this run. Evidence: `/private/tmp/zoen-media-forward-qa/v56/runtime.json`.

Isolated signed v57, source `4104323a`, imports a synthetic two-page PDF through
the native Files picker, sends it once and forwards it once to Forward QA 1018.
Quick Look displays both complete pages and the expected totals 53 and 161.
However, Share → Save to Files remains at the system loading view. Source
inspection establishes a separate concrete defect: Expo's presentation Promise
resolves before dismissal, and the old adapter immediately releases the temporary
file. The loading symptom is consistent with this defect; its OS cause is not
independently established. V57 is not accepted or promoted to the human simulator.
Evidence: `/private/tmp/zoen-media-forward-qa/v57/runtime.json`.

V57 also restores canonical file captions below media, with the existing
tap-to-react and hold-for-actions behavior. All 421 application checks pass,
including six compiled-render regressions; suppressing the caption fails the
negative control. A normally registered synthetic peer sends a signed edited-file
caption, which appears below the received PDF. One ordinary forward retains that
caption on the outgoing side. Diagnostic navigation timing chose QA Agente 1018,
so this does not claim the intended Forward QA destination. The unsafe recorded
selectors are retired, with no formal replay claim. Evidence:
`/private/tmp/zoen-file-caption-qa/v57/runtime.json`. Cold retention, other media
captions and broader accessibility remain pending.

At v57/v58, native Markup persistence remained a parity gap: the prior Swift client
created an edited copy and saved a canonical file version, while a read-only Quick
Look preview did not establish that flow. The bounded v59 journey is recorded below.
