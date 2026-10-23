import Foundation
import UIKit

/// All Client.open/close transitions for this module and the OS wake subscriber
/// pass through the same gate. The gate holds no account, token or durable state.
enum ZoenNativePushSessions {
  static let gate = ZoenProcessSessionGate<ZoenSession> { session in
    try await session.close(discardEdits: true)
  }

  static func wake(deadline: ContinuousClock.Instant) async -> ZoenPushResult {
    await gate.runWake(make: { ZoenSession() }) { session, cold, lease in
      try await session.notificationWake(cold: cold, deadline: deadline, lease: lease)
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
