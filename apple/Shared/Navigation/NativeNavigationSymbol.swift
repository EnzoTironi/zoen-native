import SwiftUI

/// Selection feedback is additive: the system's static selected cue stays instant.
/// Initial badge state is not new work; newly pending identities trigger the bell.
struct NativeNavigationSymbol: View {
    let name: String
    var selected = false
    var tap = 0
    var notificationIDs: Set<String> = []
    var diagnosticName: String? = nil
    @Environment(\.accessibilityReduceMotion) private var reduced
    @Environment(\.nativeIconReduceMotion) private var appReduced
    @Environment(\.ambientPaused) private var paused
    @State private var newNotification = 0
    @State private var selectionTap = 0

    var body: some View {
        NativeIconMotion(systemName: name, bounce: selectionTap + tap,
                         wiggle: newNotification, drawOn: true)
            .onChange(of: selected) { _, new in
                // Never bounce the tab the user is leaving, or initial selection.
                if new && tap == 0 { selectionTap += 1 }
            }
            .onChange(of: notificationIDs) { old, new in
                if !new.subtracting(old).isEmpty {
                    newNotification += 1
                    #if DEBUG
                    if let diagnosticName, !reduced && !appReduced && !paused {
                        NotificationCenter.default.post(name: Notification.Name("NativeParityEffectDiagnostic"),
                            object: "\(diagnosticName)-wiggle-submitted-to-symbol-image")
                    }
                    #endif
                }
            }
    }
}
