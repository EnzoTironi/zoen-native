#if DEBUG && os(iOS)
import SwiftUI
import UIKit

/// Diagnostic-only window geometry: not part of the cross-platform fixture identity.
/// Reads actual UIKit window safe areas, never guessed status/home-bar constants.
struct NativeParityGeometryWitness: UIViewRepresentable {
    func makeUIView(context: Context) -> Probe {
        let view = Probe()
        view.isAccessibilityElement = true
        view.accessibilityIdentifier = "parity-window-geometry"
        view.backgroundColor = .clear
        view.isUserInteractionEnabled = false
        return view
    }

    func updateUIView(_ uiView: Probe, context: Context) { uiView.refresh() }

    final class Probe: UIView {
        override func didMoveToWindow() { super.didMoveToWindow(); refresh() }
        override func safeAreaInsetsDidChange() { super.safeAreaInsetsDidChange(); refresh() }
        override func layoutSubviews() { super.layoutSubviews(); refresh() }

        func refresh() {
            guard let window else { return }
            let scale = window.screen.scale
            let insets = window.safeAreaInsets
            let bounds = window.bounds
            let value: [String: Any] = [
                "window_points": ["width": bounds.width, "height": bounds.height],
                "scale": scale,
                "safe_area_points": ["top": insets.top, "left": insets.left,
                                     "bottom": insets.bottom, "right": insets.right],
                "safe_area_pixels": ["top": insets.top * scale, "left": insets.left * scale,
                                     "bottom": insets.bottom * scale, "right": insets.right * scale],
                "orientation": window.windowScene?.interfaceOrientation.rawValue ?? 0
            ]
            guard let data = try? JSONSerialization.data(withJSONObject: value, options: [.sortedKeys]),
                  let json = String(data: data, encoding: .utf8) else { return }
            accessibilityLabel = json
        }
    }
}
#endif
