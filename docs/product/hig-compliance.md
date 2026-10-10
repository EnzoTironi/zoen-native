# HIG acceptance for Zoen

Reviewed against Apple's current guidance on 10 October 2026. Complete HIG
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

## Required evidence

For every supported native flow, record the source revision, device and OS,
appearance and accessibility settings, observed result and visual evidence.
Cover onboarding/recovery, Chats, notifications, approvals, mini-app cards and
hosts, Live Pages/history, files, profiles and settings. Recheck affected flows
after integrating parallel iOS, Android or backend changes. A failed or untested
case stays open; source-level corrections remain separate from runtime passes.

| Area | Acceptance | Current boundary |
| --- | --- | --- |
| Window and navigation | System Mac window controls remain visible, usable and clear of content. Resize, fullscreen, split view and keyboard navigation retain one clear content hierarchy. Chats contains direct, group and community conversations. | Shared shell/card journeys exist at their recorded versions. Current release resizing, focus and window-control journeys remain required. |
| Controls and gestures | Use native buttons, menus and dialogs. Every gesture action has an accessible and keyboard alternative, including card editing, reorder and removal. | A local candidate replaces gesture-only tile opening with a native Button and adds menu/accessibility actions. Fresh native activation and removal proof is pending. |
| Target sizes | Prefer 44 × 44 pt controls on iOS/iPadOS and 28 × 28 pt on Mac; Apple's minima are 28 × 28 and 20 × 20 respectively. Evaluate actual hit regions, spacing and pointer/touch context. | The tile candidate gives removal and Done controls 44 pt hit heights. Mac appearance targets and all other product controls still require measurement. A 44 pt rule must not be imposed indiscriminately on Mac. |
| Typography and text enlargement | Use legible system styles, preserve hierarchy and support enlarged content without overlap or losing actions. Default/minimum sizes are 17/11 pt for iOS and 13/10 pt for Mac. | Fixed native/editor fonts and small card/badge labels need review. iOS Dynamic Type through the largest accessibility category and Mac content enlargement remain open. Mac does not support Dynamic Type; using its name in source is not proof of enlargement. |
| Motion | Reduce Motion removes nonessential spatial animation. Necessary changes remain understandable with immediate updates or fades. Gesture tracking still follows the user. | The local tile/header candidate removes spring, scale and slide effects under Reduce Motion. The rest of the product and runtime settings changes remain open. |
| Materials and themes | Liquid Glass belongs to navigation and controls; content uses appropriate standard surfaces. Light, dark and system appearances retain legibility. Respect Reduce Transparency and Increase Contrast. | Shared themes and transparent chat overlays exist. Current contrast, accessibility-setting changes and material behavior across native and web remain open. |
| Accessibility semantics | VoiceOver exposes meaningful labels, values, state and actions in a coherent order. Decorative art does not create duplicate controls. State is understandable without color. | Static labels/actions exist. Full native VoiceOver, Switch Control and keyboard journeys remain required; a button trait alone is insufficient. |
| Focus and presentation | Opening a card moves focus into the presentation. Closing or Escape returns it appropriately. Modals retain usable dismiss and confirmation actions. | Browser card focus/Escape journeys passed on PR 44's sample preview. A local Mac candidate routes mini-apps to its existing native sheet host; fresh native proof is pending. |
| Layout and localization | Safe areas, window edges, keyboard, long text and right-to-left layouts preserve reading and actions. Avoid obstructing messages with opaque navigation containers. | Earlier pin geometry and a 390 px browser case passed. Enlarged text, RTL, software keyboard and full platform size matrices remain open. |
| Live Pages | Typed Plan fields use the same canonical Page, editor and host. Text, selection, undo, IME composition, drafts and history remain usable with assistive technology and enlarged text. | Eight Foundation raw/display mapping fixtures passed. They do not verify TextKit editing, Dynamic Type or accessibility. Plan sessions, typed native controls and real editor journeys remain open. |
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
The comparison snapshot differed only in four proposed native HIG fixes and
produced 426 concerns. These counts describe source patterns, include unpublished
editor work and preview fixtures, and do not count confirmed defects or measure
compliance. In particular, the scanner did not detect the missing Mac mini-app
presentation host or prove that Reduce Motion settings were respected.

Review findings in their enclosing control and rendered flow. Preserve the raw
report, input hashes and decision for each resolved or rejected finding. Keep
native and web images and actual videos attached to the corresponding PR using
`gh --attach`; a rendered report is documentation evidence, not product proof.
