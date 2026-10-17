import SwiftUI
import RodaCore

/// Existing schedule data, without opening a mini-app runner or an agent surface.
struct LivingEventView: View {
    @Environment(AppModel.self) private var model
    let itemId: String
    @State private var item: ItemDetail?

    var body: some View {
        Form {
            if let item {
                Section {
                    Label(item.title, systemImage: "calendar").font(.title2.bold())
                    if let summary = item.app?.headline, !summary.isEmpty { Text(summary) }
                    if let text = item.text, !text.isEmpty { Text(text).textSelection(.enabled) }
                }
            } else {
                ContentUnavailableView("Event unavailable", systemImage: "calendar.badge.exclamationmark")
            }
        }
        .navigationTitle("Event")
        .task(id: model.revision) { item = model.core.items().first { $0.id == itemId } }
    }
}
