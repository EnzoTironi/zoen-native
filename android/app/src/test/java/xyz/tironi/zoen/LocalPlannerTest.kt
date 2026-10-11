package xyz.tironi.zoen

import org.junit.Assert.*
import org.junit.Test
import xyz.tironi.zoen.data.LocalPlanner

class LocalPlannerTest {
    @Test fun parsesBothCurrenciesWithoutDroppingThousandsOrCents() {
        assertEquals(150_025L, LocalPlanner.budget("até R$ 1.500,25"))
        assertEquals(150_025L, LocalPlanner.budget("up to $1,500.25"))
        assertEquals(150_000L, LocalPlanner.budget("1500 dollars"))
        assertEquals(150_000L, LocalPlanner.budget("1500 reais"))
    }
    @Test fun missingZeroAndOverflowingBudgetsDoNotInventMoney() {
        assertNull(LocalPlanner.budget("Help me organize my week"))
        assertNull(LocalPlanner.budget("up to $0"))
        assertNull(LocalPlanner.budget("$99999999999999999999999999"))
    }
    @Test fun tripDraftStaysWithinTheRequestedBudget() {
        val plan = LocalPlanner.plan("Plan a weekend trip up to $1,500", "en-US")
        assertEquals(150_000L, plan.budgetCents)
        val total = plan.sections.flatMap { it.lines }.sumOf { it.costCents }
        assertTrue(total > 0)
        assertTrue(total <= plan.budgetCents!!)
        assertTrue(plan.sections.any { it.title == "Transport" })
    }
    @Test fun portugueseDraftUsesPortugueseTasks() {
        val plan = LocalPlanner.plan("Organizar a semana", "pt-BR")
        assertEquals("Objetivo", plan.sections.first().title)
        assertTrue(plan.sections.flatMap { it.lines }.any { it.text == "Definir prazo" })
    }
    @Test fun smallBudgetsAreNotIncreasedByRounding() {
        for (cents in listOf(1L, 100L, 4_800L, 5_301L)) {
            val prompt = "Trip up to $" + "%d.%02d".format(java.util.Locale.US, cents / 100, cents % 100)
            val plan = LocalPlanner.plan(prompt, "en-US")
            assertTrue(plan.sections.flatMap { it.lines }.sumOf { it.costCents } <= cents)
        }
    }
}
