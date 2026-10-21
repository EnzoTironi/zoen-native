import Foundation

enum ZoenTelegramVideoFailure: Error, LocalizedError {
  case unavailable, invalidRange, missingResources, destination, export, tooLarge
  var errorDescription: String? {
    switch self {
    case .unavailable: NSLocalizedString("This video is no longer available.", comment: "Video editing source unavailable")
    case .invalidRange: NSLocalizedString("Choose a valid part of this video.", comment: "Video editing invalid range")
    case .missingResources: NSLocalizedString("The video controls could not be loaded.", comment: "Pinned video editor resources unavailable")
    case .destination: NSLocalizedString("The video could not be saved. Try again.", comment: "Canonical video destination unavailable")
    case .export: NSLocalizedString("The video could not be exported. Try again.", comment: "Video export failure")
    case .tooLarge: NSLocalizedString("This video is too large to send.", comment: "Video export exceeds existing media byte bound")
    }
  }
}
