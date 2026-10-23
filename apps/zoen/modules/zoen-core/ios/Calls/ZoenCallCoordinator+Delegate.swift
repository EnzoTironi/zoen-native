import AVFoundation
import CallKit

/// One CXProvider for the process, independent of React mounts and call screens.
/// The configured main delegate queue preserves OS audio activation ordering.
@MainActor
final class ZoenCallSystem: NSObject, CXProviderDelegate {
  static let shared = ZoenCallSystem()
  let provider: CXProvider
  var retiring = false
  var afterDeactivation: (@MainActor () -> Void)?
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
    guard !retiring, afterDeactivation == nil, current == nil || current === call else { return false }
    current = call; return true
  }
  func remove(_ call: ZoenCallCoordinator) { if current === call { current = nil } }
  nonisolated func providerDidReset(_ provider: CXProvider) {
    MainActor.assumeIsolated { current?.reset(); let release = afterDeactivation; afterDeactivation = nil; release?() }
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
    MainActor.assumeIsolated { current?.activate(audioSession) }
  }
  nonisolated func provider(_ provider: CXProvider, didDeactivate audioSession: AVAudioSession) {
    MainActor.assumeIsolated { current?.deactivate(audioSession); let release = afterDeactivation; afterDeactivation = nil; release?() }
  }
  nonisolated func provider(_ provider: CXProvider, timedOutPerforming action: CXAction) {
    MainActor.assumeIsolated { current?.reset(); action.fail() }
  }
}
