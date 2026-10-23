import Foundation
import UIKit
import UserNotifications

extension ZoenSession {
  struct NotificationBorrow: Sendable {
    let engine: Client
    let generation: UInt64
    let identityID: String
    let deviceID: String
  }

  func notificationBorrow(identityID: String, deviceID: String, lease: ZoenModuleSessionLease) throws -> NotificationBorrow {
    guard lease.isLive, !erasing, !closing, mode == "account", let engine = core,
          let account = engine.account(), account.unlocked, account.registered, !account.setupPending,
          account.identityId == identityID, account.deviceId == deviceID else { throw SessionError.closed }
    return NotificationBorrow(engine: engine, generation: generation, identityID: identityID, deviceID: deviceID)
  }

  func setNotificationForeground(_ active: Bool) { notificationActive = active }

  private func notificationIsCurrent(_ engine: Client, _ expected: UInt64, _ account: ZoenPushSnapshot.Account, _ lease: ZoenPushWakeLease) -> Bool {
    guard lease.isLive, !erasing, !closing, generation == expected, core === engine, mode == "account",
          let now = engine.account(), now.unlocked, now.registered, !now.setupPending else { return false }
    return now.identityId == account.identity_id && now.deviceId == account.device_id
  }

  /// The deadline starts at OS callback admission, before the process transition queue.
  /// No identity creation, provider setup, history retry pump or alternative vault.
  func notificationWake(cold: Bool, deadline: ContinuousClock.Instant, lease: ZoenPushWakeLease, expectedBorrow: NotificationBorrow? = nil) async throws -> ZoenPushResult {
    guard lease.isLive else { throw CancellationError() }
    notificationActive = false
    if cold {
      let root = ZoenDeviceErasure.accountRoot
      let database = root.appending(path: "client.sqlite")
      let info = try FileManager.default.attributesOfItem(atPath: database.path)
      guard info[.type] as? FileAttributeType == .typeRegular else { throw SessionError.closed }
      try Task.checkCancellation()
      guard ContinuousClock.now < deadline else { throw CancellationError() }
      notificationActive = false
      let locale = Locale.preferredLanguages.first?.lowercased().hasPrefix("pt") == true ? "pt-BR" : "en"
      _ = try await open(mode: "account", locale: locale, forNotification: true, changed: { _ in })
    }
    guard !erasing, !closing, mode == "account", let engine = core,
          let account = engine.account(), account.unlocked, account.registered, !account.setupPending else { return .noData }
    if let expectedBorrow {
      guard !cold, engine === expectedBorrow.engine, generation == expectedBorrow.generation,
            account.identityId == expectedBorrow.identityID, account.deviceId == expectedBorrow.deviceID else { throw SessionError.closed }
    }
    requests += 1
    defer { retireRequest() }
    let expected = generation
    let beforeJSON: String
    if let notificationBaseline { beforeJSON = notificationBaseline }
    else { beforeJSON = try await engine.clientRequest(request: #"{"op":"snapshot"}"#) }
    let before = try ZoenPushSnapshot(json: beforeJSON)
    guard let owner = before.account, notificationIsCurrent(engine, expected, owner, lease) else { throw SessionError.closed }
    try Task.checkCancellation()
    let remaining = ContinuousClock.now.duration(to: deadline)
    let parts = remaining.components
    let milliseconds = parts.seconds * 1000 + parts.attoseconds / 1_000_000_000_000_000
    guard milliseconds > 0 else { throw CancellationError() }
    _ = try await engine.waitUntilSettled(timeoutMs: UInt64(milliseconds))
    try Task.checkCancellation()
    guard ContinuousClock.now < deadline, notificationIsCurrent(engine, expected, owner, lease) else { throw SessionError.closed }
    let afterJSON = try await engine.clientRequest(request: #"{"op":"snapshot"}"#)
    let after = try ZoenPushSnapshot(json: afterJSON)
    let center = UNUserNotificationCenter.current()
    let permission = await center.notificationSettings().authorizationStatus
    try Task.checkCancellation()
    guard ContinuousClock.now < deadline, notificationIsCurrent(engine, expected, owner, lease) else { throw SessionError.closed }
    var presented = false
    if [.authorized, .provisional, .ephemeral].contains(permission) {
      for note in after.notes(since: before) {
        try Task.checkCancellation()
        guard await MainActor.run(body: { UIApplication.shared.applicationState != .active }) else { throw CancellationError() }
        guard ContinuousClock.now < deadline, notificationIsCurrent(engine, expected, owner, lease) else { throw SessionError.closed }
        // Recheck the authenticated projection after each await, including after
        // the OS write. These are observations, not an atomic claim over system UI.
        let currentJSON = try await engine.clientRequest(request: #"{"op":"snapshot"}"#)
        let current = try ZoenPushSnapshot(json: currentJSON)
        try Task.checkCancellation()
        guard ContinuousClock.now < deadline, notificationIsCurrent(engine, expected, owner, lease) else { throw SessionError.closed }
        guard current.notes(since: before).contains(note) else { continue }
        let content = UNMutableNotificationContent()
        content.title = note.title
        content.body = note.body
        content.sound = .default
        content.threadIdentifier = note.spaceID
        content.userInfo = ["zoen_local": 1, "device_id": note.deviceID, "space_id": note.spaceID, "event_seq": note.sequence]
        try await center.add(UNNotificationRequest(identifier: note.identifier, content: content, trigger: nil))
        do {
          try Task.checkCancellation()
          guard ContinuousClock.now < deadline, notificationIsCurrent(engine, expected, owner, lease) else { throw SessionError.closed }
          let latestJSON = try await engine.clientRequest(request: #"{"op":"snapshot"}"#)
          let latest = try ZoenPushSnapshot(json: latestJSON)
          let background = await MainActor.run { UIApplication.shared.applicationState != .active }
          guard background, !Task.isCancelled, ContinuousClock.now < deadline,
                notificationIsCurrent(engine, expected, owner, lease), latest.notes(since: before).contains(note) else { throw CancellationError() }
        } catch {
          center.removePendingNotificationRequests(withIdentifiers: [note.identifier])
          center.removeDeliveredNotifications(withIdentifiers: [note.identifier])
          throw error
        }
        presented = true
      }
    }
    try Task.checkCancellation()
    guard ContinuousClock.now < deadline, notificationIsCurrent(engine, expected, owner, lease) else { throw SessionError.closed }
    notificationBaseline = afterJSON
    return presented ? .newData : .noData
  }
}
