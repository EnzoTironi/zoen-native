import Foundation

// Generated CoreError.localizedDescription reflects Swift enum names. Keep the
// generated ABI intact and translate its presentation at the native boundary.
enum ZoenCoreErrorMessage {
  static func message(_ error: CoreError, bundle: Bundle = .main) -> String {
    switch error {
    case .Invalid(let reason), .Forbidden(let reason), .Stale(let reason), .DraftRejected(let reason):
      return reason
    case .NotFound:
      return NSLocalizedString("This content is no longer available. Refresh and try again.",
        bundle: bundle, comment: "Requested core content is unavailable")
    case .Storage:
      // Storage errors can contain private paths or database details. This text
      // deliberately makes no claim that a mutation was or was not accepted.
      return NSLocalizedString("Something went wrong. Try again.",
        bundle: bundle, comment: "Core storage request failed; outcome may be ambiguous")
    }
  }
}
