import Foundation

/// Presentation ownership only: no call authority or media resources. Transition
/// completions carry a token so an old dismiss cannot hide a replacement view.
@MainActor
final class ZoenCallPresentation {
  enum Phase { case hidden, showing, visible, hiding, retired }
  private(set) var phase = Phase.hidden
  private var ticket: UUID?
  private var showRequested = false
  private var hideRequested = false
  private var returnOffered = false
  var minimized: Bool { returnOffered && phase != .visible && phase != .retired }

  func snapshotState(_ active: String) -> String { phase == .retired ? "ended" : active }

  func beginShow() -> UUID? {
    if phase == .hiding { showRequested = true; return nil }
    guard phase == .hidden else { return nil }
    let id = UUID(); ticket = id; phase = .showing; return id
  }
  func shown(_ id: UUID) -> Bool {
    guard phase == .showing, ticket == id else { return false }
    phase = .visible; ticket = nil; returnOffered = false; return true
  }
  func showFailed(_ id: UUID) {
    guard phase == .showing, ticket == id else { return }
    phase = .hidden; ticket = nil; showRequested = false; hideRequested = false; returnOffered = true
  }
  func beginHide() -> UUID? {
    if phase == .showing { hideRequested = true; return nil }
    guard phase == .visible else { return nil }
    let id = UUID(); ticket = id; phase = .hiding; returnOffered = true; return id
  }
  func hidden(_ id: UUID) -> Bool {
    guard phase == .hiding, ticket == id else { return false }
    phase = .hidden; ticket = nil; return true
  }
  func consumeShowRequest() -> Bool { let value = showRequested; showRequested = false; return value }
  func consumeHideRequest() -> Bool { let value = hideRequested; hideRequested = false; return value }
  func retire() { phase = .retired; ticket = nil; showRequested = false; hideRequested = false; returnOffered = false }
}

struct ZoenCallPresentationSnapshot: Sendable {
  let identityID: String
  let deviceID: String
  let spaceID: String
  let callID: String
  let state: String
  let minimized: Bool
  let connectedAtMs: Int64?

  func json() -> String {
    let value: [String: Any] = ["identity_id": identityID, "device_id": deviceID,
      "space_id": spaceID, "call_id": callID, "state": state, "minimized": minimized,
      "connected_at_ms": connectedAtMs.map { $0 as Any } ?? NSNull()]
    guard let data = try? JSONSerialization.data(withJSONObject: value) else { return "null" }
    return String(decoding: data, as: UTF8.self)
  }
}
