package xyz.tironi.zoen.ui

import xyz.tironi.zoen.core.OnboardingPlanDto

enum class OnboardingStep(val id: String, val title: Int, val detail: Int) {
    Hello("hello", xyz.tironi.zoen.R.string.welcome_title, xyz.tironi.zoen.R.string.welcome_detail),
    Profile("profile", xyz.tironi.zoen.R.string.onboarding_profile_title, xyz.tironi.zoen.R.string.account_detail),
    Areas("areas", xyz.tironi.zoen.R.string.areas_title, xyz.tironi.zoen.R.string.areas_detail),
    Plan("plan", xyz.tironi.zoen.R.string.plan_title, xyz.tironi.zoen.R.string.plan_detail),
    Agents("agents", xyz.tironi.zoen.R.string.trust_title, xyz.tironi.zoen.R.string.trust_detail),
    Notifications("notifications", xyz.tironi.zoen.R.string.notifications_title, xyz.tironi.zoen.R.string.notifications_detail),
    Location("location", xyz.tironi.zoen.R.string.location_title, xyz.tironi.zoen.R.string.onboarding_location_detail),
    Done("done", xyz.tironi.zoen.R.string.ready_title, xyz.tironi.zoen.R.string.onboarding_done_detail),
}

data class OnboardingRoute(
    val flow: String = "default",
    val steps: List<OnboardingStep> = OnboardingStep.entries,
    val landing: String = "home",
    val target: String? = null,
    val copy: Map<String, String> = emptyMap(),
) {
    fun text(key: String, language: String): String? = copy["$key@$language"] ?: copy[key]

    companion object {
        fun from(plan: OnboardingPlanDto): OnboardingRoute {
            val known = plan.steps.mapNotNull { id -> OnboardingStep.entries.firstOrNull { it.id == id } }.distinct()
            val safe = if (OnboardingStep.Profile in known) known else OnboardingStep.entries
            val steps = safe.filterNot { it == OnboardingStep.Done } + OnboardingStep.Done
            return OnboardingRoute(plan.flow, steps, plan.landing, plan.landingTarget, plan.copy)
        }
    }
}
