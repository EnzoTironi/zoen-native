# Attachment context-menu presentation ownership

The attachment container and durable approval gallery share one native
presentation owner, using an ordinary child UIViewController of the existing
live presenter. Its clear wrapper retains full-window coordinates and the source
container geometry, CA timing, gestures and native caption. No React view is
moved; this introduces no UIWindow, modal presentation or alternate input owner.

The prior direct window-sibling view had no controller in its responder chain.
An isolated actual UIKit/XCUI comparison reproduced both failures: a 1.2-second
press on the production recent-file cell displayed no context menu, and a normal
UIButton primary menu also remained absent. The same component, stable model and
original glass/Lottie closure under valid controller containment displayed both.
This is an adapter integration fix; it does not claim Telegram ContextController
menu animation parity.

Only the presenter's preexisting sibling views receive the existing overlap-safe
accessibility leases. The ancestor, other attachment containers and later system
menu/controller views remain unhidden. Each original sibling accessibility flag
is restored after its last lease, including prehidden values. Suspension hides both content and its wrapper so no invisible touch blocker is
left behind, and releases those same leases. Resume only reacquires the captured
prior siblings; it does not hide a later picker or system menu. Dismissal and
loss of the window remove the actual content/child controller and release leases
before the existing handoff completion. The owner keeps weak content/controller
references to avoid retaining its own view through a controller cycle. Gallery
token/context/revision, background retirement and child-yield admission remain
under the existing Expo adapter.

Focused source tests cover actual responder/parent ownership, overlapping leases,
late system surfaces, prior accessibility flags, parent-window loss, child
release, fresh reopen and full-window coordinate geometry. The bounded XCUI
fixture additionally exercises real File Preview/Select, More menus, teardown,
reopen and portrait/landscape hit targets. A standard UIKit cover/Add-button
fixture reproduces the gallery's same missing-presenter failure and verifies the
shared owner through suspension/resume and reopen. The actual gallery adapter
has its own real Expo SDK typecheck; the integrated gallery Add journey remains
with the app build owner. Original evidence is retained under
`/Users/enzotironi/.codex/builds/zoen-context-menu-v248`; the final shared-owner
eight-case execution and exact source inventory are under
`/Users/enzotironi/.codex/builds/zoen-shared-menu-v249`.
Integrated Expo/Fabric/account/background behavior and final source visual parity
remain part of the owning app validation batch; this fixture does not replace it.
