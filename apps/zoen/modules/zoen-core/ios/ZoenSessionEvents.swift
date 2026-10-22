import ExpoModulesCore
import Foundation
import UIKit
import PDFKit
#if canImport(FoundationModels)
import FoundationModels
#endif


enum SessionError: Error, LocalizedError {
  case closed, invalidMode, invalidLocale, accountExists, locked, orphanedKeys
  var errorDescription: String? {
    switch self {
    case .closed: NSLocalizedString("This session has closed. Open the app again.", comment: "Native account session closed")
    case .invalidMode: NSLocalizedString("This session could not be opened.", comment: "Invalid native session mode")
    case .invalidLocale: NSLocalizedString("Invalid session language.", comment: "Invalid native session language")
    case .accountExists: NSLocalizedString("This device already has an account.", comment: "Account creation blocked on an occupied device")
    case .locked: NSLocalizedString("This account could not be unlocked with this device's keys. Use account recovery.", comment: "Native account unlock failed")
    case .orphanedKeys: NSLocalizedString("This device has account keys, but the database is unavailable. Recover the account to preserve those keys.", comment: "Account keys exist without an available database")
    }
  }
}

final class ZoenListener: CoreListener, @unchecked Sendable {
  private let changed: @Sendable ([String: Any]) -> Void
  private let resumeHistory: @Sendable () -> Void
  private let sourceChanged: @Sendable ([String]) -> Void
  private let connectionLock = NSLock()
  private var wasReady = false
  init(_ changed: @escaping @Sendable ([String: Any]) -> Void, sourceChanged: @escaping @Sendable ([String]) -> Void = { _ in }, resumeHistory: @escaping @Sendable () -> Void = {}) {
    self.changed = changed
    self.resumeHistory = resumeHistory
    self.sourceChanged = sourceChanged
  }
  func onChange(spaceIds: [String]) { sourceChanged(spaceIds); changed(["kind": "change", "spaceIds": spaceIds]) }
  func onEphemeral(spaceId: String, fromId: String, kind: String, detail: String) {
    changed(["kind": "ephemeral", "spaceId": spaceId, "fromId": fromId, "type": kind, "detail": detail])
    if kind == "history_ready", spaceId.isEmpty, detail.isEmpty { resumeHistory() }
  }
  func onPresence(identityId: String, online: Bool) { changed(["kind": "presence", "identityId": identityId, "online": online]) }
  func onConnection(status: ConnectionDto) {
    let ready = status.state == "online" && status.synced
    connectionLock.lock()
    let becameReady = ready && !wasReady
    wasReady = ready
    connectionLock.unlock()
    changed(["kind": "connection"])
    if becameReady { resumeHistory() }
  }
  func onError(message: String) { changed(["kind": "error", "message": message]) }
  func onProfileChanged(identityId: String) { changed(["kind": "profile", "identityId": identityId]) }
}

final class ZoenHistoryListener: TransferListener, @unchecked Sendable {
  private let listener: ZoenListener
  init(_ listener: ZoenListener) { self.listener = listener }
  func onProgress(done: UInt32, total: UInt32) { listener.onChange(spaceIds: []) }
}
