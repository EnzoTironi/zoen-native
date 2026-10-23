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
  func install(_ call: ZoenCallCoordinator) -> Bool {
    guard retirement == nil, current == nil || current === call else { return false }
    current = call; return true
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
    MainActor.assumeIsolated { audioActive = false; retirement?.systemDidReset(); current?.reset() }
  }
  nonisolated func provider(_ provider: CXProvider, perform action: CXStartCallAction) {
    MainActor.assumeIsolated {
      guard let current else { action.fail(); return }
      Task { await current.startAction(action) }
    }
  }
  nonisolated func provider(_ provider: CXProvider, perform action: CXAnswerCallAction) {
    MainActor.assumeIsolated {
      guard let current else { action.fail(); return }
      Task { await current.answerAction(action) }
    }
  }
  nonisolated func provider(_ provider: CXProvider, perform action: CXEndCallAction) {
    MainActor.assumeIsolated {
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
    MainActor.assumeIsolated { current?.reset(); action.fail() }
  }
}
