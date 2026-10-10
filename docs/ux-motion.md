# Zoen motion & haptics

Taste over noise. Haptics never fire on scroll. Animations respect Reduce Motion
(fade/opacity instead of springs, morphs and stroke-draw when Reduce Motion is on).
Haptics only feel on a real device — the simulator is silent.

This is the motion/haptic policy. Current implementation evidence and unresolved platform coverage are in [roadmap status](roadmap-status.md). A named policy does not establish that every screen follows it.

## Haptics (`Haptics`)

| Trigger | Haptic | Why |
|---|---|---|
| Tab / segment / chip change | `selectionTick` | Confirms the finger crossed a choice |
| Search open, Back, avatar / profile | `tap` | Small intentional chrome taps |
| Send a message | `send` | The message left your thumb |
| Fan menu open | `open` | The + bloomed |
| Fan menu hover change | `selectionTick` | Crossing wedges |
| Fan menu commit | `commit` | You picked something |
| Fan menu cancel | `dismiss` | Closed without choosing |
| Create a Space | `commit` | A new place exists |
| Store install stamp lands | `commit` | Ink stamp pressed |
| Voice-record lock (slide up) | `recordLock` | Recording is locked in |
| + long-hold → mic morph | `recordLock` | Fan collapsed; mic armed |
| Voice cancel (slide left) | `warning` | Discarded |
| Destructive / denied | `warning` | Pause before harm |

## Signature WOW moments (implemented)

| Moment | What actually happens | Reduce Motion | Repeats |
|---|---|---|---|
| **Store install** | Hand-drawn `InkStampOverlay` (“INSTALADO” / “INSTALLED”) presses onto the row with squash → ink spread + splatter → short line boil. Get pill morphs to Open via `glassEffectID`. Success haptic. | Opacity crossfade, no squash | Stamp only the first install per listing (`WowGate`) |
| **Create Space** | After create: full-sheet `SpaceArtReveal` — doodle stroke-draws (`drawOn`), watercolor wash blooms, then the Space is pushed. | Instant wash + short delay | Every create |
| **First bubble flourish** | Once per chat key, an `InkFlourish` underline draws under the first of your bubbles. | Shows finished path | `WowGate.once` |
| **Message send** | Light send haptic; list scroll with `.snappy`. (Flight matchedGeometry is reserved for the +→Zoen voice handoff into Zoen’s chat.) | Same, no extra motion | Every send |
| **+ long-hold → Zoen voice** | Hold centre of + ~0.7s (after fan opens, within slop): fan collapses, + morphs to mic (`glassEffectID`), recording starts with composer gesture language (release send / up lock / left cancel). On send: mic glass morphs back, real voice goes to Zoen 1:1 via core, **navigate into Zoen chat** (no toast) with send-flight land on the new bubble so Zoen's reply is in view. | Crossfade instead of morph / land | Every hold |
| **Profile sheet** | Avatar/name taps open a medium→large Liquid Glass sheet with zoom transition from the source when available. Message dismisses sheet and opens the 1:1. | Sheet rise only | — |

## Chrome vs content

- **Chrome** is Liquid Glass (`.glassEffect`, floating bars, capsules, sheet background). No opaque band behind bars.
- **Content art** is pen and paper (`DoodleView`, `PaperBackground`, `InkStampMark`, line boil). Never flat AI art.

## Accessibility

- Reduce Motion: springs → fades; stamp squash skipped; doodle draw skipped; +→mic crossfades.
- VoiceOver: + has custom action “Record audio for Zoen” / “Gravar áudio para o Zoen”; profile links expose “Shows profile”; Back is `zoenBack`.
