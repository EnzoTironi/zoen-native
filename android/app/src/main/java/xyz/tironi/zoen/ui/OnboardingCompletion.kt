package xyz.tironi.zoen.ui

import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue

/** Keep completion alive while recovery replaces the account identity. */
@Composable
internal fun rememberOnboardingCompletion(owner: Any, demo: Boolean, initiallyCompleted: () -> Boolean): OnboardingCompletion =
    remember(owner, demo) { OnboardingCompletion(initiallyCompleted()) }

/** Completion is observable independently of the optional landing destination. */
internal class OnboardingCompletion(initiallyCompleted: Boolean) {
    var completed by mutableStateOf(initiallyCompleted)
        private set

    fun finish(landing: String?, onLanding: (String?) -> Unit) {
        onLanding(landing)
        completed = true
    }
}
