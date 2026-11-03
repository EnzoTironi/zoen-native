import Foundation
import UIKit

/// All Client.open/close transitions for this module and the OS wake subscriber
/// pass through the same gate. The gate holds no account, token or durable state.
enum ZoenNativePushSessions {
  static let gate = ZoenProcessSessionGate<ZoenSession> { session in
    try await session.close(discardEdits: true)
  }

  static func wake(deadline: ContinuousClock.Instant, voip: (payload: String, id: UUID)? = nil) async -> ZoenPushResult {
    await gate.runWake(make: { ZoenSession() }, retain: { await $0.retainsNotificationCall() }) { session, cold, lease in
      try await session.notificationWake(cold: cold, deadline: deadline, lease: lease, voip: voip)
    }
  }

  /// Called from the PushKit delegate on its main queue. CallKit receives the
  /// report before the session gate can wait on disk, authentication or sync.
  @MainActor
  static func reportVoipWake(id: UUID, payload: String?, completion: @escaping () -> Void) {
    let deadline = ContinuousClock.now.advanced(by: .seconds(8))
    let system = ZoenCallSystem.shared
    system.reportWake(id: id) { reported in
      completion()
      guard reported else { return }
      guard let payload else { system.finishWake(id: id); return }
      Task {
        _ = await wake(deadline: deadline, voip: (payload, id))
        system.finishWake(id: id)
      }
    }
  }

  static func open(owner: ZoenModuleSessionOwner<ZoenSession>, mode: String, locale: String,
    changed: @escaping @Sendable ([String: Any]) -> Void,
    call: @escaping @Sendable (String) -> Void, audio: @escaping @Sendable (String) -> Void) async throws -> String {
    try await gate.openAdoptingRetained(owner: owner, allowed: mode == "account") { session, lease, adopted in
      if adopted {
        let result = try await session.adoptNotificationCall(locale: locale, lease: lease, changed: changed, call: call, audio: audio)
        await ZoenVoipRegistry.shared.bind(session: session, lease: lease)
        return result
      }
      let result = try await session.open(mode: mode, locale: locale, changed: changed)
      await session.bindCalls(lease: lease, changed: call, audio: audio)
      if mode == "account" { await ZoenVoipRegistry.shared.bind(session: session, lease: lease) }
      return result
    }
  }

  static func wake(session: ZoenSession, lease: ZoenModuleSessionLease, identityID: String, deviceID: String) async {
    let deadline = ContinuousClock.now.advanced(by: .seconds(8))
    guard ZoenPushBuildAdmission.enabled(Bundle.main.infoDictionary ?? [:], bundleIdentifier: Bundle.main.bundleIdentifier) else { return }
    guard await MainActor.run(body: { UIApplication.shared.applicationState != .active }) else { return }
    guard let borrow = try? await session.notificationBorrow(identityID: identityID, deviceID: deviceID, lease: lease) else { return }
    _ = await gate.runWake(session: session, lease: lease) { session, cold, work in
      try await session.notificationWake(cold: cold, deadline: deadline, lease: work, expectedBorrow: borrow)
    }
  }
}
