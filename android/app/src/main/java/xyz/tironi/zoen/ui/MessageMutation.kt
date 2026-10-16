package xyz.tironi.zoen.ui

import kotlinx.coroutines.CancellationException

/** Dismiss only after the repository commits; cancellation is never a user-visible failure. */
internal suspend fun performMessageMutation(change: suspend () -> Unit, onSuccess: () -> Unit): String? {
    return try {
        change()
        onSuccess()
        null
    } catch (cancelled: CancellationException) {
        throw cancelled
    } catch (error: Exception) {
        error.message ?: "Não foi possível concluir. Tente novamente."
    }
}
