import ExpoModulesCore
import OSLog
import UIKit

/// Native OS entry, independent of React being mounted. No TaskManager runtime,
/// alternate account or payload-selected route. CNG registers this subscriber.
@MainActor
public final class ZoenPushDelegate: ExpoAppDelegateSubscriber {
  public func applicationDidBecomeActive(_ application: UIApplication) {
    Task { await ZoenNativePushSessions.gate.becameActive() }
  }

  public func application(
    _ application: UIApplication,
    didReceiveRemoteNotification userInfo: [AnyHashable: Any],
    fetchCompletionHandler completionHandler: @escaping (UIBackgroundFetchResult) -> Void
  ) {
    let info = Bundle.main.infoDictionary ?? [:]
    guard ZoenPushBuildAdmission.enabled(info, bundleIdentifier: Bundle.main.bundleIdentifier),
          ZoenPushWakePayload.accepts(userInfo), application.applicationState != .active else {
      completionHandler(.noData)
      return
    }
    let work = ZoenPushSystemWork(application: application, completion: completionHandler)
    work.start()
  }
}

/// An eight-second whole-callback ceiling, shortened by OS expiry. Duplicates do
/// not reset the process gate's in-flight work, and foreground open cancels it.
/// The OS may deny a background window or kill the process; the signed durable
/// log/cursors remain responsible for catch-up on the next permitted launch.
@MainActor
private final class ZoenPushSystemWork {
  private static let logger = Logger(subsystem: "xyz.tironi.zoen", category: "PushWake")
  private let application: UIApplication
  private let completion: (UIBackgroundFetchResult) -> Void
  private var identifier: UIBackgroundTaskIdentifier = .invalid
  private var worker: Task<Void, Never>?
  private var deadlineTask: Task<Void, Never>?
  private let completionGate = ZoenPushCompletion()

  init(application: UIApplication, completion: @escaping (UIBackgroundFetchResult) -> Void) {
    self.application = application
    self.completion = completion
  }

  func start() {
    Self.logger.notice("accepted")
    let deadline = ContinuousClock.now.advanced(by: .seconds(8))
    identifier = application.beginBackgroundTask(withName: "Zoen content-free sync") { [weak self] in
      Task { @MainActor in self?.finish(.failed) }
    }
    guard identifier != .invalid else { finish(.failed); return }
    deadlineTask = Task { [weak self] in
      do { try await Task.sleep(until: deadline, clock: .continuous) }
      catch { return }
      self?.finish(.failed)
    }
    worker = Task {
      let result = await ZoenNativePushSessions.wake(deadline: deadline)
      finish(result)
    }
  }

  private func finish(_ result: ZoenPushResult) {
    guard completionGate.claim() else { return }
    deadlineTask?.cancel()
    deadlineTask = nil
    worker?.cancel()
    worker = nil
    if identifier != .invalid {
      application.endBackgroundTask(identifier)
      identifier = .invalid
    }
    switch result {
    case .newData:
      Self.logger.notice("completed:newData")
      completion(.newData)
    case .noData:
      Self.logger.notice("completed:noData")
      completion(.noData)
    case .failed:
      Self.logger.notice("completed:failed")
      completion(.failed)
    }
  }
}
