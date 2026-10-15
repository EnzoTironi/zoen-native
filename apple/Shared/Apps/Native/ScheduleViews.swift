import SwiftUI
import RodaCore

/// Schedule runs as a bundled MCP App (React) in the sandboxed web view, edge to edge,
/// the same path as the Saturday hike. Close comes from the mini-app chrome.
struct ScheduleSheet: View {
    @Environment(\.dismiss) private var dismiss
    @Environment(\.miniAppClose) private var miniAppClose
    let item: ItemDetail
    @State private var height: CGFloat = 800

    var body: some View {
        McpAppWebView(itemId: item.id, displayMode: "fullscreen", height: $height, onClose: { (miniAppClose ?? { dismiss() })() }, edgeToEdge: true)
            .ignoresSafeArea()
            .background(Color(hex: "#F4F6FB").ignoresSafeArea())
            .presentationDetents([.large])
            .presentationDragIndicator(.hidden)
            .interactiveDismissDisabled()
    }
}
