import Foundation
import UIKit
import ReactNativeEnrichedMarkdown

/// Token registries contain active leases only. A closed handle is never recreated by a request.
@MainActor final class ZoenPageInputProducer: NSObject {
  nonisolated let lease: ZoenPageInputLease
  nonisolated let mode: String
  nonisolated let initialText: String
  let initialSpans: [[String: Any]]
  nonisolated let label: String
  private var token: String?
  private var revoked = false
  private var discardProducer: (any ENRMDraftSnapshotProducer)?
  private weak var textView: ZoenPageTextInput?
  private static var textOwners: [String: ZoenPageInputProducer] = [:]

  init(lease: ZoenPageInputLease, mode: String, initialText: String,
                   initialSpans: [[String: Any]], label: String) {
    self.lease = lease; self.mode = mode; self.initialText = initialText
    self.initialSpans = initialSpans; self.label = label
    super.init()
  }

  func register() throws -> String {
    guard !revoked, lease.status().valid else { throw ZoenPageInputLease.Failure.closed }
    if let token { return token }
    let token = mode == "rich" ? ENRMDraftCheckpointRegistry.register(lease) : UUID().uuidString
    self.token = token
    if mode != "rich" { Self.textOwners[token] = self }
    NotificationCenter.default.addObserver(self, selector: #selector(backgroundCheckpoint),
      name: UIApplication.willResignActiveNotification, object: nil)
    return token
  }

  static func claimText(token: String, view: ZoenPageTextInput) throws -> ZoenPageInputProducer {
    guard let owner = textOwners[token], !owner.revoked, owner.lease.status().valid,
          owner.textView == nil || owner.textView === view else { throw ZoenPageInputLease.Failure.closed }
    owner.textView = view
    return owner
  }

  @objc private func backgroundCheckpoint() {
    let status = lease.status()
    guard !revoked, status.valid, status.everBound, !status.completed, !status.sealed,
          !status.failed, let token else { return }
    do {
      if mode == "rich" { try ENRMDraftCheckpointRegistry.flush(token: token) }
      else {
        guard let textView else { throw ZoenPageInputLease.Failure.unavailable }
        try textView.flush()
      }
    } catch { lease.draftCheckpointFailed(error) }
  }

  func seal() throws {
    guard !revoked, let token else { throw ZoenPageInputLease.Failure.unavailable }
    let status = lease.status()
    guard status.valid else { throw ZoenPageInputLease.Failure.closed }
    if status.sealed, !status.unretained, !status.failed { return }
    if mode == "rich" { try ENRMDraftCheckpointRegistry.seal(token: token) }
    else {
      guard let textView else { throw ZoenPageInputLease.Failure.unavailable }
      try textView.seal()
    }
    try lease.didSeal()
  }

  func freezeForDiscard() throws {
    guard !revoked, let token else { throw ZoenPageInputLease.Failure.unavailable }
    let status = lease.status()
    guard status.valid else { throw ZoenPageInputLease.Failure.closed }
    if status.everBound {
      if mode == "rich", discardProducer == nil {
        discardProducer = try ENRMDraftCheckpointRegistry.freezeForDiscard(token: token)
      } else if mode != "rich" {
        guard let textView else { throw ZoenPageInputLease.Failure.unavailable }
        textView.freeze()
      }
    }
    try lease.didFreezeForDiscard()
  }

  func sealForClose() throws {
    let status = lease.status()
    guard status.valid else { throw ZoenPageInputLease.Failure.closed }
    if status.completed { return }
    if !status.everBound {
      // Recovery already has a durable capture. An unused new binding can be released.
      if !status.retained { try lease.releaseEmpty() }
      return
    }
    try seal()
  }

  func revoke() {
    NotificationCenter.default.removeObserver(self)
    revoked = true
    if let token {
      if mode == "rich" { ENRMDraftCheckpointRegistry.revokeToken(token) }
      else { Self.textOwners.removeValue(forKey: token); textView?.freeze() }
    }
    token = nil
    discardProducer = nil
    textView = nil
  }
}

/// Module destruction fences synchronous sinks immediately, even before the actor can run.
final class ZoenPageInputLifetime: @unchecked Sendable {
  private let lock = NSLock()
  private var retired = false
  private var leases: [String: ZoenPageInputLease] = [:]
  func bind(_ lease: ZoenPageInputLease) throws {
    lock.lock(); defer { lock.unlock() }
    guard !retired else { lease.invalidate(); throw ZoenPageInputLease.Failure.closed }
    leases[lease.id] = lease
  }
  func remove(_ id: String) { lock.lock(); leases.removeValue(forKey: id); lock.unlock() }
  func retire() {
    lock.lock(); defer { lock.unlock() }
    retired = true
    for lease in leases.values { lease.invalidate() }
    leases.removeAll()
  }
}
