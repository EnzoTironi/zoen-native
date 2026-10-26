import Foundation

/// Presentation decisions shared by the picker and its existing gallery.
/// Selection and delivery remain with the caller's signed draft/journal.
enum ZoenAttachmentGalleryLayout {
  static func prefersFullScreen(screen: String) -> Bool { screen == "preview" }
  static func usesPager(screen: String) -> Bool { screen != "selected" }
}
