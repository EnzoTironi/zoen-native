import Foundation
import RodaCore

/// ⚠️ FALLBACK DETERMINÍSTICO — não é IA.
///
/// Usado quando o Apple Foundation Models não está disponível (ex.: simulador sem
/// Apple Intelligence, Mac com o recurso desligado). Reconhece alguns tipos de pedido
/// por palavras-chave, extrai orçamento/destino/pessoas por regex e distribui o
/// orçamento por modelos fixos. Toda origem gerada aqui é rotulada como fallback.
enum LocalFallbackPlanner {
    static func plan(for prompt: String, people: [String]) -> PlanDto {
        let L = AppLocale.pick
        let lower = prompt.lowercased()
        let budget = budgetCents(in: prompt)
        let place = destination(in: prompt)
        let companion = companionName(in: prompt) ?? people.first { $0 != "Enzo" }

        let tripWords = ["viagem", "viajar", "fim de semana", "feriado", "praia", "férias", "trip", "travel", "weekend", "holiday", "beach", "vacation", "getaway"]
        let partyWords = ["jantar", "aniversário", "festa", "churrasco", "comemora", "dinner", "birthday", "party", "barbecue", "celebrat"]
        let isTrip = place != nil || tripWords.contains { lower.contains($0) }
        let isParty = partyWords.contains { lower.contains($0) }

        if isTrip {
            let b = budget ?? 150_000
            let where_ = place ?? L("a praia", "the beach")
            let with = companion.map { AppLocale.isPortuguese ? " com \(article(for: $0)) \($0)" : " with \($0)" } ?? ""
            return PlanDto(
                title: L("Fim de semana em \(where_)", "Weekend in \(where_)"),
                summary: L("Sexta a domingo\(with) · transporte, 2 noites e um passeio principal", "Friday to Sunday\(with) · transport, 2 nights and one main outing"),
                budgetCents: budget,
                sections: [
                    section(L("Transporte", "Transport"), [(L("Ida e volta para \(where_) (2 pessoas)", "Round trip to \(where_) (2 people)"), share(b, 0.17)), (L("Deslocamentos locais", "Getting around"), share(b, 0.04))]),
                    section(L("Hospedagem", "Stay"), [(L("Pousada bem avaliada · 2 noites, café incluso", "Well-rated inn · 2 nights, breakfast included"), share(b, 0.42))]),
                    section(L("Passeios", "Outings"), [(L("Passeio principal da região", "The area’s main tour"), share(b, 0.12)), (L("Praia ou trilha (grátis)", "Beach or trail (free)"), 0)]),
                    section(L("Comida", "Food"), [(L("Jantar especial", "A special dinner"), share(b, 0.14)), (L("Cafés e lanches", "Coffee and snacks"), share(b, 0.05))]),
                ],
                totalCents: 0
            )
        }

        if isParty {
            let b = budget ?? 60_000
            let title: String
            if lower.contains("aniversário") || lower.contains("birthday") {
                title = companion.map { L("Aniversário \(contraction(for: $0)) \($0)", "\($0)’s birthday") } ?? L("Aniversário", "Birthday")
            } else {
                title = companion.map { L("Jantar com \(article(for: $0)) \($0)", "Dinner with \($0)") } ?? L("Jantar", "Dinner")
            }
            return PlanDto(
                title: title,
                summary: L("Reserva, comida e um detalhe que faz a noite", "A reservation, food and one detail that makes the night"),
                budgetCents: budget,
                sections: [
                    section(L("Lugar", "Place"), [(L("Reservar mesa para o grupo", "Book a table for the group"), 0)]),
                    section(L("Comida e bebida", "Food and drinks"), [(L("Jantar", "Dinner"), share(b, 0.65)), (L("Vinho ou drinks", "Wine or cocktails"), share(b, 0.2))]),
                    section(L("Detalhes", "Details"), [(L("Bolo ou sobremesa", "Cake or dessert"), share(b, 0.1)), (L("Mensagem para os convidados", "Message to the guests"), 0)]),
                ],
                totalCents: 0
            )
        }

        let b = budget
        return PlanDto(
            title: headline(prompt),
            summary: L("Primeiro rascunho — edite à vontade", "First draft — edit freely"),
            budgetCents: b,
            sections: [
                section(L("Objetivo", "Goal"), [(headline(prompt), 0)]),
                section(L("Próximos passos", "Next steps"), [(L("Definir prazo", "Set a deadline"), 0), (L("Listar o que falta", "List what’s missing"), 0), (L("Dividir tarefas", "Split the tasks"), b.map { share($0, 0.5) } ?? 0)]),
            ],
            totalCents: 0
        )
    }

    /// "até R$ 1.500", "R$1500", "1.500 reais", "up to $1,500", "1500 dollars" → cents.
    static func budgetCents(in text: String) -> Int64? {
        let br = [#"R\$\s?([0-9]{1,3}(?:\.[0-9]{3})*|[0-9]+)(?:,([0-9]{2}))?"#, #"([0-9]{1,3}(?:\.[0-9]{3})*|[0-9]+)\s?(?:reais|conto)"#]
        let us = [#"(?<!R)\$\s?([0-9]{1,3}(?:,[0-9]{3})*|[0-9]+)(?:\.([0-9]{2}))?"#, #"([0-9]{1,3}(?:,[0-9]{3})*|[0-9]+)\s?(?:dollars|bucks|usd)"#]
        for (p, thousands) in br.map({ ($0, ".") }) + us.map({ ($0, ",") }) {
            guard let re = try? NSRegularExpression(pattern: p, options: .caseInsensitive),
                  let m = re.firstMatch(in: text, range: NSRange(text.startIndex..., in: text)),
                  let r = Range(m.range(at: 1), in: text) else { continue }
            let reais = Int64(text[r].replacingOccurrences(of: thousands, with: "")) ?? 0
            var cents = reais * 100
            if m.numberOfRanges > 2, let c = Range(m.range(at: 2), in: text) { cents += Int64(text[c]) ?? 0 }
            if cents > 0 { return cents }
        }
        return nil
    }

    static func destination(in text: String) -> String? {
        let p = #"(?:\bem|\bpara|\bpra|\bno|\bna|\bin|\bto)\s+((?:[A-ZÁÉÍÓÚÂÊÔÃÕ][\p{L}]+)(?:\s+(?:de|do|da|dos|das)?\s*[A-ZÁÉÍÓÚÂÊÔÃÕ][\p{L}]+)*)"#
        guard let re = try? NSRegularExpression(pattern: p),
              let m = re.firstMatch(in: text, range: NSRange(text.startIndex..., in: text)),
              let r = Range(m.range(at: 1), in: text) else { return nil }
        let candidate = String(text[r])
        let notPlaces: Set = ["Marina", "Enzo", "Ana", "Lucas", "Zoen", "Financeiro", "Organizador", "Finance", "Organizer", "Saturday", "Friday", "Sunday"]
        return notPlaces.contains(candidate) ? nil : candidate
    }

    static func companionName(in text: String) -> String? {
        let p = #"\b(?:com\s+(?:a|o)|with)\s+([A-ZÁÉÍÓÚ][\p{L}]+)"#
        guard let re = try? NSRegularExpression(pattern: p),
              let m = re.firstMatch(in: text, range: NSRange(text.startIndex..., in: text)),
              let r = Range(m.range(at: 1), in: text) else { return nil }
        return String(text[r])
    }

    private static func headline(_ prompt: String) -> String {
        var s = prompt.trimmingCharacters(in: .whitespacesAndNewlines)
        for prefix in ["@zoen", "zoen,", "zoen"] where s.lowercased().hasPrefix(prefix) {
            s = String(s.dropFirst(prefix.count)).trimmingCharacters(in: .whitespaces)
        }
        if s.count > 48 { s = String(s.prefix(46)) + "…" }
        return s.prefix(1).uppercased() + s.dropFirst()
    }

    private static func article(for name: String) -> String { name.hasSuffix("a") ? "a" : "o" }
    private static func contraction(for name: String) -> String { name.hasSuffix("a") ? "da" : "do" }

    /// Fração do orçamento, arredondada para dezenas de reais.
    private static func share(_ budget: Int64, _ f: Double) -> Int64 {
        Int64((Double(budget) * f / 1000).rounded()) * 1000
    }

    private static func section(_ title: String, _ lines: [(String, Int64)]) -> PlanSectionDto {
        PlanSectionDto(title: title, lines: lines.map { PlanLineDto(id: "", text: $0.0, costCents: $0.1, done: false) })
    }
}
