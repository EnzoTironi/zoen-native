import Foundation

/// A resource fence for one CallKit owner; no call authority or durable state.
/// System deactivation may arrive before, during or after asynchronous cleanup.
@MainActor
final class ZoenCallRetirement {
  let owner: UUID
  private var mediaStopped = false
  private var systemActive: Bool
  private var releaseStarted = false
  private var releaseFinished = false
  private var finished = false
  private let release: @MainActor () async -> Void
  private let completion: @MainActor (UUID) -> Void

  init(owner: UUID, systemActive: Bool, release: @escaping @MainActor () async -> Void,
       completion: @escaping @MainActor (UUID) -> Void) {
    self.owner = owner; self.systemActive = systemActive
    self.release = release; self.completion = completion
  }
  func mediaDidStop() { mediaStopped = true; advance() }
  func systemDidActivate() { systemActive = true }
  func systemDidDeactivate() { systemActive = false; advance() }
  func systemDidReset() { systemActive = false; advance() }

  private func advance() {
    guard !finished, mediaStopped, !systemActive else { return }
    if !releaseStarted {
      releaseStarted = true
      Task { await release(); releaseFinished = true; advance() }
    } else if releaseFinished {
      finished = true; completion(owner)
    }
  }
}
