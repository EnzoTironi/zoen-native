import Foundation

@main struct ThemeTouchStateTests {
  static func main() {
    var disabled = ZoenThemeTouchState()
    precondition(disabled.arm(testID: nil) == nil)
    precondition(disabled.arm(testID: "unrelated-control") == nil)
    precondition(!disabled.begin(generation: 0, touch: 1))
    var state = ZoenThemeTouchState()
    let lease = state.arm(testID: "zoen-theme-automatic-probe")!
    precondition(!state.phase(.valueChanged, generation: lease, touch: 1))
    precondition(!state.begin(generation: lease + 1, touch: 1))
    precondition(state.begin(generation: lease, touch: 41))
    precondition(!state.begin(generation: lease, touch: 42))
    precondition(!state.phase(.ended, generation: lease, touch: 42))
    precondition(!state.phase(.valueChanged, generation: lease, touch: 42))
    precondition(state.phase(.ended, generation: lease, touch: 41))
    // UIKit's same delivery may emit actual valueChanged after touchesEnded.
    precondition(state.phase(.valueChanged, generation: lease, touch: 41))
    precondition(!state.phase(.valueChanged, generation: lease, touch: 41))
    precondition(!state.phase(.ended, generation: lease, touch: 41))
    state.retire()
    precondition(!state.current(lease))
    precondition(!state.record(lease))
    precondition(!state.phase(.controlCancelled, generation: lease, touch: 41))
    precondition(state.arm(testID: "zoen-theme-automatic-probe") == nil)
    var cancelled = ZoenThemeTouchState()
    let second = cancelled.arm(testID: "zoen-theme-automatic-probe")!
    precondition(cancelled.begin(generation: second, touch: 8))
    precondition(cancelled.phase(.controlCancelled, generation: second, touch: 8))
    precondition(!cancelled.phase(.controlCancelled, generation: second, touch: 8))
    precondition(cancelled.phase(.cancelled, generation: second, touch: 8))
    cancelled.retire()
    precondition(!cancelled.phase(.valueChanged, generation: second, touch: 8))
    var detached = ZoenThemeTouchState()
    let third = detached.arm(testID: "zoen-theme-automatic-probe")!
    detached.retire() // App inactive, window/Host removal or opt-out.
    precondition(!detached.begin(generation: third, touch: 9))
    precondition(detached.arm(testID: "zoen-theme-automatic-probe") == nil)
    var budget = ZoenThemeProbeBudget()
    precondition(budget.take() == nil)
    precondition(budget.claim())
    precondition(!budget.claim())
    for expected in 1...8 { precondition(budget.take() == expected) }
    for _ in 0..<100 { precondition(budget.take() == nil) }
    var retiredBudget = ZoenThemeProbeBudget()
    precondition(retiredBudget.claim())
    precondition(retiredBudget.take() == 1)
    retiredBudget.stop()
    precondition(retiredBudget.take() == nil)
    precondition(!retiredBudget.claim())
    var bounded = ZoenThemeTouchState()
    let fourth = bounded.arm(testID: "zoen-theme-automatic-probe")!
    for _ in 0..<8 { precondition(bounded.record(fourth)) }
    precondition(!bounded.record(fourth))
    print("PASS exact opt-in, one touch, foreign/stale callbacks, terminal ordering, cancellation, retirement and cap8")
  }
}
