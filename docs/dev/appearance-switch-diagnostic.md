# Appearance Switch diagnostic (#440)

This is an opt-in observation of one input sequence, not a repair or a gesture driver.
Enable the existing `EXPO_PUBLIC_ZOEN_THEME_INPUT_PROBE=1` build setting. The universal
Switch must retain the exact `zoen-theme-automatic-probe` testID. Other controls
create no observer. The patch targets pinned `@expo/ui` 58.0.23.

The background resolves exactly one public UISwitch inside its own nearest Expo
UIHostingController, within a bounded subtree. It adds a passive recognizer to that
window and separate valueChanged/touchCancel targets to that control. No existing
target, recognizer delegate, handler or preference is replaced. The recognizer never
recognizes; cancellation, delay and prevention of other recognizers are disabled.
No setter, synthetic touch, sendActions or manual production callback is used.

## Records

The native logger keeps one process-lifetime claim and eight records total, shared
with the existing Binding/dispatch diagnostics. A second Host cannot begin another
sequence. JavaScript stage counters remain the existing independent diagnostic.

| Record                                                                  | What it establishes                                                                                                 |
| ----------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------- |
| native-touch-probe-armed                                                | The exact live Switch and its window were resolved.                                                                 |
| native-touch-began                                                      | An actual touch began inside its bounds; target class/branch and up to six ancestor recognizer states are observed. |
| native-touch-ended / native-observer-touch-cancelled                    | The same original UITouch ended or was cancelled.                                                                   |
| native-control-value-changed                                            | UISwitch actually emitted valueChanged, including its observed Boolean value.                                       |
| native-control-touch-cancelled                                          | UIControl actually emitted touchCancel. Absence is not evidence that UIKit did not cancel.                          |
| native-binding-set / native-dispatch-attempt / native-dispatch-returned | The existing actual SwiftUI setter/Expo dispatch stages executed while the probe was live.                          |

No text, labels, message contents or coordinates are logged. Class names and
ancestor traversal are bounded. Window/control references and original touch are
weak. App inactivity, Host/window teardown, opt-out and sequence cancellation
revoke capture before observers are detached. A normal end permits valueChanged
later in the same UIKit delivery and retires on the next main-queue turn; this
turn is not an input retry, timer or gesture. Stale callbacks cannot rearm capture.

## Operator interpretation

A trace without `armed` does not test input routing. With `armed`, compare the
original target branch, actual valueChanged, Binding, dispatch and JavaScript
preference stages for the one authorized ordinary gesture. Missing stages narrow
the path; they do not alone identify the responsible recognizer or prove causality.
A new run requires a fresh process; do not retry interactions to fill missing logs.
No accessibility activation is fabricated when the current QA driver lacks it.

## Qualification boundaries

The Node regression extracts the actual Foundation capture/budget helper from the
patch and compiles/runs it with Swift 6 strict concurrency. It covers exact opt-in,
foreign/stale callbacks, one sequence, post-ended ordering, cancellation, retirement,
no rearming and cap8. The same test parses the actual UIKit/SwiftUI adapter syntax.
SDK23 callback/persistence tests exercise the real JS source through a native event
port. None of these are an iPhone gesture or UIKit/Expo typecheck.

Patch application is checked against the pinned upstream Toggle source in a
throwaway directory. Full native compilation belongs to the next coherent build;
the existing iPhone failure remains open until a matched artifact is observed.
