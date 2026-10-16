package xyz.tironi.zoen.ui

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue

/** Completion is observable independently of the optional landing destination. */
internal class OnboardingCompletion(initiallyCompleted: Boolean) {
    var completed by mutableStateOf(initiallyCompleted)
        private set

    fun finish(landing: String?, onLanding: (String?) -> Unit) {
        onLanding(landing)
        completed = true
    }
}
