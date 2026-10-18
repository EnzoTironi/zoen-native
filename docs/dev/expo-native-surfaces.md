# Widgets, Live Activities and App Intents

Source audit: 2026-10-18. Current acceptance priority is iPhone; Web and Android follow later. The signed ARM64
iOS simulator Release has compiled the widget extension, Live Activities and App
Intents metadata. Signed v7 accepts the redacted Portuguese Widget gallery preview. Signed v40
accepts a private Live Activity on the Dynamic Island and Lock Screen, warm and process-terminated
cold compact taps, Lock Screen navigation and explicit ending on the isolated QA simulator.
Signed v50 accepts the bounded conversation-card journeys and preserving simulator
delivery described below; its installed hashes and strict signatures match the archive.
Signed v51 adds bounded isolated reaction-participant, Unicode emoji and text-forwarding
acceptance. Signed v52 accepts the subsequent native contrast polish, skin-tone reaction
and cold retention on the isolated simulator. The human simulator also has verified v52
through a preserving install, with bounded retention and native emoji-sheet acceptance.
Selected widget content, Siri and canonical-version-update-to-OS-tap remain pending.
Installed packages and successful compilation do not establish those journeys.

## Signed v40 checkpoint

Home favorites now support a native iOS organizer with drag handles, card-menu ordering and
pinning directly from miniapp detail. Ordered ID-only choices retain the captured account,
device and mode, validate the visible permutation when queued work executes, and preserve
the last committed order on save failure. Actual isolated drag, menu movement, detail pin
and cold retention passed; independent conversation-rail pinning was outside this checkpoint.

Live Activities store a random session link in their existing owner-scoped preference.
Expo SDK58 keeps the start URL in immutable ActivityKit attributes; content updates cannot
repair a version-bound URL. `zoen.openActivity` therefore rereads the canonical item and
admits only the still-running selected session with the same item/end and exact current
membership. Widget and shortcut links keep their strict version checks. Cold activity links
wait for preference restoration; legacy selections without a session link retire without
discarding unrelated widget/shortcut choices. Production provider, route and Siri access
share the same membership projection, excluding cached non-member conversations.

Signed Release v40 source `5ac2cb7a` has 755 frozen inputs and passes 296 application tests,
TypeScript, Oxlint and Oxfmt. It is archived and preserving-installed on both QA simulators;
installed executable/JavaScript hashes and strict signatures match. The Live Activity check
created one synthetic countdown through the ordinary catalog, accepted the normal OS prompt
only on the disposable simulator, and kept private content hidden. It did not donate widget
or Siri content. Same-end version updates and access/session races have deterministic
production-helper/adapter coverage; they were not exercised as OS mutation-to-tap journeys.
Actual successful supported-device Speech, accessibility/performance and full parity remain
pending. Evidence: `/private/tmp/zoen-home-qa/v40/runtime.json` and
`/private/tmp/zoen-iphone-release-v40-verification.json`.

## Signed v50 conversation card preferences

The implementation adds separate presentation choices for each conversation. New
canonical miniapps appear in its rail by default; hiding a card removes only that rail
entry. It preserves the miniapp, its message card and history, and Home favorites.
The native card menu offers Hide in this conversation and Organize cards. Miniapp
detail offers Show/Hide and the same organizer; conversation details keep the organizer
reachable when every card is hidden. The native organizer separates visible and hidden
cards, with restore controls and drag handles for visible cards.

`ConversationCardsController` persists bounded, versioned Item-ID-only hidden/order
choices per Space in an identity/device/mode-scoped preference. Queued writes revalidate
the captured owner, exact current membership and canonical items; invalid permutations
are rejected, failed writes preserve the committed preference, and device erasure awaits
cleanup. Demo choices remain volatile. Home and conversation cards share the canonical
Home item source and membership projection rather than maintaining another content cache.
Restoring an owner waits for that owner's pending writes before reading its preferences.

Rendering uses subscribed immutable preferences, canonical items and membership. Both
native organizers give the default recycled, movable `List.ForEach` stable Item IDs and
resolve row content through the current canonical map. Successful saves do not remount
the movable rows. The conversation's movable `ForEach` is a direct child of `List`, with
its native section heading above the list; hidden cards use a separate static section.
Only failed saves change the native List key to restore its committed visual order. Busy
buttons and queued owner/scope guards serialize edits without transient row move metadata.
That checkpoint's full check passes 320 application tests, TypeScript, Oxlint and Oxfmt. Two
actual compiled-hook regressions fail against v42 and pass with the immutable projection.

Earlier signed candidates exposed distinct native failures: v41/v42 could retain stale
cards after a successful save, later hide/restore checks updated cards but lost drag
handles or move callbacks, and v48 Home saved a reordered preference while a success-key
remount blanked its rows. These failed candidates informed the final source design; they
do not establish drag or cold-relaunch acceptance.

Signed v50 source `4ef91160` removes the temporary QA traces and passes the final 320-test
check. On the isolated simulator, actual native Home swap-and-restore retained all three
rows; conversation swap-and-restore retained both rows and drag handles. Hiding two cards
down to none and restoring both updated immediately and retained native drag handles.
A cold restart retained Home's `[Recipe, Pet, Trip]` order. Final cold-conversation
checks retained Recipe left of Pet, while the hidden QA countdown remained visible as a
canonical message card in history. The conversation opened at 100% with an empty composer.

The user's simulator received v50 through a preserving install after an idle/empty-draft
preflight. Installed executable/JavaScript hashes and strict signature match the archive.
Normal restart retained four Home cards, Recipe's 6/8 progress and existing chat history.
Fresh chat entry retained its audio/receipt above the composer by approximately 36 points.
The actual native card menu exposed Hide in this conversation and Organize cards; the
organizer opened with four rows and drag handles, then Back returned to the latest history.
No hide, reorder, pin, content or audio-send mutations were performed on that simulator.

The 13-step primary-simulator diagnostic contains one failed short-label wait, followed
by a passing corrected menu-label wait, and four cross-tree warnings. It is not a clean
formal full-flow replay. Installed failure-injection rollback remains unverified. The v40
Live Activity evidence keeps its original attribution, and full product parity remains
unproven.

## Message care — signed v52, bounded native acceptance and delivery

Source `8905754e` changes a reaction-chip tap from an immediate mutation to canonical
participant details. The native sheet shows the selected emoji's people and identifies
the current person as You / Você. Adding, replacing or removing a reaction requires an
explicit action. Participant details and full emoji entry share one sheet, retaining
the current message, owner and access checks across the transition.

The emoji field accepts a single Unicode emoji sequence through the system keyboard
or paste, including flags, skin tones and joined sequences. The OSS `emoji-regex` parser
preserves the selected sequence and enforces the core's 32-byte UTF-8 annotation limit;
it does not split a reaction into code points or silently truncate it. The existing
quick palette remains available.

Forwarding now captures identity, device, mode and source message, lists only current
member destinations and rereads the canonical source message before issuing the command.
A synchronous latch prevents duplicate callbacks within the current focus. Source and
target membership are rechecked before delivery and completion. Back, unmount, refocus
and access changes retire the navigation intent; a forward already accepted by the
core continues, while its late completion cannot navigate or update an obsolete view.
Twenty-five focused tests cover delayed preflight/completion, duplicate taps, owner and
membership changes, canonical deletion and retries. Three emoji tests cover complete
Unicode sequences and the byte bound. These are source proofs, not native induced-delay
acceptance. Source `ef7acda9` passes the final 348-test check with TypeScript, Oxlint and
Oxfmt (`/private/tmp/zoen-message-care-v52-app-check.log`).

Signed isolated v51, built from `8905754e` with 769 frozen inputs, passed installed
executable/JavaScript hash and strict-signature checks. Actual ordinary native UI accepted:

- Tapping the existing own 👍 chip opened its canonical participant name and Você without
  changing the reaction. Choosing another reaction switched that same sheet to emoji entry.
- Pasting 🇧🇷 through the native TextField's ordinary Colar action and selecting Reagir
  replaced 👍 with the flag chip in the conversation.
- Tapping the flag chip and explicitly choosing Remove returned to the conversation with
  no reaction chip.
- Long press, Forward and destination selection created one new visible forwarded text
  message `01M56G8580P27Y5XNWSJ4V0YK0` from source
  `01M56FKDP28Q8SEK4BZYDEABDY` in the new self-only `Forward QA 1018` conversation.
  No other members or contacts were selected. This establishes the bounded text-forward
  journey, without claiming cross-chat media or independent peer-byte verification.

V51 also exposed poor native emoji-field contrast and white text on the lime filled
button. Source `ef7acda9` gives the field explicit theme ink, placeholder and surface,
and the filled action its semantic contrast color. Signed v52 from that source has 769
frozen inputs; exact installed executable/JavaScript hashes and strict signature passed
on the isolated simulator. Actual bounded native checks accepted:

- Text tap opened the quick palette; More waited for the real UIKit dismissal before
  opening the native emoji sheet. The semantic field surface, placeholder, 22-point typed
  text and filled-button contrast rendered correctly.
- Invalid `hi` exposed the unsupported-input error. Clearing it, then using the ordinary
  system Colar action to paste 👍🏽 and selecting Reagir produced that own skin-tone chip.
- Tapping the chip opened its canonical people and You / Você. Closing the sheet left
  the reaction unchanged.
- Cold restart, Home, Chats and `Forward QA 1018` retained the exact single forwarded
  message `01M56G8580P27Y5XNWSJ4V0YK0`. Returning to Message Studio retained its original
  root message and 👍🏽, opened at 100%, and kept the composer empty.

The 34-step v52 diagnostic has eight cross-tree warnings and no failed waits. It is a
bounded native observation, without a polished formal-replay claim. V52 captures and
records are under `/private/tmp/zoen-message-care-qa/v52/`; build verification is
`/private/tmp/zoen-iphone-release-v52-verification.json`.

Human E2B received v52 after a fresh known-idle, empty-draft preflight on v50. Preserving
installation exited successfully, and exact installed executable/JavaScript hashes and
strict signature passed. Normal restart retained four Home cards and Recipe's 6/8 progress.
Chats to Bilingual QA reached the latest history at 100%; the existing audio/receipt ended
at 0.854 above input top 0.897, approximately 36 points clear. Its audio actions, More and
native emoji sheet opened, then closed without typing, reacting, sending, importing or
changing preferences or account data. Return retained 100%, an empty composer and the
audio clearance. The 12-step human diagnostic has three cross-tree warnings and no failed
waits; it is bounded delivery evidence rather than a clean formal replay. The isolated
and human simulators now both run accepted v52; earlier v50 delivery retains its original
historical attribution.

Native captures and accessibility records are under `/private/tmp/zoen-message-care-qa/v51/`,
including `reaction-participants.png`, `flag-input-before-polish.png`, `forward-success.png`
and their text records. The 28-step diagnostic in `flow-summary.txt` retains raw steps and
cross-tree warnings; it is not a clean formal replay. The signed build receipt is
`/private/tmp/zoen-iphone-release-v51-verification.json`. Deliberately delayed native
forwarding races remain untested; their 25 regressions are source proof. Cross-chat voice
forwarding remains unsupported by the core. Successful supported-device Speech,
group-message sender profile entry, VoiceOver, performance, broader message journeys and
complete Swift/WhatsApp/Slack parity remain pending.

## Apple baseline

`apple/Shared/Widgets/WidgetSnapshot.swift` validates seven content templates: stat,
progress, countdown, list, caption, ticket and photo. `SnapshotCard.swift` renders them
inside the app. `ChatTiles.swift` adds local presentation tiles and pinned strips.
`AppModel.swift` stores Home ordering/unpinning and chat tile ordering in UserDefaults.
Conversation pin/archive/mute preferences separately use the Rust Personal Space.

There is no WidgetKit extension target in `apple/project.yml`, or an ActivityKit,
AppIntent or AppShortcuts declaration in the baseline Swift sources. Comments describe
future widget-extension reuse. The in-app pinned-card journeys therefore do not prove
a launcher widget, Live Activity or Siri integration.

The portable `roda-apps::snapshot` supplies each built-in miniapp's canonical compact
content. Native `ItemDetail.app.snapshot_json` already exposes it. It must remain the
source of displayed values; an extension must not recreate pet, poll, schedule or
approval business rules.

## Verified SDK58 surfaces

`expo-widgets ~58.0.20` supports a pure `'widget'` layout using Expo UI SwiftUI views,
snapshot/timeline updates, configurable widget parameters, and Live Activity factories.
That layout runs in an isolated extension runtime with no hooks, asynchronous work or
access to the application engine. All required data enters through bounded props.
Its button callback can update local props while the app is closed; the application
interaction listener only runs while the app process is alive. Neither establishes a
signed Zoen mutation. [Expo Widgets SDK58](https://docs.expo.dev/versions/v58.0.0/sdk/widgets/)

`expo-app-intents ~0.5.9` is alpha. Real AppIntent, AppEntity, EntityQuery and shortcut
types must be declared in Swift compiled into the app target using Expo inline modules.
JavaScript cannot register those types or phrases dynamically. The invocation queue is
persistent and at least once; dispatch does not await a JavaScript result. On-screen
entity associations additionally require Xcode 27, and the relevant OS availability.
[Expo AppIntents SDK58](https://docs.expo.dev/versions/v58.0.0/sdk/app-intents/)

Live Activities fit ongoing events with a defined end, generally within eight hours.
Private content should remain in the application; Lock Screen information is visible
to nearby people. A distant trip countdown belongs in a widget rather than an activity
kept alive for days. [Apple ActivityKit](https://developer.apple.com/documentation/activitykit/displaying-live-data-with-live-activities),
[Apple Live Activities guidance](https://developer.apple.com/design/human-interface-guidelines/live-activities)

## Shared implementation checkpoint

`apps/zoen/src/native-surfaces/snapshot.ts` checks snapshot item ID, app ID and deep link
against the actual Item, with bounded JSON/text/arrays and known local art. It produces
an immutable item/version/account/device projection. The default projection omits names,
values, rows, bars, artwork and domain actions. Explicit content selection exposes only
the compact content, never `view_json`, page bodies, account keys or database paths.

`catalog.ts` publishes at most 50 explicitly selected, currently accessible item IDs and
titles, with account/device/version metadata. The default selection is empty and
`hideInSpotlight` remains true. Callers must replace catalogs with an empty catalog on
sign-out or access loss. These navigation intents are not donation-enabled, so there
are no registered donations to remove. Calling the SDK's donation deletion API for
their names fails even on a fresh install.

`intents.ts` accepts navigation-only `zoen.openItem`, `zoen.openChat` and the app-owned
`zoen.openActivity` session link described above. It validates the invocation
age, exact parameter shape, current account/device, item version and current Space
access, checking the owner again after asynchronous reads. Unknown names and mutations
are rejected. This is repeatable navigation; the native queue may acknowledge only an
actual `opened` result. Invalid/retired actions need an explicit discard policy; transient
failures stay pending. App-owned Swift declarations now expose these read-only actions with local device authentication and opening dialogs. A single ready-session root consumer handles pending navigation; unknown actions and transient/foreign-owner failures remain pending, while known invalid/expired or stale-version requests are retired. Native metadata compilation and actual queue journeys remain pending.

`miniapp-widget.ios.tsx` is a real SDK58 pure layout registered as `ZoenMiniapp`, registered by the source-controlled CNG plugin. Signed v7 compiles the extension and its actual Portuguese redacted gallery preview
renders. Selected-content projection and refresh acceptance remain pending. It uses native Text timers, progress,
rows, rendering-environment adaptation and `privacySensitive` content. It has no local
mutation buttons. The text layout does not claim parity with animated baseline artwork
or the baseline's extra hike/ticket tiles.

Nine focused tests cover snapshot binding, unsafe content, bounded projection, default
redaction, explicit catalog selection, stale/foreign/unauthorized navigation and account
switches during asynchronous reads. They exercise shared code, not an installed extension.

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
reserves the measured composer height while the sticky composer follows the
keyboard; the current architecture is detailed below. Older history can scroll
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

## iOS keyboard checkpoint — signed v38, bounded runtime pass

On iOS, `KeyboardChatScrollView` owns native keyboard insets and reserves the full
measured composer height: 96 points in the controlled fixture. Supported
`KeyboardStickyView` movement, the 26-point safe-area correction and inline Expo UI
Hosts ignoring the keyboard safe area keep layout stable without per-frame
JavaScript measurement. The initial end anchor observes native content/viewport/
inset/Y before FlashList suppression, deduplicates within one point and cancels
on first focus, drag, native user scroll, search, reply/edit/jump, blur or scope
change. It uses layout/scroll events and cancellable frames, with no fixed timer
or private API. Other platforms retain their fallback.

Signed v38 source `f7682098` has 749 verified inputs, manifest SHA256
`d54125cc4742b81ec7318f520797c41f25f03d18ad725f8959bd151ec6219d34`.
Build, archive, strict signature and exact preserving-installed executable/JS
verification passed on isolated 388CA and human E2B. JavaScript is exactly v36's
6,406,723-byte bundle, SHA256
`9b5853ec029bd5f3edaf2b477df4cc9577c86bd8c856fc1d6eaf42d80ccb092d`.
The full 254-test app check, TypeScript, Oxlint and Oxfmt passed
(`/private/tmp/zoen-v38-pr1608-app-check.log`). Actual bounded outcomes:

- Two fresh entries without swiping reached 100%; reply badge bottom 0.870 cleared
  input top 0.902. Portrait field Y was 486 points, height 52 and keyboard top 546:
  an 8-point gap. Landscape field Y was 118 points, height 52 and keyboard top 178,
  giving the same gap.
- Normal opening/hiding and immediate interactive dismissal passed observation.
  Reviewed video `1792284820127` shows continuous field/keyboard progression
  without the earlier fixed-field/jump. Video `1792284864960` supports two settled
  partial/reverse trials, including a repeat after 20 seconds, with the keyboard
  remaining open; settled dismissal passed the structural flow. Keyboard-open
  landscape rotation and portrait return passed. Transient 15–25-point gaps remain;
  no perfect timing or full motion/performance claim is made.
- After user gestures and rotation, 96% is the retained reading position, not a
  latest-message assertion. Actual zero-result search/fragment replay belongs to
  v36: `qazxv36missing` exposed AI without invocation or send, and closing restored
  the empty draft. The identical v38 JavaScript does not make this a v38 rerun.

Exact metrics, recording paths/hashes and limitations are in
`/private/tmp/zoen-message-polish-qa/v38/runtime.json`; signed artifact and delivery
proof is `/private/tmp/zoen-iphone-release-v38-verification.json`. Human E2B received
a preserving verified v38 update after a fresh, user-confirmed idle preflight.
Restart retained four Home cards, 6/8 progress and Bilingual QA history. Fresh chat
without a swipe reached 100%; existing audio/receipt bottom 0.856 cleared input top
0.897 by about 36 points. The native anchored Menu opened/dismissed without selection,
imports, sends or account mutations. Its v32 delivery is now a preserved historical
receipt, not the current human artifact. Original encrypted edited-voice and selected
thread-photo proof still belongs to v29; those media were not resent here.

The earlier isolated trials retain their bounded attribution: v33 failed with a
shrinking composer; v34 passed one fresh entry and a 49–50-point opening message
gap but native controls double-avoided the keyboard; v35 passed control alignment
and Menu opening but failed fresh entry at 86%. V36 passed three fresh entries,
8-point open-keyboard geometry and the search fragment, then failed interactive
dismissal. V37's custom `Animated.View` consumer of existing move/interactive/end
events reproduced that stall and was rejected. Card opacity contributed to late
reappearance; those videos alone do not prove frozen native keyboard events.

V38 retains precisely the two Swift files from open, unreviewed upstream
[PR #1608](https://github.com/kirillzyusko/react-native-keyboard-controller/pull/1608),
commit `daf3c0081c891d442cf84cccda6ef36226276de1`, related to
[issue #1563](https://github.com/kirillzyusko/react-native-keyboard-controller/issues/1563).
Patch SHA256 is `cfd86881125882e3f46edb8f501af748b6dfb30b79d463b85111f6dbe7f1439a`.
Pinned `patch-package` 8.0.1 applies it via `postinstall` command
`patch-package --error-on-fail`; exact application/reverse/reapplication were verified.
No extra position math or private API was added. These observed paths support
retaining the pinned native backport, without establishing the application's exact
root cause or upstream approval. Physical devices and future upgrades still need
verification. SwiftUI virtual accessibility frames can be stale after ancestor
transforms despite working Host hit tests/Menu; actual VoiceOver impact remains
unconfirmed. Full accessibility, motion/performance, thread/multiline/cold and
comparative product acceptance remain pending, with no blanket parity or CI claim.

## Historical initial-position checkpoint — signed v32, targeted runtime pass

Root observed initial latest-message/composer overlap in v29 on both QA accounts.
Signed v30's first-positive-composer gate and v31's positive initial footer spacer
both left the controlled main chat at 97%, with the reply badge ending 0.932 below
the input's top 0.902. A settled 30-second v30 capture was unchanged. Native model and
visible/AX frames differed by 54 points; that difference and source ordering do not
establish the full cause. These remain historical failed attempts, with evidence in
`/private/tmp/zoen-message-polish-qa/v31/runtime.json` and their artifact records.

Historical v32 source `994ec452` admits one cancellable animation-frame native
nonanimated end request after the actual native footer, list load and content size are ready, only
on normal fresh main-chat entry. User drag, search, reveal/jump, reply/edit, owner or
scope changes and unmount skip or cancel it. There is no fixed timer; ongoing
Reanimated footer updates, keyboard ownership and message/search anchors remain.
Six admission/cancellation regressions extend the full app check to 245/245, with
TypeScript, Oxlint and Oxfmt passing
(`/private/tmp/zoen-v32-initial-native-end-app-check.log`). V32 reuses the exact v30
rebuilt native core with unchanged generated Swift ABI
(`/private/tmp/zoen-iphone-v32-core-verification.json`).

Signed v32 build, strict signature, stable 743-input verification, archive and
preserving isolated installation passed; installed executable/JavaScript match.
Manifest SHA256 is
`38b119b0e852645114b94726a690785efa1337ee72141fdedaaf74c7b166a038`;
exact hashes are under `apps/zoen/acceptance.json → previousControlledV32IphoneArtifact`, with
`/private/tmp/zoen-iphone-release-v32-verification.json` as build/install evidence.

Actual same-history main-chat fresh entry without a swipe reaches 100%. The final
View 1 reply badge at y 0.820/height 0.050 ends 0.870, above composer top 0.902: a 0.032
window gap, approximately 28 points. Root reviewed
`/var/folders/9b/t_cg13vn2fz15fy_n31pj6jm0000gn/T/simserver-pgAu2y/media/730184000-1792281825745.png`.
The exact history passed two fresh entries and a 30-second settled read, all 100%
without swiping. Named `fresh-entry.png`, `settled-entry.png` and
`second-fresh-entry.png`, plus `runtime.json`, are under
`/private/tmp/zoen-message-polish-qa/v32/`. This is targeted initial-layout acceptance;
broad long/tall content, keyboard, accessibility and comparative acceptance remain
pending.

Human E2B also received a preserving verified v32 archive after a fresh idle Chats
preflight. Installed executable/JavaScript and strict signature passed. Restart
retained four canonical Home cards/progress and the Bilingual QA conversation/history.
Fresh entry without swipe places the existing audio/receipt bottom 0.876 above input
top 0.897; scrollbar 99% initially and 100% settled with the native Menu open. Menu
opened/dismissed without selection, imports, sends or account mutations. Named
`human-home.png`, `human-fresh-entry.png` and `human-native-menu.png` accompany the
runtime record; delivery checks are in the verification JSON `human_installation`.
Earlier independent media proof below belongs to actual v29 under
`previousControlledV29IphoneArtifact`; those media were not resent in v32.

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
`crates/roda-ffi/src/net.rs` to model old-epoch application refusal before the winning
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

## In-app Home and miniapp cards

The source now defines four native tabs: Home (`/`), Chats (`/chats`), Files and You.
Activity remains a root Stack screen at `/activity`; the Home menu always opens it,
and a review tile appears when canonical requests are Pending or Stale. Chats aligns
its heading and native glass search/compose capsule in one row, with the same
18-point top and 20-point horizontal content padding as Home. The whole heading and
miniapp rail live inside the automatic-inset list; the separate Stack header is
hidden. There is no expanded search field. Search opens the actual `/search` route,
and the cards retain separate spacing below the heading.

`HomeProvider` is mounted under the canonical client. It reads actual `items` with
miniapp content and filters them against currently known conversation membership.
Personal or unknown cached spaces remain hidden until the core supplies an explicit
authorization projection for them.
Home renders explicit favorites followed by remaining miniapps; Chats renders the
accessible miniapps. An individual conversation rail filters by its own Space and the
independent hidden/order preferences described above.
Compact cards render the canonical snapshot using bundled Zoen artwork and open
`/miniapp/[id]`. Invalid snapshots have a bounded title/headline fallback. These
in-app cards do not require external widget-content consent and do not donate titles,
start Live Activities or publish anything to the system launcher.

Photographic cards preserve a clear image hero and place every caption/value in a
lower68–82% black gradient region. A contrast regression checks primary/secondary
inks against arbitrary image pixels, including pure white. Photo press feedback
uses scale rather than alpha to preserve text contrast. Installed visual acceptance
on actual photographic cards remains pending.

Favorites persist only Item IDs in a versioned device-local preference namespace
bound to identity and device. There are no implicit default pins. Restore/read/write
results are fenced across account changes; failed pin writes retain the prior pins,
and device erasure awaits their cleanup. Demo pins are volatile. Home's native organizer
and card-menu movement persist an explicit favorite order, accepted with isolated drag
and cold retention in v40. Independent per-conversation choices are implemented above,
with v50 isolated drag, hide-all/restore and cold-conversation retention checks.
Conversation pin/archive/mute commands retain their own meaning.

Home uses native SwiftUI add and per-card pin menus. Add opens the existing miniapp
creation flow, new conversation, Activity or search. On iOS 26+, the native tab bottom
accessory provides a stateless Search Zoen launcher while Home is selected. It stays
mounted, and the SDK58 documented `unstable_nativeProps.ios.bottomAccessoryHidden`
host option hides the entire accessory on other routes. Merely conditionally removing
its children left an empty rounded native plate on Chats in installed v12. The
installed React Native Screens host maps this flag to UIKit `setBottomAccessory:nil`,
rather than hiding only the React content. Actual signed v13/v15 screenshots confirm
the empty plate absent on Chats and Files. Search opens the search screen. Loading, failure and empty
states expose real state rather than synthetic content. Controller tests cover owner
changes, persistence failures, stale reads, access loss and cleanup.
Actual v12 screenshots show canonical Recipe/Countdown/Pet/Poll cards in Home and
Chats, and Recipe opened its native detail. Signed v15 also confirms the Files
heading and actions below the notch. The later v40 checkpoint above accepts isolated
Home menu movement, detail pinning, drag ordering and cold retention. Signed v50 also
accepts isolated Home and conversation drag, hide-all/restore and cold retention,
plus preserving delivery and menu/organizer inspection on the user's simulator.

## Integration and command map

| Journey                                              | Existing authoritative operations                                                        | Remaining integration                                                                                                                                                                                                                                                                                                                                       |
| ---------------------------------------------------- | ---------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Read a miniapp widget                                | `items`, `item`, `item_at`, `app.snapshot_json`                                          | CNG widget kind/App Group, per-instance item selection, active-account publisher, freshness and cleanup on access loss.                                                                                                                                                                                                                                     |
| In-app Home favorites and miniapp rails              | Canonical `items`, `app.snapshot_json`; identity/device-scoped local Item-ID preferences | Signed v50 accepts isolated Home/conversation swap-and-restore, all-hidden/restore with retained drag handles, cold order and preserving primary delivery. The primary menu/organizer opens without preference mutations. Installed failure-injection rollback and clean formal replay remain unverified. No conversation preference command is repurposed. |
| Open an item/chat through Siri                       | `item` plus current conversation membership                                              | Swift entities/query and read-only intents, app-target setup, local device authentication, single queue consumer, catalog update/clear lifecycle and actual Shortcuts/Siri QA.                                                                                                                                                                              |
| Perform a miniapp tool from a widget/intent          | `app_specs`, `app_call_tool`; Rust grant/confirmation checks                             | Durable invocation-ID deduplication atomic with the mutation and outcome, then native result/confirmation semantics. No props-only Feed/Nap/vote simulation.                                                                                                                                                                                                |
| Send a message or decide a request through an intent | `send_message`, `decide_request`                                                         | Persistent idempotency for invocation replay; approval/account/access checks and honest queued/acknowledged/refused results. These actions are not accepted by this router.                                                                                                                                                                                 |
| Live countdown or current operation                  | Rust-derived item state and target time                                                  | Explicit start/stop UX, system availability, bounded duration/staleDate, activity ownership/recovery and immediate end on sign-out or access loss; actual Dynamic Island/Lock Screen checks.                                                                                                                                                                |
| Remote Live Activity updates                         | Existing `register_push` is a content-free message-wake registration                     | Separate token purpose/ownership/provider contract for push-to-start and per-activity tokens. Do not treat a notification token as an activity token or send private E2E content through APNs.                                                                                                                                                              |

Before the next CNG integration, register only actual home/Lock Screen widget kinds with
supported families. A Live Activity uses `createLiveActivity` and must not receive an
empty `widgets[]` entry. The App Group stores compact projections and selected public
assets only; the existing primary SQLite database and Keychain remain application-owned.

Runtime acceptance must cover first-run widget gallery, selection of two distinct items,
updates after a real signed change and relaunch, light/dark/tinted/Lock Screen rendering,
content redaction, item removal and account change cleanup. App Intents additionally need
warm/cold delivery, rename/delete/access loss and queue-removal failure/replay. A future
mutating intent needs a crash between commit and dequeue to demonstrate durable deduplication.

Useful primary references: [Expo SwiftUI Text timers](https://docs.expo.dev/versions/v58.0.0/sdk/ui/swift-ui/text/),
[Expo SwiftUI modifiers](https://docs.expo.dev/versions/v58.0.0/sdk/ui/swift-ui/modifiers/),
[Apple intent authentication](https://developer.apple.com/documentation/appintents/intentauthenticationpolicy).

## Current CNG integration

`app.config.ts` applies `plugins/with-native-surfaces.cjs`, composing the installed
SDK58 `expo-widgets` and `expo-app-intents` plugins. The extension uses bundle
`xyz.tironi.zoen.widgets` and isolated App Group `group.xyz.tironi.zoen.widgets`.
The primary SQLite database and Keychain service are not shared with the extension.
Push-to-start and frequent remote activity updates are disabled.

`NativeSurfacesProvider` publishes from the live canonical driver and refreshes on
store changes and foreground. Device preferences store only selected item IDs,
owner/device binding, explicit content choices and active system activity IDs.
`MiniappSurfaceControls` offers one device-wide chosen widget item, explicit content
reveal, opt-in Siri/Shortcuts titles, and start/stop for countdowns ending within eight
hours. Multiple widget instances currently use the same selected item; independent
per-instance selection is not implemented. A future redacted timeline entry is
scheduled at 15 minutes; WidgetKit owns scheduling, so this is not a guaranteed erasure
or refresh deadline. No private-content freshness guarantee is claimed while the
app is terminated. Live Activities use canonical ends and system stale dates.

The root mounts the provider beneath `ClientProvider`, the single
`NativeIntentConsumer` beneath ready navigation, and controls in the miniapp screen.
These mounts must be included before runtime acceptance. Surface deep links route
through `/surface`, reread item/access/version and then replace with the actual item.

Apply CNG and generate the Rust simulator framework:

```sh
cd apps/zoen
npx expo prebuild --platform ios
env -u SDKROOT DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer \
  node ../../scripts/build-expo-core.mjs ios --simulator
```

Build the generated workspace for the simulator UDID selected through Argent:

```sh
env -u SDKROOT DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer xcodebuild \
  -workspace ios/Zoen.xcworkspace -configuration Debug -scheme Zoen \
  -destination "id=$ZOEN_SIMULATOR_UDID" ARCHS=arm64 ONLY_ACTIVE_ARCH=YES \
  COMPILER_INDEX_STORE_ENABLE=NO
```

Keep Xcode's default ad-hoc simulator signing enabled. `CODE_SIGNING_ALLOWED=NO`
omits the simulated application entitlements and can cause Keychain access to fail.
The Rust simulator framework currently contains ARM64, so a generic destination
that also links Intel is not supported by this local artifact.

Install the resulting `Zoen.app` through `npx expo run:ios --binary <app-path>
--no-bundler`, which preserves the existing app data. Explicitly open the current
Metro dev-client URL afterwards if Expo selects a different server. Do not uninstall
the existing app or reset its account to make these features appear.

Native App Intents metadata extraction, extension entitlements,
App Group provisioning for device distribution, gallery rendering, Dynamic Island,
Siri warm/cold delivery and accessibility require runtime verification. No paid build
or developer portal changes were performed by this source integration.

## Local device erasure

The You screen's **Sair e apagar dados** action captures the current identity and
device before displaying its destructive confirmation. The store rejects an old
confirmation after an account change and immediately fences old UI results,
timelines, live status and new requests while the operation runs.

Before deleting canonical data, it awaits the registered projection cleanup: widget
content, entity catalogs, activity termination, queued App Intents,
presentation preferences and device notifications. Notification cleanup drains local
notification and badge writes already in flight. If any cleanup fails, the operation
stays retryable and does not delete the account's keys or database. A process restart
at that preflight stage may restore the unchanged account; erasure has not committed.

The iOS bridge then records a small nonsecret intent scoped to the exact device,
drains active core requests and history reception, stops sync and calls the existing
Rust erasure operation. It additionally verifies removal of the account's complete
Keychain namespace and clears the six owned cache namespaces: `ZoenMedia`,
`ImagePicker`, `ImageManipulator`, `DocumentPicker`, `ExpoAudio` and `zoen-exports`.
The preflight also drains active SDK file writers and clears image memory/disk caches
through the public Expo Image APIs. Private image views disable caching. Picker
originals are disposed after profile processing, including failure paths. Cleanup
preserves photo-library originals, Documents, assets, fonts and update caches; re-audit
the owned namespace list when upgrading the SDK packages. Missing files are distinguished
from failed metadata inspection: inaccessible cache or journal parents fail closed instead
of being treated as empty. The intent is removed last; a cold
open resumes an interrupted destructive stage before allowing another account to
open. Neither the account on other devices nor their histories is deleted.

The browser driver uses a strict, atomic IndexedDB transaction to clear the complete
protected vault while retaining its exclusive account lock. It aborts provider setup,
stops messaging and pairing, and releases staged files and photos. A failed storage
transaction remains fenced and retryable; a successful clear removes device keys,
session and root authority together. The action is not offered on Android until its
native lifecycle has separate implementation and acceptance.

Store, browser-vault and pure Foundation tests cover ordering, stale ownership,
delayed writes, inaccessible files and retry behavior. Signed iOS checkpoint `f9544c3a`
passed two disposable-device confirmations, cold onboarding without Metro and exact
primary-history preservation; it also accepted profile save/cold reopen and canceled
silent recording. Actual Web checkpoint `03d787f` passed erasure and peer preservation
in CI. Signed v3 and v4 installed acceptance subsequently passed with the
cache-inspection correction: confirmed local erasure, cold onboarding without Metro
and exact primary-history preservation. v3 repeated cancel as well. A sanitized
runtime witness found that the official SDK launch envelope reaches the native
intent with a slash root. The minimal validator correction accepts only that root
variant while retaining the exact SDK query checks and never following its inner
address. Signed v4 opened directly into Chats through the official CLI and through
a Metro-off linked cold restart; it then passed erasure and Metro-off empty cold
onboarding. Its 244-input source SHA256 is
`f53a592b3dd40bfe6c9d52c99efa8b7cc8f6c2dc40a66b0bcd353087d13d67b0`,
with JS bundle SHA256
`9b23c6324db740ca431ecdd00c34676bf8a7e95ae51109e1f2fcef89e9288553`.
Temporary diagnostic instrumentation was removed before the final bundle.
Existing personal accounts must never be reset to produce evidence.

## Language ownership and native metadata

The custom Expo entry resolves ordered device preferences once and initializes
the typed i18n-js catalogs before Router imports screens. Native bridge open and
browser-worker initialization require that canonical `en` or `pt-BR` tag. Rust
scopes the immutable engine language around synchronous calls and every async
poll, restoring ambient thread state. Language is not persisted in the account.
The current iOS/Web implementation reopens the process/page when preferences
change; coordinated Android foreground changes remain deferred.

Locale changes separators and grammar. Existing plan/grant cents retain their
documented BRL meaning; miniapp scheduling uses its explicit currency and scale,
and the signed owner policy remains USD/6. JavaScript formats/parses exact integer
strings with BigInt, preserving signed-i64 limits without floating-point rounding.
Rust still validates financial policy, including new-negative-cost refusal.

CNG configuration declares EN/PT support and generates translated permission
descriptions, Localizable strings and a separate AppShortcuts table. The plugin
links the native tables into the app/widget targets; widget worklets receive
bounded localized projection props and import no JS translation singleton.
App-owned native errors use English source keys with Portuguese translations.
Generated-resource inspection, Swift parsing, Foundation fixtures and CocoaPods
installation passed. Signed v6 accepts app-owned English and Portuguese main journeys; permission,
gallery and Shortcuts runtime acceptance remains pending.

## iPhone-first installed acceptance

Signed v6 on the disposable iPhone18 Pro/iOS27 simulator accepts ordinary encrypted
linking/history, actual chat rendering and profile photo propagation in both
directions against a fresh current-core primary. Native stock-photo save produced
profile version 2 and an exact matching primary photo hash; the primary update
produced version 3 and visibly replaced the iPhone avatar, name and bio. This
resolves the earlier observed photo-null/version-1 convergence failure.

With Metro stopped, a cold chat deep link opened the real conversation and Back
returned to native tabs. Metro was restored afterward. Per-app PT → EN → PT kept
the same device identity and original literal encrypted history. Native Google
confirmation opened; cancellation returned to onboarding without provisioning an
account. Real provider authentication is a separate remaining setup/acceptance step.

The v5 splash-screen failure was traced through native public logs to missing
`Intl.PluralRules` in Hermes. Registering supported CLDR rules from the existing
OSS i18n-js/make-plural integration fixed startup in v6. A regression removes that
API before executing real English and Portuguese plural messages, without a global
polyfill. iOS money formatting also requires a number-only Intl path: the installed
Hermes tag rejects BigInt there and does not expose `formatToParts` on Apple.
Amount arithmetic remains exact BigInt; only small separator/sign/currency probes
reach the native formatter. Its installed v8 financial journey now passes, as recorded below.

V6 source/artifact manifest SHA256:
`c5ed9e0d2e94775b5a1a5f19a1ca9c971320877554447563788dfd9221914b94`.
JS bundle SHA256:
`8a0b2e32737db608fc665f79ec74916a0107335fb380356dc1cfac617eca5888`.
A 27-second native main-journey clip was recorded. Signed v7 subsequently accepted
the native SwiftUI Appearance Picker; v8 accepted exact money and native UIKit theme
synchronization, as recorded below.

Signed v7 also discovers Zoen in the native Home Screen Widget gallery. Its actual
Portuguese preview displays the privacy description, lock/Zoen branding and the
choose-a-miniapp placeholder without chat, profile or other private content. No
Widget was added and no content preference changed during that check. Selected
content updates, timeline refresh, Live Activities and Siri remain separate journeys.

Signed v8 recovers the exact approved Plan that v7 could not open. The original
certified QA Agent proposal and ordinary one-time owner approval are unchanged.
The corrected confirmed fold binds their exact authenticated opening and receipt,
preserving Agent authorship, history and domain guards. A normal GUI edit saves
`9223372036854775807` BRL cents; two independent canonical peers return that exact
line/total string. Every displayed digit is visible in the iPhone UI. A one-cent
overflow is refused without another version; Metro-off cold reopen retains the
maximum. Normal GUI restoration produces `12345` cents/version 3 on both peers.

The same signed build makes the native glass tab bar dark with white icons when
Dark is selected. A Metro-off cold launch retains the selection; System removes
the override. This uses React Native 0.88's supported app-window appearance API,
not a custom replacement tab bar. Dynamic Type's native-menu fallback still needs
its own runtime acceptance.

V8 source/artifact manifest SHA256:
`e185609cc67bb58599bc30a371e1ac9f5eed7cee2f33b76143b3732e374d4a4e`.
JS bundle SHA256:
`a40bb211968cf234cbef22eede165d66b1f3e73f532094974cd01379565193dc`.
Full native Rust 367/367, focused Plan 71/71, scoped Clippy, Rust formatting and
app TypeScript/Oxlint/Oxfmt plus 123/123 tests pass. These journeys do not establish
real Apple/Google entry, physical-device distribution or complete product parity.

The final signed v9 changes only two rendered Agent captions from v8: it removes
the legacy hardcoded claim that every Agent executes on this device with Apple
Intelligence. A separately owner-authorized Agent principal does not establish
that location or model. Both You and Agent detail now omit that unsupported claim;
owner administration remains available. A Metro-off cold launch accepts both
surfaces and retains the profile photo, native Dark preference and normal
`12345`-cent Plan. The money boundary/overflow matrix was not repeated for this
caption-only delta.

Final v9 source/artifact manifest SHA256:
`e8a6f4180e4aacab89a3f2a3f0541db4800e3a5153606813e9a515d901fc5a9f`.
JS bundle SHA256:
`30242242ae2fddb70e5993c4652e53f34029caa98708bc5f22fb713f074f50da`.
The simulator app contains 136,264,462 bytes. Its source checks and 123 app tests
pass. The earlier functional v8 acceptance and unchanged native Rust artifact
remain identified separately above.
