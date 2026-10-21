# Photos reference menu

This bounded port replaces the Photos More `UIMenu` with the modern reference/list
presentation from Telegram iOS `f1dd7a2dbd02cbbf513e75d5695d8d36d1cf5838`.
The action remains Zoen's existing terminal Send-as-File intent. No new delivery,
selection, caption, account or document authority is introduced.

## Source behavior

`MediaPickerScreen:2761–2769,3286–3287,3501–3512` passes the actual glass More
button to a reference source. `ContextControllerImpl:422–490` routes the default
implementation through `ContextSourceContainer`,
`ContextControllerExtractedPresentationNode` and
`ContextControllerActionsStackNodeImpl`. The legacy14pt/250pt renderer is not
used. The Photos glass button does not opt into liquid morph; the opt-in defaults
false in `ContextContentSourceNode:7–17`.

The port retains the modern list's10pt outer vertical padding,18pt text inset,
11pt row vertical padding,20pt icon inset,32pt icon column,8pt icon/text spacing,
220pt minimum width with titles measured against the240pt budget, and radius
`min(30,height/2)`. It reuses the already ported original glass surface and
container with28pt spacing. The reference backdrop stays clear, without screen
blur or dimming. Source conversion,7pt anchor gap,16pt horizontal bounds,
bottom10pt plus intrinsic inset and initial overflow offset follow the original
reference branch. Source text is plain/two-line/default17pt in this slice.

The original Display layer helper supplies the opening spring:0.42s,
damping104, velocity0, scale0.01→1, additive source-to-menu position and0.05s
opacity. Dismissal uses0.2s easeInOut, opacity→0, scale→0.01 and the source
offset. The immediate-touch selection recognizer follows
`ContextControllerActionsStackNode:1405–1467`; enabled rows highlight with
selection haptics,0.16s movement and0.2s appearance/release. A canceled gesture
does not perform an action. Outside tap and accessibility Escape dismiss.
`ContextSourceContainer:17–25,410–423` supplies the same invisible1×1 rendered
image to the outside-dismiss layer: the pinned source records that a clear layer
with no contents can pass hit testing yet receive no delivered touches on iOS27.
The image is shared and creates no visible dimming or change to the hit rectangle.

`MediaPickerScreen:3174–3181` begins dismissal before invoking Send-as-File;
sending does not wait for the animation completion. Zoen's existing
`performSendAsFileOption` still checks the exact current revision, ordered IDs,
caption limit, busy/retired state and final native caption at that moment.

## Ownership and adaptations

The existing shared native presentation owner supplies child-controller,
window and accessibility ownership. The new menu has one presentation and one
dismissal. A retired/opening/closing callback cannot invoke a second action.
Model revision, selection, option availability, screen/category, busy/caption
state and theme invalidation close the menu; the callback also revalidates.
Retirement, app deactivation and parent/window loss remove it synchronously.
The caption view is never reparented, remounted or asked to resign first responder.

On interruption the closing animation samples current presentation position,
scale and opacity. Reduced Motion keeps opacity while omitting transforms.
These are explicit UIKit adaptations. An unusually narrow host caps width to
its viewport minus32pt; overflow uses a native scroll view and disables the
selection-drag recognizer while scrolling. Rich text, submenus, custom font-size
configuration, hardware shortcut routing, reactions, extracted recent-file rows
and optional liquid morph are outside this batch. The distinct extracted File
source is covered by [its follow-up port](telegram-ios-file-extracted-menu.md).
Gallery Add still uses its existing UIKit menu.

## Qualification

The pinned files, hashes and translated ranges are in
[`telegram-ios-reference-menu-sources.json`](telegram-ios-reference-menu-sources.json).
The isolated proof is `/Users/enzotironi/.codex/builds/zoen-reference-menu-v251/`.

Executed: the exact production Foundation/CoreGraphics layout and single-use
lifecycle, including source anchors,144 layouts, title/icon sizing and interrupted
retirement. SDK-qualified: actual caller, menu, shared owner, original glass,
Display helpers, Lottie, compiled ExpoModulesCore/React dependencies, authored
UIKit tests and existing caller tests. The UIKit test copies change only the
target-module import for the standalone typecheck. There are no UIKit/rendering
substitutes.

No simulator, app, main build, Pods, core build or service was used for this
candidate. The authored UIKit tests have not executed in this batch. The parent
must qualify actual More hit targets, drag/outside cancel, close during opening,
caption focus/final keystroke, invalidation/retirement and day/night geometry
in the next integrated runtime batch. Pixel/animation parity is not certified by
typechecking or arithmetic tests.
The outside-dismiss UIKit assertion checks the1×1 rendered contents; calling
`sendActions` still does not establish that the OS delivered a physical tap.
That touch-delivery follow-up is SDK-qualified in
`/Users/enzotironi/.codex/builds/zoen-reference-menu-touch-v253/`; its UIKit test
has only been typechecked. Integrated iOS27 outside taps remain the parent batch.
