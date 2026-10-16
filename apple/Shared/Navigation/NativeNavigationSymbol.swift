import SwiftUI

/// Selection feedback is additive: the system's static selected cue stays instant.
/// Initial badge state is not new work; newly pending identities trigger the bell.
struct NativeNavigationSymbol: View {
    let name: String
    var selected = false
    var notificationIDs: Set<String> = []
    @State private var newNotification = 0
    @State private var selectionTap = 0

    var body: some View {
        NativeIconMotion(systemName: name, bounce: selectionTap,
                         wiggle: newNotification, drawOn: true)
            .onChange(of: selected) { _, new in
                // Never bounce the tab the user is leaving, or initial selection.
                if new { selectionTap += 1 }
            }
            .onChange(of: notificationIDs) { old, new in
                if !new.subtracting(old).isEmpty { newNotification += 1 }
            }
    }
}
