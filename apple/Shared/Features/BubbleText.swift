import SwiftUI

/// An inline, indivisible reservation lets the time wrap with the text's final line.
struct BubbleText<Meta: View>: View {
    let text: Text
    @ViewBuilder var meta: () -> Meta
    @State private var metaSize: CGSize = .zero

    var body: some View {
        let spacer = Image(size: CGSize(width: metaSize.width + 6, height: max(1, metaSize.height))) { _ in }
        Text("\(text) \(Text(spacer))")
            .fixedSize(horizontal: false, vertical: true)
            .overlay(alignment: .bottomTrailing) {
                meta()
                    .fixedSize()
                    .onGeometryChange(for: CGSize.self) { $0.size } action: { metaSize = $0 }
                    // Measure before drawing so even the initial layout cannot overlap text.
                    .opacity(metaSize == .zero ? 0 : 1)
            }
    }
}
