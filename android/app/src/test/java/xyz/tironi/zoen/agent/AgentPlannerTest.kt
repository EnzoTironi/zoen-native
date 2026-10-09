package xyz.tironi.zoen.agent

import kotlinx.coroutines.*
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.test.runTest
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.*
import org.junit.Test
import xyz.tironi.zoen.core.*

class AgentPlannerTest {
    private class Model(var status: ModelStatus, val answer: String = PLAN, val wait: Long = 0) : OnDeviceModel {
        var calls = 0
        var prompt = ""
        override suspend fun check() = ModelAvailability(status)
        override suspend fun generate(prompt: String, maxTokens: Int): String { calls++; this.prompt = prompt; delay(wait); return answer }
        override fun download(): Flow<ModelAvailability> = flowOf(ModelAvailability(ModelStatus.Downloading, 1000), ModelAvailability(ModelStatus.Ready))
        override fun close() = Unit
    }

    @Test fun supportedModelUsesRealGeneratedDraftAndChatContext() = runTest {
        val model = Model(ModelStatus.Ready)
        val planner = AgentPlanner(model)
        val draft = planner.makePlan("Plan dinner, up to $400", listOf("Marina"), "en", PlannerContext("Weekend", listOf("Ana"), "Dinner", listOf("Ana: Saturday works")))
        assertEquals("Dinner Saturday", draft.plan.title)
        assertEquals(40_000L, draft.plan.budgetCents)
        assertTrue(draft.engineLabel.contains("Gemini Nano"))
        assertTrue(model.prompt.contains("Ana: Saturday works"))
        assertTrue(model.prompt.contains("English"))
        assertEquals(1, model.calls)
    }

    @Test fun unavailableAndNotYetDownloadedModelsUseLabeledRulesWithoutInference() = runTest {
        for (status in listOf(ModelStatus.Unavailable, ModelStatus.Downloadable, ModelStatus.Downloading, ModelStatus.Failed)) {
            val model = Model(status)
            val draft = AgentPlanner(model).makePlan("Organizar a semana", emptyList(), "pt-BR")
            assertTrue(draft.engineLabel.contains("sem IA"))
            assertEquals("Objetivo", draft.plan.sections.first().title)
            assertEquals(0, model.calls)
        }
    }

    @Test fun generationDeadlineFallsBackExplicitly() = runTest {
        val model = Model(ModelStatus.Ready, wait = 60_000)
        val draft = AgentPlanner(model, 100).makePlan("Plan a trip", emptyList(), "en")
        assertTrue(draft.engineLabel.contains("timed out"))
        assertEquals(1, model.calls)
    }

    @Test fun invalidModelJsonCannotCreateAnUnvalidatedPlan() = runTest {
        val model = Model(ModelStatus.Ready, "{\"sections\":[],\"budgetCents\":999999999}")
        val draft = AgentPlanner(model).makePlan("Plan dinner, up to $5", emptyList(), "en")
        assertEquals(500L, draft.plan.budgetCents)
        assertTrue(draft.engineLabel.contains("invalid draft"))
        assertTrue(draft.plan.sections.flatMap { it.lines }.sumOf { it.costCents } <= 500)
    }

    @Test fun callerCancellationDoesNotBecomeAnAgentFallback() = runTest {
        val model = Model(ModelStatus.Ready, wait = 60_000)
        var completed = false
        val job = launch { AgentPlanner(model).makePlan("Plan dinner", emptyList(), "en"); completed = true }
        yield(); job.cancelAndJoin()
        assertFalse(completed)
    }

    @Test fun modelDownloadExposesProgressAndReadyStatus() = runTest {
        val planner = AgentPlanner(Model(ModelStatus.Downloadable))
        planner.refreshAvailability()
        assertEquals(ModelStatus.Downloadable, planner.availability.value.status)
        planner.downloadModel()
        assertEquals(ModelStatus.Ready, planner.availability.value.status)
    }

    @Test fun freeChatUsesTheModelAndDoesNotBecomeAPlan() = runTest {
        val model = Model(ModelStatus.Ready, "Podemos começar pelo que você precisa hoje.")
        val reply = AgentPlanner(model).reply("Como você pode ajudar?", "Zoen", "pt-BR", null)
        assertEquals(model.answer, reply)
        assertTrue(model.prompt.contains("Brazilian Portuguese"))
        assertFalse(AgentPlanner.looksLikePlanRequest("hello there"))
    }

    @Test fun starterPlanHasItsOwnShortDeadline() = runTest {
        val model = Model(ModelStatus.Ready, wait = 12_000)
        val draft = AgentPlanner(model).makeStarterPlan(listOf("Health", "Work"), "en")
        assertEquals(listOf("Health", "Work"), draft.plan.sections.map { it.title })
        assertTrue(draft.engineLabel.contains("timed out"))
    }

    @Test fun generatedCostsCannotOverrideBudgetOrInflateSmallAmounts() {
        for (budget in listOf(1L, 33L, 99L, 4_801L, Long.MAX_VALUE)) {
            val plan = AgentPlanner.parsePlan(PLAN, budget)
            assertEquals(budget, plan.budgetCents)
            assertTrue(plan.sections.flatMap { it.lines }.sumOf { it.costCents } <= budget)
            assertTrue(plan.sections.flatMap { it.lines }.all { it.costCents >= 0 && !it.done && it.id.isEmpty() })
        }
    }

    @Test fun schemaRejectsNegativeCostsHugeCostsAndEmptySections() {
        for (bad in listOf(PLAN.replace("200", "-1"), PLAN.replace("200", "99999999999999"), PLAN.replace("\"items\":[{\"text\":\"Buy dinner ingredients\",\"cost\":200}]", "\"items\":[]"))) {
            assertThrows(Exception::class.java) { AgentPlanner.parsePlan(bad, 1000) }
        }
    }

    @Test fun localAppChoiceCoversBothLanguagesAndOriginalIntent() {
        assertEquals("hike", AppChooser.kind("Planeja uma trilha amanhã"))
        assertEquals("pet", AppChooser.kind("Vamos adotar um burrinho chamado Jorge"))
        assertEquals("recipe", AppChooser.kind("jantar vegetariano para 4"))
        assertEquals("list", AppChooser.kind("Who brings what?"))
        assertEquals("poll", AppChooser.kind("Praia ou cachoeira?"))
        assertNull(AppChooser.kind("Tell me about your day"))
        assertEquals("Peanut", AppChooser.renameTarget("Let's call him Peanut"))
        assertEquals("Paçoca", AppChooser.renameTarget("vamos chamar ele de Paçoca"))
        assertEquals("Amanhã", AppChooser.hikeDay("a hike tomorrow", "pt-BR"))
    }

    @Test fun pollQuestionKeepsTheDayOutOfTheLastOption() {
        val (question, options) = AppChooser.pollParts("Zoen, faz uma enquete: praia ou cachoeira no sábado?", "pt-BR")
        assertTrue(question.contains("sábado"))
        assertEquals(listOf("Praia", "Cachoeira"), options)
    }

    @Test fun appModelCannotChooseExternalToolsOrSingleOptionPoll() {
        assertThrows(Exception::class.java) { AppChooser.parse("{\"kind\":\"send_payment\"}", "a poll", "en") }
        assertThrows(Exception::class.java) { AppChooser.parse("{\"kind\":\"poll\",\"title\":\"Dinner?\",\"options\":[\"one\"]}", "a poll", "en") }
    }

    @Test fun hikeUsesExplicitDriverAndRsvpWithoutInventingRealAddresses() {
        fun person(name: String) = Persona(name, PersonaKind.PERSON, name, name.lowercase(), name.take(1), "#ffffff", null, "", null, null, null, false, false)
        fun message(name: String, text: String, at: Long = 100) = TimelineEntry(name, 1uL, person(name), at, EntryKind.Message(text, null), Delivery.SENT)
        val entries = listOf(message("Lucas", "I can drive, room for 3"), message("Marina", "I'm in"), message("Ana", "conta comigo"))
        val demo = Json.parseToJsonElement(HikePlanner.plan(entries, 0, listOf("Enzo", "Marina"), true)!!).jsonObject
        assertEquals("Lucas", demo["driver"]!!.jsonPrimitive.content)
        assertEquals(3, demo["pickups"]!!.jsonArray.size)
        val real = HikePlanner.plan(entries, 0, emptyList(), false)!!
        assertFalse(real.contains("Hayes Valley"))
        assertFalse(real.contains("Mission"))
        assertNull(HikePlanner.plan(entries, 101, listOf("Enzo"), true))
    }

    @Test fun browserClosingOrClickingCannotHandControlBack() {
        var ownership = BrowserOwnership().needsYou().notNow().takeOver()
        assertEquals(BrowserPhase.Driving, ownership.phase)
        assertEquals(BrowserPhase.Driving, ownership.done().phase)
        ownership = ownership.text(8).backspace()
        assertEquals(7, ownership.typed)
        assertEquals(BrowserPhase.Driving, ownership.notNow().phase)
        assertEquals(BrowserPhase.Finished, ownership.done().phase)
        assertEquals(0, ownership.done().typed)
        assertEquals(0, BrowserOwnership().text(8).typed)
    }

    companion object {
        private const val PLAN = """{"title":"Dinner Saturday","summary":"Cook at home for the group.","sections":[{"title":"Food","items":[{"text":"Buy dinner ingredients","cost":200}]}]}"""
    }
}
