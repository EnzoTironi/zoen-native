# HIG acceptance for Zoen

Source references reviewed on 10 October 2026, with Apple's accessibility and materials references refreshed on 12 October; current native evidence updated on 12 October 2026. Complete HIG
alignment is a release requirement. It is not established by a source scan,
a preview or an earlier native journey. No complete compliance claim is made
for the current integration.

Use Apple's [accessibility](https://developer.apple.com/design/human-interface-guidelines/accessibility),
[typography](https://developer.apple.com/design/human-interface-guidelines/typography),
[materials](https://developer.apple.com/design/human-interface-guidelines/materials),
[windows](https://developer.apple.com/design/human-interface-guidelines/windows),
[sidebars](https://developer.apple.com/design/human-interface-guidelines/sidebars)
and [toolbars](https://developer.apple.com/design/human-interface-guidelines/toolbars)
as the current design sources. Apply platform conventions to each client while
preserving Zoen's shared themes, conversations and document model.

Merged [PR 67](https://github.com/EnzoTironi/zoen-native/pull/67) preserves the
accepted `eec5c432` native product/check source. Its Core193 app on macOS 27.2
uses an owned demo database and two Debug-only test adaptations. Three full
arm64 app builds, eleven Swift/API cases, eighteen bounded Mac runtime assertions
and nine additional TextKit/editor assertions retain their revision scope.
[Actual client images/video](https://github.com/EnzoTironi/zoen-native/pull/67)
include exact Plan save/cold reopen/restore, scoped Escape, themes, conversation
opening, notification switching, real typing/undo and Unicode save/restore.

Merged [PR 69](https://github.com/EnzoTironi/zoen-native/pull/69), at product source `19836654`, passes three full arm64 Debug app
builds (Mac, iOS simulator and generic iOS device) with Core193. Actual Tab/Space
journeys verify request review/back with focus return, native change-event opening
and quoted-message navigation. All 426 product and nine check files are preserved
when composing it with Main 68. Its complete hosted CI and resulting Main `a90bb52` have the same whole tree. Keyboard Navigation was temporarily enabled
and restored to its original off value. Review actions left all three requests
pending; the quote journey added one local demo reply. Videos are explicitly
labeled screenshot walkthroughs. Core198 native packaging, physical iOS, Release,
minimum-OS, assistive technology and the full workflow/settings matrix remain open.

Merged [PR 71](https://github.com/EnzoTironi/zoen-native/pull/71), with motion product source `47c90fb`, passes three full arm64 Debug
builds with Core193. Separate bounded Mac journeys verify the chat menu/quote jump,
a local thread reply sent by Cmd-Return and retained after cold reopen, and
Cards/List/Cards with task/approval filters and zero decisions. All 426 product and
nine check inputs remain literal after composing with Main 69. Thread initiation
used a swipe in that unit. Its original 28-second video uses eight actual captures.
A [separate settings unit](https://github.com/EnzoTironi/zoen-native/pull/71#issuecomment-6132977105)
checks actual Reduce Motion, Reduce Transparency and Increase Contrast behavior,
with four images and its own screenshot walkthrough; all temporary preferences
were restored. These bounded checks retain their specific workflow scope.

The current shared-main native unit composes reviewed Core216 libraries with the
previous Swift ABI and passes three arm64 Debug builds. Native menu actions open
the correct reply/thread through Tab/Space, and one actual VoiceOver action opens
the correct thread. VoiceOver and Keyboard Navigation were restored off. An
inline-reply Escape failure was retained and then corrected: the new build removes
the reply bar while retaining composer focus. The attachment menu is visible;
its native picker/review/send journey remains under verification. Complete
assistive-technology, camera/Photos, physical iOS and Release acceptance remain open.

## Required evidence

For every supported native flow, record the source revision, device and OS,
appearance and accessibility settings, observed result and visual evidence.
Cover onboarding/recovery, Chats, notifications, approvals, mini-app cards and
hosts, Live Pages/history, files, profiles and settings. Recheck affected flows
after integrating parallel iOS, Android or backend changes. A failed or untested
case stays open; source-level corrections remain separate from runtime passes.

| Area | Acceptance | Current boundary |
| --- | --- | --- |
| Window and navigation | System Mac window controls remain visible, usable and clear of content. Resize, fullscreen, split view and keyboard navigation retain one clear content hierarchy. Chats contains direct, group and community conversations. | Current Mac captures use the extended chat/inspector layout, one item header and shared themes. System close/minimize/fullscreen controls are exposed in the initial native AX state. Actual control activation, release resizing/fullscreen/split-view and focus journeys remain required. |
| Controls and gestures | Use native buttons, menus and dialogs. Every gesture action has an accessible and keyboard alternative, including card editing, reorder and removal. | Request review headers, change events and quoted replies use native Buttons. Their bounded Mac Tab/Space journeys pass; Plan card opening and confirmed removal/restoration retain prior proof. The Core216 follow-up adds native Reply/Reply in thread menus and verifies keyboard initiation. Full keyboard order and pin reorder/removal journeys remain open. |
| Target sizes | Prefer 44 × 44 pt controls on iOS/iPadOS and 28 × 28 pt on Mac; Apple's minima are 28 × 28 and 20 × 20 respectively. Evaluate actual hit regions, spacing and pointer/touch context. | The candidate gives removal and Done controls 44 pt hit heights, and Mac appearance controls 40 pt hit heights within the existing rail. All affected rendered targets and the remaining product controls still require measurement. A 44 pt rule must not be imposed indiscriminately on Mac. |
| Typography and text enlargement | Use legible system styles, preserve hierarchy and support enlarged content without overlap or losing actions. Default/minimum sizes are 17/11 pt for iOS and 13/10 pt for Mac. | The candidate uses a stacked header and multiline title at accessibility text sizes and increases Mac badge text to 10 pt. Fixed native/editor fonts and the full iOS Dynamic Type/Mac enlargement journeys remain open. Mac does not support Dynamic Type; using its name in source is not proof of enlargement. |
| Motion | Reduce Motion removes nonessential spatial animation. Necessary changes remain understandable with immediate updates or fades. Gesture tracking still follows the user. | PR 71 removes nonessential spatial effects from chat, threads, composer, headers and request lists. Its separate actual settings journeys pass within their scope. The current notification card-stack correction suppresses spring/flip/spatial transitions and passes three Debug builds; its reduced-motion runtime journey and remaining product animation sites require verification. |
| Materials and themes | Liquid Glass belongs to navigation and controls; content uses appropriate standard surfaces. Light, dark and system appearances retain legibility. Respect Reduce Transparency and Increase Contrast. | Current Mac light/dark/light and separate actual Reduce Transparency/Increase Contrast checks retain the shared palette, overlays and Plan content. Measured contrast ratios, system appearance and the full platform/web preference matrix remain open. |
| Accessibility semantics | VoiceOver exposes meaningful labels, values, state and actions in a coherent order. Decorative art does not create duplicate controls. State is understandable without color. | Native request-review labels and a hidden decorative icon improve the control boundary. Full VoiceOver/Switch Control, duplicate agent/avatar announcement review and coherent complete keyboard order remain required; a button trait or a bounded Tab journey alone is insufficient. |
| Focus and presentation | Opening a card moves focus into the presentation. Closing or Escape returns it appropriately. Modals retain usable dismiss and confirmation actions. | Current Mac history Escape dismisses only its sheet; subsequent Escape closes the Plan inspector. Creation-menu Escape, text-page history Escape and unsaved line-editor cancellation retain prior proof. The current request-review destination opens with Back focused, and Back restores focus to the originating list button. Change-event opening and quoted-message navigation pass by Tab/Space. Window-scoped Escape handling preserves sheets and marked-text composition in source; actual IME, every presentation and assistive-technology focus journeys remain open. |
| Layout and localization | Safe areas, window edges, keyboard, long text and right-to-left layouts preserve reading and actions. Avoid obstructing messages with opaque navigation containers. | Earlier pin geometry and a 390 px browser case passed. Enlarged text, RTL, software keyboard and full platform size matrices remain open. |
| Live Pages | Typed Plan fields use the same canonical Page, editor and host. Text, selection, undo, IME composition, drafts and history remain usable with assistive technology and enlarged text. | Foundation mapping/draft cases retain their original scope. Eleven Swift/API cases verify typed Plan presentation and generated reads. Actual Mac one-cent Plan save/reopen/remove/restore and TextKit typing/undo/Unicode save/cold reopen/read-only preview/new-version restoration pass. Canonical Plan writers/history, IME, full selection/viewport, text enlargement/accessibility and independent-device convergence remain open. |
| Web parity | Semantic HTML, keyboard focus, zoom/reflow, reduced motion/transparency and contrast preferences preserve the native product hierarchy and behavior. | Existing shell smoke/focus checks use sample data. Authenticated messaging/editor, zoom/reflow and the remaining preference/device matrix remain open. |

## Source audit

The [locked hig-doctor command](../../tools/hig-audit/README.md) supplements this
matrix. Its local core/CLI tests passed 232/232. A Node 24.19.0 check reproduced
truncated upstream JSON through a pipe; the wrapper now publishes a complete
report after a successful scan. The published package matched
the reviewed source bundle. Calibration found both useful detections and known
false positives/blind spots; its references stop at 2 February 2025.

Three complete scans use the same frozen 122 product-file inputs and locked
Node 24.19.0 tool. Published source `15a58f0` produces 334 Apple moderate concerns
and 37 web concerns (six serious, 31 moderate). Follow-up source `19836654`
produces 330 Apple moderate concerns and the identical 37 web concerns. The motion
follow-up at source `47c90fb`, composed as `883713a`, also produces 330 Apple moderate
concerns and the same 37 web concerns. No Apple report has critical or serious findings.
The four pattern-count changes between the first two scans
are three replaced tap-gesture sites and one decorative image annotation.
The unchanged count after the motion corrections illustrates the scanner's limited
coverage of actual preference behavior; retain the runtime settings matrix.

These are source patterns, not confirmed defect counts or a compliance score.
The four serious web landmark patterns occur inside the labeled sidebar, heading
and navigation; the two focus patterns have enclosing focus-within or overriding
focus-visible styles. Review their actual rendered keyboard flows before treating
the regex classification as a defect or a runtime pass. The two remaining Apple
tap patterns also require their enclosing control and accessible companion to be
reviewed. Runtime settings, VoiceOver, layout and Release evidence remain separate.

The [earlier 126-file scan](https://github.com/EnzoTironi/zoen-native/blob/15a58f0c6955ebd9b951b63dd56971b9a95e3c4c/docs/product/hig-compliance.md)
included preview/editor inputs and produced 425 concerns. Its differing scope
prevents comparison with these 122-file scans. The scanner did not detect a
previous missing Mac mini-app host or establish actual Reduce Motion behavior.

The earlier isolated Mac build used PR 44's `c9d6f8b` base and six proposed HIG
files. Its sparse video capture and observed high CPU remain historical evidence.
The current compiled/native unit above has its own source and runtime receipts;
older scan counts and app results do not certify that newer unit.

Review findings in their enclosing control and rendered flow. Preserve the raw
report, input hashes and decision for each resolved or rejected finding. Keep
native and web images and actual videos attached to the corresponding PR using
`gh --attach`; a rendered report is documentation evidence, not product proof.
