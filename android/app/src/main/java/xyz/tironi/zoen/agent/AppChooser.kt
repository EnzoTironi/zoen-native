package xyz.tironi.zoen.agent

import java.time.LocalDate
import java.time.ZoneId
import kotlinx.serialization.json.*
import xyz.tironi.zoen.agent.AgentPlanner.Companion.shortText

object AppChooser {
    fun kind(text: String): String? {
        val t = " ${folded(text)} "
        fun contains(vararg words: String) = words.any(t::contains)
        return when {
            contains("trilha", "hike", "hiking", " trail", "caminhada", "trekking") -> "hike"
            contains("adot", "adopt", "bichinho", "burro", "burrinho", "jumento", "donkey", "tamagotchi", "pet do grupo", "group pet", "a pet", "mascot") -> "pet"
            contains("enquete", "votar", "votacao", "vamos decidir", "decide ai", "qual voces preferem", " poll", " vote", "let's decide", "let’s decide") ||
                ((contains(" ou ", " or ")) && t.contains('?') && !contains("planej", "plan ")) -> "poll"
            contains("geografia", "maptap", "jogo pra gente", "um jogo", "geography", "a game", "game for us") -> "maptap"
            contains("receita", "jantar vegetariano", "jantar rapido", "o que cozinhar", "quem cozinha", "recipe", "vegetarian dinner", "what to cook", "who's cooking", "who’s cooking") -> "recipe"
            contains("lista do que levar", "o que levar", "checklist", "lista compartilhada", "lista de compras", "quem leva o que", "what to bring", "packing list", "shared list", "shopping list", "who brings what") -> "list"
            contains("countdown", "contagem regressiva") -> "countdown"
            else -> null
        }
    }

    fun renameTarget(text: String): String? {
        for (marker in listOf("chamar ele de ", "chamar de ", "o nome dele vai ser ", "o nome dele é ", "renomeia ele pra ", "renomear pra ", "call him ", "call it ", "call her ", "name him ", "name it ", "his name is ", "rename him to ", "rename it to ")) {
            val position = folded(text).indexOf(folded(marker))
            if (position >= 0) return text.substring(position + marker.length).trim { it.isWhitespace() || it in ",.!?\"'" }
                .split(Regex("\\s+")).take(2).joinToString(" ").take(18).takeIf(String::isNotBlank)
        }
        return null
    }

    private fun petName(text: String): String? {
        for (marker in listOf("chamado ", "chamada ", "nome de ", "se chama ", "named ", "called ")) {
            val position = text.indexOf(marker, ignoreCase = true)
            if (position >= 0) return text.substring(position + marker.length).trim().takeWhile(Char::isLetter).take(18).takeIf(String::isNotEmpty)?.replaceFirstChar(Char::titlecase)
        }
        return null
    }

    fun hikeDay(text: String, locale: String): String {
        val t = folded(text)
        return when {
            listOf("sunday", "domingo").any(t::contains) -> pick(locale, "Sunday", "Domingo")
            listOf("tomorrow", "amanha").any(t::contains) -> pick(locale, "Tomorrow", "Amanhã")
            listOf("friday", "sexta").any(t::contains) -> pick(locale, "Friday", "Sexta")
            else -> pick(locale, "Saturday", "Sábado")
        }
    }

    fun pollParts(text: String, locale: String): Pair<String, List<String>> {
        var body = text.substringAfter(':', text).trim()
        for (lead in listOf("@zoen,", "@zoen ", "zoen,", "zoen ", "faz uma enquete", "cria uma enquete", "enquete", "make a poll", "start a poll", "poll")) {
            if (body.startsWith(lead, ignoreCase = true)) body = body.drop(lead.length).trim()
        }
        body = body.trim(' ', ',')
        val question = body.ifEmpty { text }.let { if (it.endsWith('?')) it else "$it?" }.take(160)
        val options = body.replace("?", "").split(Regex("\\s+(ou|or)\\s+", RegexOption.IGNORE_CASE)).map(String::trim).filter(String::isNotBlank).toMutableList()
        if (options.size < 2) return question to listOf(pick(locale, "Yes", "Sim"), pick(locale, "No", "Não"))
        val first = options[0].split(' ')
        if (first.size > 3) options[0] = first.takeLast(options[1].split(' ').size.coerceIn(1, 3)).joinToString(" ")
        var last = options.last()
        if (last.split(' ').size > options.first().split(' ').size) {
            for (prep in listOf(" no ", " na ", " neste ", " nesse ", " pro ", " pra ", " para ", " amanhã", " hoje", " on ", " this ", " for ", " tomorrow", " today", " tonight")) {
                val position = last.indexOf(prep, ignoreCase = true)
                if (position >= 0) { last = last.take(position); break }
            }
            options[options.lastIndex] = last
        }
        return question to options.take(6).map { it.take(80).replaceFirstChar(Char::titlecase) }.filter(String::isNotBlank)
    }

    fun fallback(prompt: String, kind: String, locale: String, label: String): AppChoice? {
        val args = buildJsonObject {
            when (kind) {
                "pet" -> put("name", petName(prompt) ?: pick(locale, "Donkey", "Jumento"))
                "hike" -> { put("day", hikeDay(prompt, locale)); put("area", "Bay Area") }
                "recipe" -> put("servings", (Regex("\\d+").find(prompt)?.value?.toIntOrNull() ?: 3).coerceIn(1, 12))
                "poll" -> { val (question, options) = pollParts(prompt, locale); put("question", question); putJsonArray("options") { options.forEach { add(it) } } }
                "list" -> {
                    put("title", pick(locale, "What to bring", "O que levar"))
                    putJsonArray("items") { (if (portuguese(locale)) listOf("Protetor solar", "Carregador", "Roupa de banho", "Documentos", "Remédios") else listOf("Sunscreen", "Charger", "Swimsuit", "Documents", "Meds")).forEach { add(it) } }
                }
                "countdown" -> {
                    val date = Regex("\\d{4}-\\d{2}-\\d{2}").find(prompt)?.value ?: return null
                    val target = try { LocalDate.parse(date).atStartOfDay(ZoneId.systemDefault()).toInstant().toEpochMilli() } catch (_: Exception) { return null }
                    put("title", prompt.take(32)); put("target_ms", target)
                }
                "maptap" -> Unit
                else -> return null
            }
        }
        val tool = when (kind) { "pet" -> "adopt_pet"; else -> "start_$kind" }
        return AppChoice(tool, args.toString(), label)
    }

    fun parse(response: String, prompt: String, locale: String): AppChoice {
        val objectValue = AgentPlanner.objectFrom(response)
        val kind = objectValue.shortText("kind", 10)
        require(kind in setOf("pet", "poll", "list", "maptap", "recipe"))
        if (kind == "recipe" || kind == "maptap") return checkNotNull(fallback(prompt, kind, locale, ""))
        val args = buildJsonObject {
            when (kind) {
                "pet" -> put("name", objectValue.shortText("petName", 18))
                "poll", "list" -> {
                    val options = objectValue.getValue("options").jsonArray.map { element ->
                        val field = element.jsonPrimitive
                        require(field.isString)
                        field.content.also { value -> require(value.isNotBlank() && value.length <= 100) }
                    }.distinct()
                    require(options.size in (if (kind == "poll") 2..6 else 1..8))
                    put(if (kind == "poll") "question" else "title", objectValue.shortText("title", 160))
                    putJsonArray(if (kind == "poll") "options" else "items") { options.forEach { add(it) } }
                }
            }
        }
        return AppChoice(if (kind == "pet") "adopt_pet" else "start_$kind", args.toString(), "")
    }
}
