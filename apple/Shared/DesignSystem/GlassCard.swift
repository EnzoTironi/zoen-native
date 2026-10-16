import SwiftUI

extension View {
    /// Shared card surface, rendered entirely by the platform's Liquid Glass material.
    func glassCard(radius: CGFloat = 22, padding: CGFloat = 16) -> some View {
        self.padding(padding)
            .glassEffect(.regular, in: RoundedRectangle(cornerRadius: radius, style: .continuous))
    }
}
