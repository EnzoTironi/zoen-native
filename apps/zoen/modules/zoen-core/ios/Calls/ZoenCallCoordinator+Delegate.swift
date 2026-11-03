import AVFoundation
import CallKit

/// One CXProvider for the process, independent of React mounts and call screens.
/// The configured main delegate queue preserves OS audio activation ordering.
@MainActor
final class ZoenCallSystem: NSObject, CXProviderDelegate {
  static let shared = ZoenCallSystem()
  let provider: CXProvider
  private(set) var audioActive = false
  private var retirement: ZoenCallRetirement?
  private var pendingWake: UUID?
  private var pendingAnswer: CXAnswerCallAction?
  private var current: ZoenCallCoordinator?
  private override init() {
    let configuration = CXProviderConfiguration()
    configuration.supportsVideo = true
    configuration.maximumCallGroups = 1
    configuration.maximumCallsPerCallGroup = 1
    configuration.supportedHandleTypes = [.generic]
    configuration.includesCallsInRecents = false
    provider = CXProvider(configuration: configuration)
    super.init()
    provider.setDelegate(self, queue: .main)
  }
  func reportWake(id: UUID, completion: @escaping (Bool) -> Void) {
    let update = CXCallUpdate()
    update.remoteHandle = CXHandle(type: .generic, value: Locale.preferredLanguages.first?.hasPrefix("pt") == true ? "Chamada" : "Call")
    update.supportsDTMF = false
    update.supportsGrouping = false
    update.supportsUngrouping = false
    let available = current == nil && retirement == nil && pendingWake == nil
    if available { pendingWake = id }
    provider.reportNewIncomingCall(with: id, update: update) { error in
      Task { @MainActor in
        guard available, self.pendingWake == id, error == nil else {
          if self.pendingWake == id && available { self.finishWake(id: id, reportEnd: false) }
          if error == nil && self.current?.id != id && self.pendingWake != id {
            self.provider.reportCall(with: id, endedAt: nil, reason: .failed)
          }
          completion(false)
          return
        }
        completion(true)
      }
    }
  }

  func finishWake(id: UUID, reportEnd: Bool = true) {
    guard pendingWake == id else { return }
    pendingWake = nil
    pendingAnswer?.fail()
    pendingAnswer = nil
    if reportEnd { provider.reportCall(with: id, endedAt: nil, reason: .failed) }
  }

  func install(_ call: ZoenCallCoordinator, adoptingWake: Bool = false) -> Bool {
    guard retirement == nil, current == nil || current === call else { return false }
    if adoptingWake {
      guard pendingWake == call.id else { return false }
      pendingWake = nil
    } else {
      guard pendingWake == nil else { return false }
    }
    current = call
    if let answer = pendingAnswer {
      pendingAnswer = nil
      Task { await call.answerAction(answer) }
    }
    return true
  }
  func retire(_ call: ZoenCallCoordinator, release: @escaping @MainActor () async -> Void) -> ZoenCallRetirement {
    let fence = ZoenCallRetirement(owner: call.id, systemActive: audioActive, release: release) { [weak self] owner in
      guard self?.retirement?.owner == owner else { return }
      self?.retirement = nil
    }
    retirement = fence
    return fence
  }
  func remove(_ call: ZoenCallCoordinator) { if current === call { current = nil } }
  nonisolated func providerDidReset(_ provider: CXProvider) {
    MainActor.assumeIsolated {
      if let pendingWake { finishWake(id: pendingWake, reportEnd: false) }
      audioActive = false; retirement?.systemDidReset(); current?.reset()
    }
  }
  nonisolated func provider(_ provider: CXProvider, perform action: CXStartCallAction) {
    MainActor.assumeIsolated {
      guard let current else { action.fail(); return }
      Task { await current.startAction(action) }
    }
  }
  nonisolated func provider(_ provider: CXProvider, perform action: CXAnswerCallAction) {
    MainActor.assumeIsolated {
      if pendingWake == action.callUUID, pendingAnswer == nil {
        pendingAnswer = action
        return
      }
      guard let current else { action.fail(); return }
      Task { await current.answerAction(action) }
    }
  }
  nonisolated func provider(_ provider: CXProvider, perform action: CXEndCallAction) {
    MainActor.assumeIsolated {
      if pendingWake == action.callUUID {
        finishWake(id: action.callUUID, reportEnd: false)
        action.fulfill()
        return
      }
      guard let current else { action.fail(); return }
      Task { await current.endAction(action) }
    }
  }
  nonisolated func provider(_ provider: CXProvider, perform action: CXSetMutedCallAction) {
    MainActor.assumeIsolated { guard let current else { action.fail(); return }; current.muteAction(action) }
  }
  nonisolated func provider(_ provider: CXProvider, didActivate audioSession: AVAudioSession) {
    MainActor.assumeIsolated { audioActive = true; retirement?.systemDidActivate(); current?.activate(audioSession) }
  }
  nonisolated func provider(_ provider: CXProvider, didDeactivate audioSession: AVAudioSession) {
    MainActor.assumeIsolated { audioActive = false; retirement?.systemDidDeactivate(); current?.deactivate(audioSession) }
  }
  nonisolated func provider(_ provider: CXProvider, timedOutPerforming action: CXAction) {
    MainActor.assumeIsolated {
      if pendingAnswer === action { pendingAnswer = nil }
      else if let callAction = action as? CXCallAction, let current, current.id == callAction.callUUID {
        current.reset()
      }
      action.fail()
    }
  }
}
