import SwiftUI

/// Native symbol feedback is driven by navigation selection and genuinely new work.
/// Initial badge state is not a new notification; reduced motion keeps static symbols.
struct NativeNavigationSymbol: View {
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    let name: String
    var selected = false
    var notificationCount = 0
    @State private var newNotification = 0

    var body: some View {
        Group {
            if reduceMotion {
                Image(systemName: name)
            } else {
                Image(systemName: name)
                    .symbolEffect(.bounce, value: selected)
                    .symbolEffect(.wiggle, value: newNotification)
                    .contentTransition(.symbolEffect(.replace.magic(fallback: .replace)))
                    .transition(.symbolEffect(.drawOn))
            }
        }
        .onChange(of: notificationCount) { old, new in
            if new > old { newNotification += 1 }
        }
    }
}
