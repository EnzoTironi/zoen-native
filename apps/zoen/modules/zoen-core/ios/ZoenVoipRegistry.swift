import Foundation
@preconcurrency import PushKit

/// PushKit uses the existing process CallKit provider and session gate.
@MainActor
final class ZoenVoipRegistry: NSObject, PKPushRegistryDelegate {
  static let shared = ZoenVoipRegistry()
  private var registry: PKPushRegistry?
  private var token: String?
  private var platform: String?
  private weak var session: ZoenSession?
  private var lease: ZoenModuleSessionLease?
  private var registration: Task<Void, Never>?
  private var revision: UInt64 = 0

  func start() {
    let info = Bundle.main.infoDictionary ?? [:]
    guard registry == nil,
          info["ZoenVoipWakeEnabled"] as? Bool == true,
          (info["UIBackgroundModes"] as? [String])?.contains("voip") == true,
          ZoenPushBuildAdmission.enabled(info, bundleIdentifier: Bundle.main.bundleIdentifier) else { return }
    platform = info["ZoenPushEnvironment"] as? String == "sandbox" ? "apns-voip-sandbox" : "apns-voip"
    let registry = PKPushRegistry(queue: .main)
    registry.delegate = self
    self.registry = registry
    registry.desiredPushTypes = [.voIP]
  }

  func bind(session: ZoenSession, lease: ZoenModuleSessionLease) {
    guard registry != nil, lease.isLive else { return }
    self.session = session
    self.lease = lease
    revision &+= 1
    drainRegistration()
  }

  nonisolated func pushRegistry(_ registry: PKPushRegistry, didUpdate pushCredentials: PKPushCredentials, for type: PKPushType) {
    MainActor.assumeIsolated {
      guard type == .voIP, self.registry === registry else { return }
      let bytes = pushCredentials.token
      guard !bytes.isEmpty, bytes.count <= 256 else { return }
      updateToken(bytes.map { String(format: "%02x", $0) }.joined())
    }
  }

  nonisolated func pushRegistry(_ registry: PKPushRegistry, didInvalidatePushTokenFor type: PKPushType) {
    MainActor.assumeIsolated {
      guard type == .voIP, self.registry === registry else { return }
      updateToken("")
    }
  }

  private func updateToken(_ value: String) {
    guard token != value else { return }
    token = value
    revision &+= 1
    drainRegistration()
  }

  private func drainRegistration() {
    guard registration == nil, let session, let lease, lease.isLive, let token, let platform else { return }
    let expected = revision
    registration = Task {
      defer {
        registration = nil
        if revision != expected { drainRegistration() }
      }
      for delay in [0, 1, 2, 4, 8, 16] {
        if delay > 0 { try? await Task.sleep(for: .seconds(delay)) }
        guard lease.isLive, revision == expected else { return }
        do {
          try await session.registerVoipToken(platform: platform, token: token, lease: lease)
          return
        } catch {
          // The relay can still be reconnecting during native account open.
        }
      }
    }
  }

  nonisolated func pushRegistry(
    _ registry: PKPushRegistry,
    didReceiveIncomingPushWith payload: PKPushPayload,
    for type: PKPushType,
    completion: @escaping () -> Void
  ) {
    MainActor.assumeIsolated {
      guard type == .voIP else { completion(); return }
      let dictionary = payload.dictionaryPayload
      let data = try? JSONSerialization.data(withJSONObject: dictionary)
      let json = data.flatMap { $0.count <= 512 ? String(data: $0, encoding: .utf8) : nil }
      let nonce = (dictionary["zoen"] as? [String: Any])?["call_wake"] as? String
      ZoenNativePushSessions.reportVoipWake(id: Self.callID(nonce) ?? UUID(), payload: json, completion: completion)
    }
  }

  private static func callID(_ nonce: String?) -> UUID? {
    guard let nonce, nonce.utf8.count == 32,
          nonce.utf8.allSatisfy({ (48...57).contains($0) || (97...102).contains($0) }) else { return nil }
    let bytes = Array(nonce.utf8)
    let groups = [0..<8, 8..<12, 12..<16, 16..<20, 20..<32]
    return UUID(uuidString: groups.map { String(decoding: bytes[$0], as: UTF8.self) }.joined(separator: "-"))
  }
}
