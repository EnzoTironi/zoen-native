import Foundation
import RodaCore
#if canImport(FoundationModels)
import FoundationModels
#endif

/// Which mini-app (local MCP App) the agent opens from a chat message.
struct AppChoice: Equatable {
    /// Ferramenta `model` do servidor MCP local: `adopt_pet`, `start_poll`, `start_list`.
    var startTool: String
    var argsJSON: String
    /// Assinado na origem do Item: quem escolheu e como.
    var engineLabel: String
}

/// 1. Foundation Models no aparelho (`@Generable`) com teto curto: a graça do momento
///    é ser instantâneo.
/// 2. Senão, regras por palavra-chave, rotuladas como tal na origem do Item.
extension AppChoice {
    func withLabel(_ l: String) -> AppChoice { var c = self; c.engineLabel = l; return c }
}

@MainActor
enum AppChooser {
    static let onDeviceTimeout: Double = 12

    /// Frases que pedem um mini-app compartilhado, e não um plano.
    static func looksLikeAppRequest(_ text: String) -> Bool {
        kind(for: text) != nil
    }

    /// Keyword fallback in both languages (people mix them; the model handles the rest).
    static func kind(for text: String) -> String? {
        let t = " " + text.lowercased().folding(options: .diacriticInsensitive, locale: Locale(identifier: "en_US")) + " "
        let hike = ["trilha", "hike", "hiking", " trail", "caminhada", "trekking"]
        if hike.contains(where: t.contains) { return "hike" }
        let pet = ["adot", "adopt", "bichinho", "burro", "burrinho", "jumento", "donkey", "tamagotchi", "pet do grupo", "group pet", "a pet", "mascot"]
        if pet.contains(where: t.contains) { return "pet" }
        let poll = ["enquete", "votar", "votacao", "vamos decidir", "decide ai", "qual voces preferem", " poll", " vote", "let's decide", "let’s decide"]
        if poll.contains(where: t.contains) || ((t.contains(" ou ") || t.contains(" or ")) && t.contains("?") && !t.contains("planej") && !t.contains("plan ")) { return "poll" }
        let game = ["jogo de geografia", "geografia", "maptap", "jogo pra gente", "um jogo", "geography", "a game", "game for us"]
        if game.contains(where: t.contains) { return "maptap" }
        let recipe = ["receita", "jantar vegetariano", "jantar rapido", "o que cozinhar", "quem cozinha", "recipe", "vegetarian dinner", "what to cook", "who's cooking", "who’s cooking"]
        if recipe.contains(where: t.contains) { return "recipe" }
        let list = ["lista do que levar", "o que levar", "checklist", "lista compartilhada", "lista de compras", "quem leva o que", "what to bring", "packing list", "shared list", "shopping list", "who brings what"]
        if list.contains(where: t.contains) { return "list" }
        return nil
    }

    static func choose(prompt: String, planner: AgentPlanner) async -> AppChoice? {
        guard let kind = kind(for: prompt) else { return nil }
        // `-RodaAppAITimeout 0` pula o modelo (demos/capturas determinísticas).
        let d = UserDefaults.standard
        let timeout = d.object(forKey: "RodaAppAITimeout") != nil ? d.double(forKey: "RodaAppAITimeout") : onDeviceTimeout
        #if canImport(FoundationModels)
        // The on-device model's schema doesn't cover the hike yet: local rules pick the day.
        if case .onDevice = planner.availability, timeout > 0, kind != "hike" {
            do {
                let g = try await withTimeout(seconds: timeout) { try await generate(prompt: prompt, kind: kind) }
                if let choice = g { return choice }
            } catch {}
        }
        #endif
        return fallback(prompt: prompt, kind: kind)
    }

    // MARK: fallback determinístico

    static func fallback(prompt: String, kind: String) -> AppChoice {
        let label = String(localized: "Zoen · local rules (fallback, no AI)")
        switch kind {
        case "pet":
            let name = petName(in: prompt) ?? AppLocale.pick("Jumento", "Donkey")
            return AppChoice(startTool: "adopt_pet", argsJSON: json(["name": name]), engineLabel: label)
        case "maptap":
            return AppChoice(startTool: "start_maptap", argsJSON: "{}", engineLabel: label)
        case "hike":
            return AppChoice(startTool: "start_hike", argsJSON: json(["day": hikeDay(prompt), "area": "Bay Area"]), engineLabel: label)
        case "recipe":
            let n = Int(prompt.split(whereSeparator: { !$0.isNumber }).first ?? "") ?? 3
            return AppChoice(startTool: "start_recipe", argsJSON: json(["servings": min(12, max(1, n))]), engineLabel: label)
        case "poll":
            let (q, opts) = pollParts(prompt)
            return AppChoice(startTool: "start_poll", argsJSON: json(["question": q, "options": opts]), engineLabel: label)
        default:
            let items = AppLocale.isPortuguese ? ["Protetor solar", "Carregador", "Roupa de banho", "Documentos", "Remédios"] : ["Sunscreen", "Charger", "Swimsuit", "Documents", "Meds"]
            return AppChoice(startTool: "start_list", argsJSON: json(["title": AppLocale.pick("O que levar", "What to bring"), "items": items]), engineLabel: label)
        }
    }

    /// "…on saturday" / "…no domingo" → the day, in the app's language.
    static func hikeDay(_ text: String) -> String {
        let t = text.lowercased().folding(options: .diacriticInsensitive, locale: nil)
        let days: [(keys: [String], pt: String, en: String)] = [
            (["sunday", "domingo"], "Domingo", "Sunday"),
            (["tomorrow", "amanha"], "Amanhã", "Tomorrow"),
            (["friday", "sexta"], "Sexta", "Friday"),
        ]
        for d in days where d.keys.contains(where: t.contains) { return AppLocale.pick(d.pt, d.en) }
        return AppLocale.pick("Sábado", "Saturday")
    }

    /// "vamos chamar ele de Paçoca" / "let's call him Peanut" → the pet's new name.
    static func renameTarget(_ text: String) -> String? {
        for marker in ["chamar ele de ", "chamar de ", "o nome dele vai ser ", "o nome dele é ", "renomeia ele pra ", "renomear pra ",
                       "call him ", "call it ", "call her ", "name him ", "name it ", "his name is ", "rename him to ", "rename it to "] {
            if let r = text.range(of: marker, options: [.caseInsensitive, .diacriticInsensitive]) {
                let rest = text[r.upperBound...].trimmingCharacters(in: .whitespacesAndNewlines.union(.punctuationCharacters))
                let name = rest.split(separator: " ").prefix(2).joined(separator: " ")
                if !name.isEmpty { return String(name.prefix(18)) }
            }
        }
        return nil
    }

    /// "…um burro chamado Jorge" → Jorge.
    static func petName(in text: String) -> String? {
        for marker in ["chamado ", "chamada ", "nome de ", "se chama ", "named ", "called "] {
            if let r = text.range(of: marker, options: .caseInsensitive) {
                let word = text[r.upperBound...].split(whereSeparator: { !$0.isLetter }).first.map(String.init)
                if let word, !word.isEmpty { return word.prefix(1).uppercased() + word.dropFirst() }
            }
        }
        return nil
    }

    /// "Pousada Arte Urquijo ou Casa Turquesa?" → pergunta + 2 opções.
    static func pollParts(_ text: String) -> (String, [String]) {
        var body = text.trimmingCharacters(in: .whitespacesAndNewlines)
        if let colon = body.firstIndex(of: ":") { body = String(body[body.index(after: colon)...]) }
        // "Zoen, faz uma enquete praia ou…" sem dois-pontos: tira o vocativo e o pedido.
        for lead in ["zoen,", "zoen ", "faz uma enquete", "cria uma enquete", "enquete", "make a poll", "start a poll", "poll"] where body.lowercased().hasPrefix(lead) {
            body = String(body.dropFirst(lead.count))
        }
        body = body.trimmingCharacters(in: .whitespacesAndNewlines.union(CharacterSet(charactersIn: ",")))
        let question = body.isEmpty ? text : (body.prefix(1).uppercased() + body.dropFirst())
        let asked = question.hasSuffix("?") ? question : question + "?"
        body = body.replacingOccurrences(of: "?", with: "")
        let separator = body.contains(" ou ") ? " ou " : " or "
        let parts = body.components(separatedBy: separator).map { $0.trimmingCharacters(in: .whitespaces) }.filter { !$0.isEmpty }
        if parts.count >= 2 {
            var opts = parts
            // "vamos de praia ou cachoeira" → tira o começo da primeira opção ("vamos de").
            let first = opts[0].split(separator: " ")
            if first.count > 3 {
                let keep = min(3, max(1, opts[1].split(separator: " ").count))
                opts[0] = first.suffix(keep).joined(separator: " ")
            }
            // "praia ou cachoeira no sábado" → o "no sábado" é da pergunta, não da última opção.
            let firstWords = opts[0].split(separator: " ").count
            if var last = opts.last, last.split(separator: " ").count > firstWords {
                for prep in [" no ", " na ", " neste ", " nesse ", " pro ", " pra ", " para ", " amanhã", " hoje", " on ", " this ", " for ", " tomorrow", " today", " tonight"] {
                    if let r = last.range(of: prep) { last = String(last[..<r.lowerBound]); break }
                }
                opts[opts.count - 1] = last
            }
            opts = opts.map { $0.prefix(1).uppercased() + $0.dropFirst() }
            return (asked, Array(opts.prefix(6)))
        }
        return (asked, [String(localized: "Yes"), String(localized: "No")])
    }

    private static func json(_ v: [String: Any]) -> String {
        (try? JSONSerialization.data(withJSONObject: v)).flatMap { String(data: $0, encoding: .utf8) } ?? "{}"
    }

    // MARK: no aparelho

    #if canImport(FoundationModels)
    private static func generate(prompt: String, kind: String) async throws -> AppChoice? {
        let session = LanguageModelSession(instructions: """
        You pick and configure a shared mini-app for a group of friends in the Roda app.
        Mini-apps: pet (the group's virtual pet, a pixel-art donkey), poll (a vote to decide), list (shared list), maptap (geography game), recipe (dinner recipe).
        Answer in \(AppLocale.languageName): every title, name and option you write must be in that language. Don't invent places the conversation didn't mention.
        """)
        let r = try await session.respond(to: "Group message: \(prompt)\nRule-based suggestion: \(kind)", generating: GeneratedAppChoice.self)
        let g = r.content
        let label = String(localized: "Zoen · Apple Intelligence on device (\(Money.format(0)))")
        switch g.kind {
        case .pet:
            let name = g.petName.trimmingCharacters(in: .whitespaces)
            return AppChoice(startTool: "adopt_pet", argsJSON: json(["name": name.isEmpty ? AppLocale.pick("Burrico", "Donkey") : String(name.prefix(18))]), engineLabel: label)
        case .poll:
            let opts = g.options.map { $0.trimmingCharacters(in: .whitespaces) }.filter { !$0.isEmpty }
            guard opts.count >= 2 else { return nil }
            return AppChoice(startTool: "start_poll", argsJSON: json(["question": g.title, "options": Array(opts.prefix(6))]), engineLabel: label)
        case .maptap:
            return AppChoice(startTool: "start_maptap", argsJSON: "{}", engineLabel: label)
        case .recipe:
            return fallback(prompt: prompt, kind: "recipe").withLabel(label)
        case .list:
            return AppChoice(startTool: "start_list", argsJSON: json(["title": g.title, "items": Array(g.options.prefix(8))]), engineLabel: label)
        }
    }
    #endif
}

#if canImport(FoundationModels)
@Generable
enum GeneratedAppKind {
    case pet, poll, list, maptap, recipe
}

@Generable
struct GeneratedAppChoice {
    @Guide(description: "Which mini-app to open: pet to adopt/look after a pet, poll to decide between options, list to collect things, maptap for a geography game, recipe for a dinner recipe")
    var kind: GeneratedAppKind
    @Guide(description: "Short title in the user's language: the poll question or the list name. For pet, a short phrase")
    var title: String
    @Guide(description: "Short, friendly pet name if it's a pet; otherwise empty")
    var petName: String
    @Guide(description: "Poll options or list items, short, in the user's language", .count(0...6))
    var options: [String]
}
#endif
