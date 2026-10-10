package xyz.tironi.zoen.data

import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asSharedFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import xyz.tironi.zoen.core.AgentRequestDto
import xyz.tironi.zoen.core.RequestDecision
import xyz.tironi.zoen.core.RequestStatus

data class PendingApproval(val owner: String, val request: AgentRequestDto, val decision: RequestDecision, val ids: Set<String>)
data class ApprovalReviewState(val pending: PendingApproval? = null, val committing: Set<String> = emptySet(), val decided: Int = 0) {
    val hidden: Set<String> get() = committing + pending?.ids.orEmpty()
}

/** The undo window precedes the signed decision; undo never sends a compensating approval. */
class ApprovalReview(
    private val scope: CoroutineScope,
    private val app: StateFlow<AppState>,
    private val write: suspend (PendingApproval) -> Boolean,
    private val onError: (Exception) -> Unit,
) {
    private val mutableState = MutableStateFlow(ApprovalReviewState())
    val state = mutableState.asStateFlow()
    private val stored = MutableSharedFlow<Unit>(extraBufferCapacity = 1)
    val committed = stored.asSharedFlow()
    private var timer: Job? = null
    private var owner: String? = null

    fun reconcile(current: AppState) {
        if (owner != current.me?.id || current.keyMissing) {
            timer?.cancel(); timer = null
            mutableState.value = ApprovalReviewState()
            owner = current.me?.id
        }
    }

    fun queue(id: String, decision: RequestDecision): Boolean {
        val current = app.value
        reconcile(current)
        val me = current.me?.id ?: return false
        val request = current.requests.firstOrNull { it.id == id && it.agent.isMine && it.status == RequestStatus.PENDING } ?: return false
        if (current.keyMissing || id in state.value.hidden || (decision == RequestDecision.ALWAYS_APPROVE && !request.canAlwaysApprove)) return false
        flush()
        val standing = decision == RequestDecision.ALWAYS_APPROVE || decision == RequestDecision.ALWAYS_DENY
        val ids = if (standing) current.requests.filter {
            it.id !in state.value.hidden && it.agent.isMine && it.status == RequestStatus.PENDING && it.agent.id == request.agent.id &&
                it.spaceId == request.spaceId && it.actionKey == request.actionKey &&
                (decision == RequestDecision.ALWAYS_DENY || it.canAlwaysApprove)
        }.map { it.id }.toSet() else setOf(id)
        val pending = PendingApproval(me, request, decision, ids)
        mutableState.value = state.value.copy(pending = pending, decided = state.value.decided + ids.size)
        timer = scope.launch {
            delay(4_500)
            if (state.value.pending === pending) flush()
        }
        return true
    }

    fun undo(): Boolean {
        val pending = state.value.pending ?: return false
        timer?.cancel(); timer = null
        mutableState.value = state.value.copy(pending = null, decided = (state.value.decided - pending.ids.size).coerceAtLeast(0))
        return true
    }

    fun flush(): Job? {
        val pending = state.value.pending ?: return null
        timer?.cancel(); timer = null
        mutableState.value = state.value.copy(pending = null, committing = state.value.committing + pending.ids)
        return scope.launch {
            var accepted = false
            try {
                if (app.value.me?.id == pending.owner && !app.value.keyMissing) accepted = write(pending)
                if (accepted && app.value.me?.id == pending.owner && !app.value.keyMissing) stored.tryEmit(Unit)
            } catch (error: Exception) {
                if (error is CancellationException) throw error
                onError(error)
            } finally {
                if (app.value.me?.id == pending.owner) mutableState.value = state.value.copy(
                    committing = state.value.committing - pending.ids,
                    decided = (state.value.decided - if (accepted) 0 else pending.ids.size).coerceAtLeast(0),
                )
            }
        }
    }
}
