import SwiftUI

/// Ambient, looping decoration (ink boil, mascot, spinning globe, chat backdrops) stops
/// when nobody can see it, e.g. the screen under the full-screen approvals stack. Those
/// views kept redrawing their Canvases every frame behind the cover and ate the main
/// thread while a card was being dragged.
extension EnvironmentValues {
    @Entry var ambientPaused: Bool = false
}
