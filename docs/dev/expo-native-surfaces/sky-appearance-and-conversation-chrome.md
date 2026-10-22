[Native surface index](../expo-native-surfaces.md).

## Sky appearance and conversation chrome

The iPhone appearance setting is **Your sky / Seu céu**. Six curated phases replace
the light/dark/system control: deep night, dawn, morning, day, sunset and night.
Automatic follows the device's local civil clock, using anchors at 00:00, 05:00,
08:00, 13:00, 18:00 and 21:00. It does not request location or use weather or solar
services. Existing light/dark/system preferences resolve to day/night/automatic;
the existing serialized preference writes remain authoritative.

The shared backdrop uses bundled photorealistic sky textures over quiet Fabric gradients, with a bounded radial haze. The photos are cropped to a small patch of sky, enlarged 1.8× and blurred natively by 20 points; illustrated stars and celestial objects are absent.
Native Stack headers are transparent, with blur, shadows and scroll-edge fills disabled. The per-scene sky continues behind navigation and scroll content; native Back, glass title/actions and system search remain UIKit controls. Static Search and Forward fields reserve the actual native header height, while scroll-first screens use automatic insets without adding that height twice. Content surfaces retain their own contrast. The selector uses native Expo UI rows, photographic previews and
an explicit dismissal action. The selected phase also sets UIKit appearance, so
native sheets, buttons and glass agree with the sky.

The clock samples only while Automatic is active and the app is in the foreground.
Normal samples can fade on the native driver after checking the outgoing and
incoming color envelope. Manual selections, clock/timezone jumps, foreground
resumption, scheme changes and reduced motion update immediately. The backdrop
has no perpetual animation and cannot receive touches or accessibility focus.
Eight deterministic tests cover all 1,440 local minutes, gradient/haze/photo contrast,
alpha-composited surfaces, paired controls, midnight, preference migration and
clock/transition guards. Photographic overlays use 28% intensity in the four light
presets and 20% in the two night presets; automatic twilight can reduce intensity
to preserve contrast. All semantic inks are checked against arbitrary image pixels.
Native tab controllers paint their own scene, so each tab also hosts its backdrop
inside that scene; relying only on a root underlay left a white canvas in v10. These checks complement installed iPhone visual QA.

Conversation headers preserve UIKit Back and group avatar, name and disclosure in
a native glass title. Native phone/video buttons remain disabled because the core
has no calling operation; the adjacent magnifier opens real search scoped to that
conversation. The composer uses native circular glass buttons and a growing field:
empty input offers voice recording, while text offers Send. Existing attachment
review, recording lifecycle, encrypted send and draft retention remain in place.
Delivery marks use vector symbols and canonical core acknowledgements: pending
clock, one sent check, two delivered checks, and blue checks only after Read.
Local-only and failed messages have distinct device/warning symbols.

The composer overlays the full timeline in main chat and threads. Its wrapper is
transparent, with no full-width blur or painted panel; the existing native glass
input and controls retain their own material. On iOS, the native chat scroller
reserves the measured composer height while native bottom layout follows the existing
keyboard clock; the v56 architecture is detailed above. Older history can scroll
behind the controls. The composer is above the fixed rail in stacking order. Own
bubbles are white/black text in light skies and black/white text in dark skies; received bubbles use neutral #E9E9EB/#262628. Actual signed v20 light-sky
capture confirms own white bubbles, gray received messages and history behind the
transparent footer. A later signed v22 capture confirms black own bubbles with
white text on the night sky, hidden cards while typing and approximately 8.7 points
between the composer and keyboard. Interactive restoration, multiline/thread and
cold acceptance remain separate from that bounded screenshot.

The card rail follows actual keyboard move/interactive/end frames. Cards stay
mounted, preserving horizontal position; hidden cards lose touch/accessibility
targets. Reduced motion removes their extra slide/fade, and notices remain visible.
The earlier keyboard-avoider/footer implementation and its v20/v22 screenshots
are historical observations, not acceptance of the current iOS keyboard layout.

Conversation search reuses the actual message input rather than replacing the
header. The header magnifier switches that same field into search; the microphone
slot becomes older/newer arrows; a completed search with a nonempty query and
zero hits offers Send to Ask Zoen. Loading, failures and stale results never admit AI. Canonical paginated hits
reveal exact message IDs, including thread replies temporarily rendered in the
same conversation. Closing restores the normal field, saved text and audio owner.
Only an opaque ticket enters navigation; the question remains in memory. The
earlier v23 header-search observation is historical and does not validate this
new composer layout.

Ask Zoen uses real Apple Foundation Models when the current iPhone supports and
enables the model and language. It is a bounded, read-only temporary conversation,
not a new durable Agent, Space or chat message. Each turn reads a current bounded
canonical text excerpt and rechecks membership and device/session ownership after
generation. Closing or erasing cancels and drains the lease. Unsupported hardware,
disabled/downloading models and unsupported languages produce truthful capability
states; there is no remote or canned-answer fallback. A root-reviewed signed v23
capture during human use shows a Portuguese model reply grounded in the limited
text excerpt. That observation is not a controlled known-query or physical-device
acceptance run.

The same message field and MediaComposer stay mounted while search changes the
controlled input and hides the audio review, preserving staged audio and text.
Entering search stops a live
recording into its retained draft; asynchronous recording startup checks suspension
after permission and preparation, preventing a hidden recording from starting later.

Conversation miniapps stay fixed below the measured native header, with 6-point
rail padding. Their wrapper is a transparent absolute overlay; messages scroll
behind the cards and remain visible in their gaps, without a fade, mask or painted
background. The list reserves the measured rail height only at its content start,
and message jumps offset below that overlay. In signed v19, two actual screenshots
during human scrolling show timeline96%→27% while all three card frames stay at
normalized y0.140/height0.174. No automation swipe was performed during that user
interaction. Earlier v17 independently confirmed fixed frames across one bounded
automated swipe. Final keyboard/jump, isolated pin/unpin and cold acceptance remain
pending.
