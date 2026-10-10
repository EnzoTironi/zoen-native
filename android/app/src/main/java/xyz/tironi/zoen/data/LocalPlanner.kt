package xyz.tironi.zoen.data

import java.util.Locale
import java.math.RoundingMode
import xyz.tironi.zoen.core.PlanDto
import xyz.tironi.zoen.core.PlanLineDto
import xyz.tironi.zoen.core.PlanSectionDto

/** Deterministic EN/PT fallback; every resulting item names this engine. */
object LocalPlanner {
    private fun pt(locale: String) = Locale.forLanguageTag(locale).language == "pt"

    fun budget(text: String): Long? {
        val patterns = listOf(
            Regex("R\\$\\s*([0-9]+(?:\\.[0-9]{3})*)(?:,([0-9]{2}))?", RegexOption.IGNORE_CASE) to ".",
            Regex("(?<!R)\\$\\s*([0-9]+(?:,[0-9]{3})*)(?:\\.([0-9]{2}))?") to ",",
            Regex("([0-9]+(?:\\.[0-9]{3})*)\\s*(?:reais|conto)", RegexOption.IGNORE_CASE) to ".",
            Regex("([0-9]+(?:,[0-9]{3})*)\\s*(?:dollars|bucks|usd)", RegexOption.IGNORE_CASE) to ",",
        )
        for ((pattern, separator) in patterns) {
            val match = pattern.find(text) ?: continue
            val whole = match.groupValues[1].replace(separator, "").toLongOrNull() ?: continue
            if (whole > Long.MAX_VALUE / 100) continue
            val cents = match.groupValues.getOrNull(2)?.toLongOrNull() ?: 0
            if (whole * 100 > Long.MAX_VALUE - cents) continue
            return (whole * 100 + cents).takeIf { it > 0 }
        }
        return null
    }

    fun plan(prompt: String, locale: String, people: List<String> = emptyList(), companions: List<String> = people): PlanDto {
        val portuguese = pt(locale)
        fun l(en: String, br: String) = if (portuguese) br else en
        val text = prompt.trim().replace(Regex("^@?zoen[, ]*", RegexOption.IGNORE_CASE), "")
        val lower = text.lowercase(Locale.ROOT)
        val budget = budget(text)
        val place = destination(text, people)
        val companion = companion(text) ?: companions.firstOrNull()
        val trip = place != null || listOf("trip", "weekend", "travel", "beach", "holiday", "vacation", "getaway", "viagem", "viajar", "praia", "fim de semana", "feriado", "férias").any(lower::contains)
        val dinner = listOf("dinner", "party", "birthday", "barbecue", "celebrat", "jantar", "festa", "aniversário", "churrasco", "comemora").any(lower::contains)
        fun section(title: String, vararg lines: Pair<String, Long>) = PlanSectionDto(title,
            lines.map { PlanLineDto("", it.first, it.second, false) })
        fun share(amount: Long, part: Double) = amount.toBigDecimal().multiply(part.toBigDecimal()).setScale(0, RoundingMode.DOWN).longValueExact()
        var title = text.take(64).replaceFirstChar { it.titlecase() }
        var summary = l("First draft. Edit freely.", "Primeiro rascunho. Edite à vontade.")
        val sections = when {
            trip -> {
                val amount = budget ?: 150_000L
                title = place?.let { l("Weekend in $it", "Fim de semana em $it") } ?: l("Weekend trip", "Viagem de fim de semana")
                val with = companion?.let { l(" with $it", " com $it") }.orEmpty()
                summary = l("Friday to Sunday$with · transport, two nights and one main outing", "Sexta a domingo$with · transporte, duas noites e um passeio principal")
                listOf(
                    section(l("Transport", "Transporte"), (place?.let { l("Round trip to $it", "Ida e volta para $it") } ?: l("Round trip", "Ida e volta")) to share(amount, .17), l("Getting around", "Deslocamentos locais") to share(amount, .04)),
                    section(l("Stay", "Hospedagem"), l("Two nights, breakfast included", "Duas noites, café incluso") to share(amount, .42)),
                    section(l("Outings", "Passeios"), l("The main outing", "Passeio principal") to share(amount, .12), l("A beach or trail", "Praia ou trilha") to 0L),
                    section(l("Food", "Comida"), l("A special dinner", "Jantar especial") to share(amount, .14), l("Coffee and snacks", "Cafés e lanches") to share(amount, .05)),
                )
            }
            dinner -> {
                val amount = budget ?: 60_000L
                val birthday = lower.contains("birthday") || lower.contains("aniversário")
                title = if (birthday) companion?.let { l("$it’s birthday", "Aniversário de $it") } ?: l("Birthday", "Aniversário")
                    else companion?.let { l("Dinner with $it", "Jantar com $it") } ?: l("Dinner", "Jantar")
                summary = l("A reservation, food and one detail that makes the night", "Reserva, comida e um detalhe que faz a noite")
                listOf(
                    section(l("Place", "Lugar"), l("Book a table for the group", "Reservar mesa para o grupo") to 0L),
                    section(l("Food and drinks", "Comida e bebida"), l("Dinner", "Jantar") to share(amount, .65), l("Wine or drinks", "Vinho ou drinks") to share(amount, .2)),
                    section(l("Details", "Detalhes"), l("Cake or dessert", "Bolo ou sobremesa") to share(amount, .1), l("Message the guests", "Mensagem para os convidados") to 0L),
                )
            }
            else -> listOf(
                section(l("Goal", "Objetivo"), text.take(120) to 0L),
                section(l("Next steps", "Próximos passos"), l("Set a deadline", "Definir prazo") to 0L, l("List what's missing", "Listar o que falta") to 0L, l("Split the tasks", "Dividir tarefas") to (budget?.let { share(it, .5) } ?: 0L)),
            )
        }
        return PlanDto(title.take(80), summary.take(300), budget, sections, 0)
    }

    fun destination(text: String, people: List<String> = emptyList()): String? {
        val candidate = Regex("(?i:\\bem|\\bpara|\\bpra|\\bno|\\bna|\\bin|\\bto)\\s+((?:\\p{Lu}[\\p{L}\\p{M}]+)(?:\\s+(?:(?:de|do|da|dos|das)\\s+)?\\p{Lu}[\\p{L}\\p{M}]+)*)")
            .find(text)?.groupValues?.get(1) ?: return null
        val reserved = setOf("zoen", "financeiro", "organizador", "finance", "organizer", "home", "work", "casa",
            "monday", "tuesday", "wednesday", "thursday", "friday", "saturday", "sunday", "segunda", "terça", "quarta", "quinta", "sexta", "sábado", "domingo",
            "january", "february", "march", "april", "may", "june", "july", "august", "september", "october", "november", "december",
            "janeiro", "fevereiro", "março", "abril", "maio", "junho", "julho", "agosto", "setembro", "outubro", "novembro", "dezembro")
        val normalized = candidate.lowercase(Locale.ROOT)
        return candidate.take(64).takeUnless { normalized in reserved || people.any { it.equals(candidate, ignoreCase = true) } }
    }

    fun companion(text: String): String? = Regex("\\b(?i:com\\s+(?:a|o)\\s+|com\\s+|with\\s+)(\\p{Lu}[\\p{L}\\p{M}]+)")
        .find(text)?.groupValues?.get(1)?.take(40)
}
