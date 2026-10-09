package xyz.tironi.zoen.data

import java.util.Locale
import java.math.RoundingMode
import xyz.tironi.zoen.core.PlanDto
import xyz.tironi.zoen.core.PlanLineDto
import xyz.tironi.zoen.core.PlanSectionDto

/** The same deterministic fallback as Apple. Every resulting item names this engine. */
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

    fun plan(prompt: String, locale: String): PlanDto {
        val portuguese = pt(locale)
        fun l(en: String, br: String) = if (portuguese) br else en
        val text = prompt.trim().replace(Regex("^@?zoen[, ]*", RegexOption.IGNORE_CASE), "")
        val lower = text.lowercase(Locale.ROOT)
        val budget = budget(text)
        val trip = listOf("trip", "weekend", "travel", "beach", "viagem", "praia", "fim de semana").any(lower::contains)
        val dinner = listOf("dinner", "party", "birthday", "jantar", "festa", "aniversário").any(lower::contains)
        fun section(title: String, vararg lines: Pair<String, Long>) = PlanSectionDto(title,
            lines.map { PlanLineDto("", it.first, it.second, false) })
        fun share(amount: Long, part: Double) = amount.toBigDecimal().multiply(part.toBigDecimal()).setScale(0, RoundingMode.DOWN).longValueExact()
        val sections = when {
            trip -> {
                val amount = budget ?: 150_000L
                listOf(
                    section(l("Transport", "Transporte"), l("Round trip", "Ida e volta") to share(amount, .17), l("Getting around", "Deslocamentos locais") to share(amount, .04)),
                    section(l("Stay", "Hospedagem"), l("Two nights, breakfast included", "Duas noites, café incluso") to share(amount, .42)),
                    section(l("Outings", "Passeios"), l("The main outing", "Passeio principal") to share(amount, .12), l("A beach or trail", "Praia ou trilha") to 0L),
                    section(l("Food", "Comida"), l("A special dinner", "Jantar especial") to share(amount, .14), l("Coffee and snacks", "Cafés e lanches") to share(amount, .05)),
                )
            }
            dinner -> {
                val amount = budget ?: 60_000L
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
        return PlanDto(text.take(64).replaceFirstChar { it.titlecase() }, l("First draft. Edit freely.", "Primeiro rascunho. Edite à vontade."), budget, sections, 0)
    }

}
