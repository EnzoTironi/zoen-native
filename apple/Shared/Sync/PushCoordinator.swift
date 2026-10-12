import Foundation
import Observation

enum PushPermission: Equatable, Sendable {
    case unknown, notDetermined, denied, authorized, provisional, ephemeral
    var allowsNotifications: Bool {
        switch self {
        case .authorized, .provisional, .ephemeral: true
        default: false
        }
    }
}

enum PushRegistrationState: String, Equatable, Sendable {
    case unregistered, queued, registered, refused
}

struct PushRegistrationSnapshot: Equatable, Sendable {
    let supported: Bool
    let state: PushRegistrationState
    let environment: PushEnvironment?
    let topic: String?
    let deliveryAvailable: Bool
}

struct PushRegistrationRequest: Equatable, Sendable {
    let token: String?
    let environment: PushEnvironment
    let topic: String
}

@MainActor
protocol PushNotificationSystem: AnyObject {
    func permission() async throws -> PushPermission
    func requestPermission() async throws -> PushPermission
    func registerForRemoteNotifications()
    func unregisterForRemoteNotifications()
}

/// The adapter must use the current authenticated device and real Core snapshots.
/// synchronize validates membership and the signed log. It never navigates or acts.
@MainActor
protocol PushRegistrationStore: AnyObject {
    var deviceSessionId: String? { get }
    func snapshot() throws -> PushRegistrationSnapshot
    func setRegistration(_ request: PushRegistrationRequest) throws -> PushRegistrationSnapshot
    func synchronize(_ wake: PushWake) async throws -> Bool
}

@MainActor
@Observable
final class PushCoordinator {
    enum Failure: Equatable, Sendable { case permission, token, registration, sync }
    private(set) var permission = PushPermission.unknown
    private(set) var registration: PushRegistrationSnapshot?
    private(set) var failure: Failure?

    @ObservationIgnored private let system: any PushNotificationSystem
    @ObservationIgnored private let configuration: PushConfiguration?
    @ObservationIgnored private weak var store: (any PushRegistrationStore)?
    @ObservationIgnored private let retryDelays: [Duration]
    @ObservationIgnored private var retry: Task<Void, Never>?
    @ObservationIgnored private var retryCount = 0
    @ObservationIgnored private var epoch: UInt64 = 0
    @ObservationIgnored private var permissionRevision: UInt64 = 0
    @ObservationIgnored private var sessionId: String?
    @ObservationIgnored private var token: String?
    @ObservationIgnored private var lastAccepted: PushRegistrationRequest?
    @ObservationIgnored private var awaitingToken = false
    @ObservationIgnored private var prompting = false

    init(system: any PushNotificationSystem, configuration: PushConfiguration?, store: any PushRegistrationStore,
         retryDelays: [Duration] = [.seconds(10), .seconds(20), .seconds(40), .seconds(80), .seconds(160)]) {
        self.system = system
        self.configuration = configuration
        self.store = store
        self.retryDelays = Array(retryDelays.prefix(5))
        renewSession()
    }

    deinit { retry?.cancel() }

    /// Recheck Settings on foreground without showing a permission prompt.
    func foreground() async {
        renewSession()
        guard sessionId != nil, !prompting else { return }
        cancelRetry()
        retryCount = 0
        permissionRevision &+= 1
        let revision = permissionRevision
        let stamp = epoch
        do {
            let current = try await system.permission()
            guard currentSession(stamp), revision == permissionRevision else { return }
            permission = current
            reconcile()
        } catch {
            guard currentSession(stamp), revision == permissionRevision else { return }
            failure = .permission
        }
    }

    /// Call only from the person's explicit notification-permission action.
    func requestPermission() async {
        renewSession()
        guard sessionId != nil, !prompting else { return }
        prompting = true
        permissionRevision &+= 1
        let revision = permissionRevision
        let stamp = epoch
        defer { if stamp == epoch { prompting = false } }
        do {
            let current = try await system.requestPermission()
            guard currentSession(stamp), revision == permissionRevision else { return }
            permission = current
            reconcile()
        } catch {
            guard currentSession(stamp), revision == permissionRevision else { return }
            failure = .permission
        }
    }

    func receivedToken(_ bytes: Data) {
        renewSession()
        guard sessionId != nil, permission.allowsNotifications else { return }
        awaitingToken = false
        guard let value = PushConfiguration.tokenHex(bytes) else { failure = .token; return }
        token = value
        cancelRetry()
        retryCount = 0
        reconcile()
    }

    func registrationFailed() {
        renewSession()
        guard sessionId != nil, awaitingToken else { return }
        awaitingToken = false
        failure = .token
        scheduleRetry()
    }

    /// Core connectivity changes refresh status; queued work remains Core's outbox.
    func registrationChanged() {
        renewSession()
        reconcile()
    }

    func receiveWake(_ wake: PushWake) async -> Bool {
        renewSession()
        guard let store, sessionId != nil else { return false }
        let stamp = epoch
        do {
            let updated = try await store.synchronize(wake)
            guard currentSession(stamp) else { return false }
            return updated
        } catch {
            guard currentSession(stamp) else { return false }
            failure = .sync
            return false
        }
    }

    /// Persist the removal while this device still exists, then invalidate callbacks.
    @discardableResult
    func detach() -> Bool {
        renewSession()
        var persisted = true
        if let store, sessionId != nil, let configuration {
            do {
                _ = try store.setRegistration(PushRegistrationRequest(token: nil,
                    environment: configuration.environment, topic: configuration.topic))
            } catch { persisted = false }
        }
        store = nil
        renewSession()
        system.unregisterForRemoteNotifications()
        return persisted
    }

    private func renewSession() {
        let current = store?.deviceSessionId
        guard current != sessionId else { return }
        epoch &+= 1
        permissionRevision &+= 1
        sessionId = current
        token = nil
        lastAccepted = nil
        registration = nil
        permission = .unknown
        failure = nil
        prompting = false
        awaitingToken = false
        retryCount = 0
        cancelRetry()
    }

    private func currentSession(_ stamp: UInt64) -> Bool {
        guard stamp == epoch, sessionId != nil, sessionId == store?.deviceSessionId else {
            renewSession()
            return false
        }
        return true
    }

    private func reconcile() {
        guard let store, sessionId != nil, let configuration else { return }
        do {
            registration = try store.snapshot()
            guard registration?.supported == true, permission != .unknown else { return }
            if !permission.allowsNotifications {
                token = nil
                awaitingToken = false
                cancelRetry()
                system.unregisterForRemoteNotifications()
                try persist(token: nil, store: store, configuration: configuration)
            } else if let token {
                try persist(token: token, store: store, configuration: configuration)
            } else if !awaitingToken {
                awaitingToken = true
                system.registerForRemoteNotifications()
            }
        } catch {
            failure = .registration
            scheduleRetry()
        }
    }

    private func persist(token: String?, store: any PushRegistrationStore, configuration: PushConfiguration) throws {
        let request = PushRegistrationRequest(token: token, environment: configuration.environment, topic: configuration.topic)
        guard request != lastAccepted else { return }
        registration = try store.setRegistration(request)
        lastAccepted = request
        failure = nil
        cancelRetry()
        retryCount = 0
    }

    private func cancelRetry() { retry?.cancel(); retry = nil }

    private func scheduleRetry() {
        guard retry == nil, retryCount < retryDelays.count, sessionId != nil else { return }
        let delay = retryDelays[retryCount]
        retryCount += 1
        let stamp = epoch
        retry = Task { [weak self] in
            do { try await Task.sleep(for: delay) }
            catch { return }
            guard let self, self.currentSession(stamp) else { return }
            self.retry = nil
            self.reconcile()
        }
    }
}
