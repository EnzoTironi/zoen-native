# HIG acceptance for Zoen

Source references reviewed on 10 October 2026; current native evidence updated on 12 October 2026. Complete HIG
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

Current native evidence uses source `eec5c432` with Core193 on macOS 27.2,
an owned demo database and Debug-only test adaptations. The current-main merge
preserves its product/check sources. Three full arm64 app builds (Mac, iOS
simulator and generic iOS device), eleven Swift/API cases and eighteen bounded
Mac runtime assertions pass. [Actual client images/video](https://github.com/EnzoTironi/zoen-native/pull/66#issuecomment-6130391154)
include exact Plan save/cold reopen/restore, scoped Escape, light/dark themes,
conversation opening and notification switching. The video is a screenshot
walkthrough. Physical iOS, Release, minimum-OS, assistive-technology and the full
workflow/settings matrix remain open.

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
| Controls and gestures | Use native buttons, menus and dialogs. Every gesture action has an accessible and keyboard alternative, including card editing, reorder and removal. | Current cards use native Buttons with menu/accessibility actions. Plan card opening and confirmed line removal/restoration pass in the current Mac demo. Full keyboard, pin reorder/removal and gesture alternatives across supported flows remain open. |
| Target sizes | Prefer 44 × 44 pt controls on iOS/iPadOS and 28 × 28 pt on Mac; Apple's minima are 28 × 28 and 20 × 20 respectively. Evaluate actual hit regions, spacing and pointer/touch context. | The candidate gives removal and Done controls 44 pt hit heights, and Mac appearance controls 40 pt hit heights within the existing rail. All affected rendered targets and the remaining product controls still require measurement. A 44 pt rule must not be imposed indiscriminately on Mac. |
| Typography and text enlargement | Use legible system styles, preserve hierarchy and support enlarged content without overlap or losing actions. Default/minimum sizes are 17/11 pt for iOS and 13/10 pt for Mac. | The candidate uses a stacked header and multiline title at accessibility text sizes and increases Mac badge text to 10 pt. Fixed native/editor fonts and the full iOS Dynamic Type/Mac enlargement journeys remain open. Mac does not support Dynamic Type; using its name in source is not proof of enlargement. |
| Motion | Reduce Motion removes nonessential spatial animation. Necessary changes remain understandable with immediate updates or fades. Gesture tracking still follows the user. | The local tile/header candidate removes spring, scale and slide effects under Reduce Motion. The rest of the product and runtime settings changes remain open. |
| Materials and themes | Liquid Glass belongs to navigation and controls; content uses appropriate standard surfaces. Light, dark and system appearances retain legibility. Respect Reduce Transparency and Increase Contrast. | Current Mac light/dark/light switching retains the shared palette, transparent chat overlays and typed Plan content. Contrast measurements, Reduce Transparency/Increase Contrast, system appearance and web preference behavior remain open. |
| Accessibility semantics | VoiceOver exposes meaningful labels, values, state and actions in a coherent order. Decorative art does not create duplicate controls. State is understandable without color. | Static labels/actions exist. Full native VoiceOver, Switch Control and keyboard journeys remain required; a button trait alone is insufficient. |
| Focus and presentation | Opening a card moves focus into the presentation. Closing or Escape returns it appropriately. Modals retain usable dismiss and confirmation actions. | Current Mac history Escape dismisses only its sheet; subsequent Escape closes the Plan inspector. The close button, creation-menu Escape, text-page history Escape and unsaved line-editor cancellation also pass. Window-scoped handling preserves sheets and marked-text composition. Explicit focus movement/return, actual IME and assistive-technology journeys remain open. |
| Layout and localization | Safe areas, window edges, keyboard, long text and right-to-left layouts preserve reading and actions. Avoid obstructing messages with opaque navigation containers. | Earlier pin geometry and a 390 px browser case passed. Enlarged text, RTL, software keyboard and full platform size matrices remain open. |
| Live Pages | Typed Plan fields use the same canonical Page, editor and host. Text, selection, undo, IME composition, drafts and history remain usable with assistive technology and enlarged text. | Foundation mapping/draft cases retain their original scope. Eleven current Swift/API cases verify typed Plan presentation and generated reads; actual Mac legacy-compatible one-cent save/reopen/remove/restore passes. Canonical Plan writers/history, current real TextKit typing/IME/undo, Dynamic Type/accessibility and independent-device convergence remain open. |
| Web parity | Semantic HTML, keyboard focus, zoom/reflow, reduced motion/transparency and contrast preferences preserve the native product hierarchy and behavior. | Existing shell smoke/focus checks use sample data. Authenticated messaging/editor, zoom/reflow and the remaining preference/device matrix remain open. |

## Source audit

The [locked hig-doctor command](../../tools/hig-audit/README.md) supplements this
matrix. Its local core/CLI tests passed 232/232. A Node 24.19.0 check reproduced
truncated upstream JSON through a pipe; the wrapper now publishes a complete
report after a successful scan. The published package matched
the reviewed source bundle. Calibration found both useful detections and known
false positives/blind spots; its references stop at 2 February 2025.

An immutable local candidate snapshot of 126 native/web product and preview
files produced 425 concerns, with no critical and seven serious classifications.
The earlier comparison snapshot differed only in four proposed native HIG fixes
and produced 426 concerns. The current six-file native candidate also includes
adaptive title sizing and Mac appearance/badge changes; its source scan remains
at 425 concerns. These counts describe source patterns, include unpublished
editor work and preview fixtures, and do not count confirmed defects or measure
compliance. In particular, the scanner did not detect the missing Mac mini-app
presentation host or prove that Reduce Motion settings were respected.

The earlier isolated Mac build used PR 44's `c9d6f8b` base and six proposed HIG
files. Its sparse video capture and observed high CPU remain historical evidence.
The current compiled/native unit above has its own source and runtime receipts;
older scan counts and app results do not certify that newer unit.

Review findings in their enclosing control and rendered flow. Preserve the raw
report, input hashes and decision for each resolved or rejected finding. Keep
native and web images and actual videos attached to the corresponding PR using
`gh --attach`; a rendered report is documentation evidence, not product proof.
