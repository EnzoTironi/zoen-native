package xyz.tironi.zoen.growth

import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.first
import xyz.tironi.zoen.core.AccountDto
import xyz.tironi.zoen.data.AppState

/** Optional reporting follows enrollment; it neither blocks onboarding nor survives an account swap. */
internal suspend fun reportGrowthWhenRegistered(
    state: Flow<AppState>,
    owner: AccountDto,
    report: suspend () -> Unit,
) {
    fun belongsToOwner(account: AccountDto?) = account != null && account.identityId == owner.identityId &&
        account.deviceId == owner.deviceId && account.relayUrl == owner.relayUrl

    val ready = state.first { current ->
        !belongsToOwner(current.account) ||
            current.account?.let { it.registered && it.unlocked } == true && current.connection.state == "online"
    }
    if (!belongsToOwner(ready.account)) return
    try {
        report()
    } catch (error: Exception) {
        if (error is CancellationException) throw error
        // This report is best effort, as on Apple. The core retains unaccepted metadata.
    }
}
