package xyz.tironi.zoen.ui

import org.junit.Assert.*
import org.junit.Test
import xyz.tironi.zoen.core.OnboardingPlanDto

class OnboardingRouteTest {
    @Test fun remoteFlowSkipsUnknownScreensAndAlwaysEndsInDone() {
        val route = OnboardingRoute.from(OnboardingPlanDto("friend", listOf("hello", "unknown", "profile", "areas"), "chat_with_inviter", "ana", mapOf("title" to "Hello", "title@pt" to "Olá"), "flow:b"))
        assertEquals(listOf(OnboardingStep.Hello, OnboardingStep.Profile, OnboardingStep.Areas, OnboardingStep.Done), route.steps)
        assertEquals("ana", route.target)
        assertEquals("Olá", route.text("title", "pt"))
        assertEquals("Hello", route.text("title", "en"))
    }
    @Test fun remoteConfigCannotRemoveRequiredProfileOrFinishEarly() {
        val missing = OnboardingRoute.from(OnboardingPlanDto("bad", listOf("hello", "done"), "home", null, emptyMap(), null))
        assertEquals(OnboardingStep.entries, missing.steps)
        val reordered = OnboardingRoute.from(OnboardingPlanDto("safe", listOf("done", "profile", "profile", "areas"), "space", "invite", emptyMap(), null))
        assertEquals(listOf(OnboardingStep.Profile, OnboardingStep.Areas, OnboardingStep.Done), reordered.steps)
    }
}
