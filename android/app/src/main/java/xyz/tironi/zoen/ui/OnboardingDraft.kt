package xyz.tironi.zoen.ui

import kotlinx.serialization.json.*
import xyz.tironi.zoen.agent.PlanDraft
import xyz.tironi.zoen.core.PlanDto
import xyz.tironi.zoen.core.PlanLineDto
import xyz.tironi.zoen.core.PlanSectionDto

internal object OnboardingDraft {
    fun encode(draft: PlanDraft): String = buildJsonObject {
        put("engine", draft.engineLabel)
        put("title", draft.plan.title); put("summary", draft.plan.summary)
        putJsonArray("sections") { draft.plan.sections.forEach { section -> add(buildJsonObject {
            put("title", section.title)
            putJsonArray("lines") { section.lines.forEach { line -> add(buildJsonObject { put("text", line.text); put("cost", line.costCents) }) } }
        }) } }
    }.toString()

    fun decode(value: String): PlanDraft? = runCatching {
        require(value.length <= 32_000)
        val root = Json.parseToJsonElement(value).jsonObject
        fun JsonObject.text(key: String, max: Int) = getValue(key).jsonPrimitive.content.also { require(it.length in 1..max) }
        val sections = root.getValue("sections").jsonArray.also { require(it.size in 1..8) }.map { element ->
            val section = element.jsonObject
            PlanSectionDto(section.text("title", 64), section.getValue("lines").jsonArray.also { require(it.size in 1..4) }.map { entry ->
                val line = entry.jsonObject
                PlanLineDto("", line.text("text", 240), line.getValue("cost").jsonPrimitive.long.also { require(it in 0..2_000_000) }, false)
            })
        }
        PlanDraft(PlanDto(root.text("title", 80), root.text("summary", 300), null, sections, 0), root.text("engine", 500))
    }.getOrNull()
}
