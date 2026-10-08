import Foundation
import RodaCore
#if canImport(FoundationModels)
import FoundationModels
#endif

struct PlanDraft {
    var plan: PlanDto
    /// Vai para a "origem" do Item, assinada no log: quem/como gerou.
    var engineLabel: String
}

/// O cérebro do agente neste protótipo.
///
/// 1. **Apple Foundation Models (no aparelho)** com geração guiada (`@Generable`) quando o
///    sistema disponibiliza o modelo (Apple Intelligence ligado e pronto).
/// 2. Senão, **`LocalFallbackPlanner`**: um planejador determinístico por regras,
///    claramente rotulado como fallback na origem do Item e na tela Você.
///
/// Nenhuma chave de API ou conta externa é usada. O orçamento (R$) é extraído do texto
/// por regra, nunca "inventado" pelo modelo.
@MainActor
final class AgentPlanner {
    enum Availability: Equatable {
        case onDevice
        case unavailable(String)
    }

    var availability: Availability {
        #if canImport(FoundationModels)
        switch SystemLanguageModel.default.availability {
        case .available: return .onDevice
        case .unavailable(let reason):
            switch reason {
            case .deviceNotEligible: return .unavailable(String(localized: "device doesn’t support Apple Intelligence"))
            case .appleIntelligenceNotEnabled: return .unavailable(String(localized: "Apple Intelligence is off in Settings"))
            case .modelNotReady: return .unavailable(String(localized: "model still downloading"))
            @unknown default: return .unavailable(String(localized: "unavailable"))
            }
        }
        #else
        return .unavailable(String(localized: "SDK without Foundation Models"))
        #endif
    }

    var availabilityLabel: String {
        switch availability {
        case .onDevice: String(localized: "Apple Intelligence · on device")
        case .unavailable(let why): String(localized: "Local planner (fallback) · \(why)")
        }
    }

    static func looksLikePlanRequest(_ text: String) -> Bool {
        let t = text.lowercased()
        let keys = ["planej", "plano", "organiz", "monta", "roteiro", "viagem", "fim de semana", "feriado", "festa", "jantar", "aniversário", "mudança", "lista de",
                    "plan", "organize", "itinerary", "trip", "weekend", "holiday", "party", "dinner", "birthday", "moving", "list of"]
        return keys.contains { t.contains($0) }
    }

    func makePlan(prompt: String, people: [String], context: PlannerContext? = nil) async -> PlanDraft {
        let budget = LocalFallbackPlanner.budgetCents(in: prompt)
        #if canImport(FoundationModels)
        if case .onDevice = availability {
            do {
                let plan = try await withTimeout(seconds: Self.onDeviceTimeout) {
                    try await self.generateOnDevice(prompt: prompt, people: people, budget: budget, context: context)
                }
                return PlanDraft(plan: plan, engineLabel: String(localized: "Zoen · Apple Intelligence on device (\(Money.format(0)))"))
            } catch is PlannerTimeout {
                let fallback = LocalFallbackPlanner.plan(for: prompt, people: people)
                return PlanDraft(plan: fallback, engineLabel: String(localized: "Zoen · local planner (fallback: the on-device model took longer than \(Int(Self.onDeviceTimeout)) s)"))
            } catch {
                let fallback = LocalFallbackPlanner.plan(for: prompt, people: people)
                return PlanDraft(plan: fallback, engineLabel: String(localized: "Zoen · local planner (fallback: the on-device model failed — \(error.localizedDescription))"))
            }
        }
        #endif
        return PlanDraft(plan: LocalFallbackPlanner.plan(for: prompt, people: people), engineLabel: String(localized: "Zoen · local planner (deterministic fallback, no AI)"))
    }

    /// Teto para o modelo no aparelho. No simulador o 3B pode levar 30–60 s; no
    /// aparelho é bem mais rápido. Passou disso, o fallback assume e diz que assumiu.
    static let onDeviceTimeout: Double = 45

    /// Onboarding's first plan: the aha has to land in seconds, so the on-device model
    /// gets a short budget (8 s, or `-RodaAppAITimeout`) before the labeled fallback.
    func makeStarterPlan(areas: [OnboardingArea], prompt: String) async -> PlanDraft {
        let d = UserDefaults.standard
        let timeout = d.object(forKey: "RodaAppAITimeout") != nil ? d.double(forKey: "RodaAppAITimeout") : 8
        #if canImport(FoundationModels)
        if case .onDevice = availability, timeout > 0 {
            let ask = prompt + " One short section per area, two concrete items each."
            do {
                let plan = try await withTimeout(seconds: timeout) {
                    try await self.generateOnDevice(prompt: ask, people: [], budget: nil, context: nil)
                }
                return PlanDraft(plan: plan, engineLabel: String(localized: "Zoen · Apple Intelligence on device (\(Money.format(0)))"))
            } catch is PlannerTimeout {
                return PlanDraft(plan: LocalFallbackPlanner.starter(areas: areas), engineLabel: String(localized: "Zoen · local planner (fallback: the on-device model took longer than \(Int(timeout)) s)"))
            } catch {
                return PlanDraft(plan: LocalFallbackPlanner.starter(areas: areas), engineLabel: String(localized: "Zoen · local planner (fallback: the on-device model failed — \(error.localizedDescription))"))
            }
        }
        #endif
        return PlanDraft(plan: LocalFallbackPlanner.starter(areas: areas), engineLabel: String(localized: "Zoen · local planner (deterministic fallback, no AI)"))
    }

    func reply(to text: String, agentName: String, context: PlannerContext? = nil) async -> String {
        #if canImport(FoundationModels)
        if case .onDevice = availability {
            do {
                let session = LanguageModelSession(instructions: """
                You are \(agentName), the user's personal agent inside the Roda messaging app.
                Answer in \(AppLocale.languageName), in at most two short sentences, warmly and without emoji.
                When it makes sense, offer to turn the request into a plan or a task.
                """)
                let response = try await session.respond(to: context.map { "\(text)\n\n\($0.promptBlock)" } ?? text)
                let content = response.content.trimmingCharacters(in: .whitespacesAndNewlines)
                if !content.isEmpty { return content }
            } catch {}
        }
        #endif
        // Fallback honesto, sem fingir inteligência.
        return String(localized: "I can’t chat freely without Apple Intelligence on this device yet, but I turn requests into plans. Try: “plan a dinner Saturday with Marina, up to \(Money.format(40_000))”.")
    }

    #if canImport(FoundationModels)
    private func generateOnDevice(prompt: String, people: [String], budget: Int64?, context: PlannerContext?) async throws -> PlanDto {
        let session = LanguageModelSession(instructions: """
        You are Zoen, an agent that turns one sentence into a concrete, editable plan.
        Write in \(AppLocale.languageName). Use real place names only when the request names a destination.
        Never invent a city, neighborhood or business the request didn't mention: with no destination, the plan is local and generic (at home, a nearby restaurant, etc.).
        Don't include transport, a car or a hotel unless the request involves travel.
        Be specific and short: each item fits on one line, with a verb or a concrete object. Costs are realistic estimates in whole \(AppLocale.isPortuguese ? "reais" : "US dollars") for the whole group; tasks with no cost are 0.
        """)
        var ask = "Request: \(prompt)\nPeople in the chat: \(people.joined(separator: ", "))."
        if let budget {
            ask += "\nThe total budget is \(Money.format(budget)). The costs must add up to between 80% and 95% of that."
        }
        if let context { ask += "\n" + context.promptBlock }
        let response = try await session.respond(to: ask, generating: GeneratedPlan.self)
        let g = response.content
        return PlanDto(
            title: g.title,
            summary: g.summary,
            budgetCents: budget,
            sections: g.sections.map { s in
                PlanSectionDto(title: s.title, lines: s.items.map { PlanLineDto(id: "", text: $0.text, costCents: Int64(max(0, $0.costReais)) * 100, done: false) })
            },
            totalCents: 0
        )
    }
    #endif
}

struct PlannerTimeout: Error {}

/// Invisible context for the on-device model: where the user is (chat, people nearby,
/// the open card, the last few messages). It never leaves the device and isn't shown;
/// the model is told to use it only when relevant.
struct PlannerContext: Sendable {
    var spaceTitle: String
    var names: [String]
    var openCard: String?
    var recent: [String]

    var promptBlock: String {
        var lines = ["Context the user doesn't see (use it only when it helps; never mention it):"]
        if !spaceTitle.isEmpty { lines.append("- Chat: “\(spaceTitle)”") }
        if !names.isEmpty { lines.append("- People here: \(names.joined(separator: ", "))") }
        if let openCard { lines.append("- Card open on screen: “\(openCard)”") }
        if !recent.isEmpty { lines.append("- Recent messages:\n" + recent.map { "  \($0)" }.joined(separator: "\n")) }
        return lines.joined(separator: "\n")
    }
}

/// Corre `work` contra um relógio; quem terminar primeiro vence e o outro é cancelado.
func withTimeout<T: Sendable>(seconds: Double, _ work: @escaping @Sendable () async throws -> T) async throws -> T {
    try await withThrowingTaskGroup(of: T.self) { group in
        group.addTask { try await work() }
        group.addTask {
            try await Task.sleep(for: .seconds(seconds))
            throw PlannerTimeout()
        }
        defer { group.cancelAll() }
        guard let first = try await group.next() else { throw PlannerTimeout() }
        return first
    }
}

#if canImport(FoundationModels)
@Generable
struct GeneratedPlan {
    @Guide(description: "Short, specific plan title in the user's language, no emoji. E.g. Weekend in Paraty")
    var title: String
    @Guide(description: "One line: when, who and the essentials of the plan")
    var summary: String
    @Guide(description: "2 to 3 sections that fit the request (trip: Transport, Stay, Tours; party: Guests, Food, Gift)", .count(2...3))
    var sections: [GeneratedSection]
}

@Generable
struct GeneratedSection {
    @Guide(description: "Short section name")
    var title: String
    @Guide(description: "1 to 3 concrete items", .count(1...3))
    var items: [GeneratedLine]
}

@Generable
struct GeneratedLine {
    @Guide(description: "Short, concrete item")
    var text: String
    @Guide(description: "Estimated cost in whole currency units for the whole group; 0 if free", .range(0...20000))
    var costReais: Int
}
#endif
