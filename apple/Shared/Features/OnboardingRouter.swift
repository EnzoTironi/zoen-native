import SwiftUI
import RodaCore

/// The server-driven onboarding (ADR 0044): which steps, which copy and where it lands,
/// chosen by the core from the remote config, where this install came from (friend link,
/// Space link, campaign) and the install's experiment arm. The app only renders it.
struct OnboardingRoute {
    var flow = "default"
    var steps: [OnboardingFlow.Step] = OnboardingFlow.Step.allCases
    /// `home`, `chat_with_inviter` or `space`.
    var landing = "home"
    var target: String?
    var copy: [String: String] = [:]

    init() {}

    init(_ plan: OnboardingPlanDto) {
        flow = plan.flow
        let known = plan.steps.compactMap(OnboardingFlow.Step.init(id:))
        // A config can reorder or drop screens, never strand someone without a profile.
        steps = known.contains(.profile) ? known : OnboardingFlow.Step.allCases
        if steps.last != .done { steps.append(.done) }
        landing = plan.landing
        target = plan.landingTarget
        copy = plan.copy
    }

    /// Remote copy for `key` in the app's language, if the flow overrides it.
    func text(_ key: String) -> String? {
        let lang = Locale.current.language.languageCode?.identifier ?? "en"
        return copy["\(key)@\(lang)"] ?? copy[key]
    }

    func next(after s: OnboardingFlow.Step, by delta: Int) -> OnboardingFlow.Step? {
        guard let i = steps.firstIndex(of: s) else { return nil }
        let j = i + delta
        return steps.indices.contains(j) ? steps[j] : nil
    }
}

extension OnboardingFlow.Step {
    init?(id: String) {
        switch id {
        case "hello": self = .hello
        case "profile": self = .profile
        case "areas": self = .areas
        case "plan": self = .plan
        case "agents": self = .agents
        case "notifications": self = .notifications
        case "location": self = .location
        case "done": self = .done
        default: return nil
        }
    }

    var id: String {
        switch self {
        case .hello: "hello"
        case .profile: "profile"
        case .areas: "areas"
        case .plan: "plan"
        case .agents: "agents"
        case .notifications: "notifications"
        case .location: "location"
        case .done: "done"
        }
    }
}

extension AppModel {
    /// A link opened the app (custom scheme or universal link): remember where this install
    /// came from (first touch only; the friend or code never leaves the device).
    func captureAcquisition(_ url: URL) {
        _ = core.growthCaptureLink(link: url.absoluteString)
    }

    /// Refreshes the remote config and sends what's pending (source once, arms shown).
    func growthSync() async {
        _ = try? await core.growthSync(relayUrl: sync.account?.relayUrl ?? SyncModel.defaultRelayURL,
                                       shareHealth: false, sessions: 0, crashes: 0)
    }

    /// Where onboarding ends: the chat with the friend who invited, the Space of the link,
    /// or the usual home. Falls back to home if the friend or Space can't be reached.
    func land(_ route: OnboardingRoute) async -> Bool {
        guard let target = route.target, !target.isEmpty else { return false }
        do {
            switch route.landing {
            case "chat_with_inviter":
                let people = try await core.findPeople(query: target)
                guard let friend = people.first(where: { $0.handle.lowercased() == target.lowercased() }) else { return false }
                let id = try core.startDirect(identityId: friend.id)
                refresh()
                go(.space(id))
                return true
            case "space":
                let id = try await core.joinInvite(code: target)
                refresh()
                go(.space(id))
                return true
            default:
                return false
            }
        } catch {
            return false
        }
    }
}
