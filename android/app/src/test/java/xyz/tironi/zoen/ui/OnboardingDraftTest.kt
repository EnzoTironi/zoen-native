package xyz.tironi.zoen.ui

import org.junit.Assert.*
import org.junit.Test
import xyz.tironi.zoen.agent.PlanDraft
import xyz.tironi.zoen.core.PlanDto
import xyz.tironi.zoen.core.PlanLineDto
import xyz.tironi.zoen.core.PlanSectionDto

class OnboardingDraftTest {
    @Test fun savedPreviewRestoresExactTextCostsAndEngineWithoutGeneratingAgain() {
        val draft = PlanDraft(PlanDto("A calmer week", "Your own plan", null, listOf(PlanSectionDto("Money", listOf(PlanLineDto("", "Save for a trip 🌿", 12_345, false)))), 0), "Zoen · Gemini Nano on device")
        assertEquals(draft, OnboardingDraft.decode(OnboardingDraft.encode(draft)))
        assertNull(OnboardingDraft.decode("{\"title\":\"broken\"}"))
        assertNull(OnboardingDraft.decode(OnboardingDraft.encode(draft).replace("12345", "-1")))
    }
}
