# Attachment context-menu presentation ownership

The attachment container uses an ordinary child UIViewController of the existing
live presenter. Its view retains the source container geometry, CA timing,
window-coordinate viewport, gestures and native caption. No React view is moved,
and this introduces no UIWindow, modal presentation or alternate input owner.

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
is restored after its last lease, including prehidden values. Dismissal and loss
of the window remove the actual child controller and release the leases before
the existing handoff completion. The container keeps a weak controller reference
to avoid retaining its own view through a controller cycle.

Focused source tests cover actual responder/parent ownership, overlapping leases,
late system surfaces, prior accessibility flags, parent-window loss, child
release, fresh reopen and full-window coordinate geometry. The bounded XCUI
fixture additionally exercises real File Preview/Select, More menus, teardown,
reopen and portrait/landscape hit targets. Evidence and exact source inventories
are retained under `/Users/enzotironi/.codex/builds/zoen-context-menu-v248`.
Integrated Expo/Fabric/account/background behavior and final source visual parity
remain part of the owning app validation batch; this fixture does not replace it.
