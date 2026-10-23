import Foundation

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
}
