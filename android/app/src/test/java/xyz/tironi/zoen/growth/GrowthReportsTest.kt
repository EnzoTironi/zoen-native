package xyz.tironi.zoen.growth

import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.launch
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.*
import org.junit.Test
import xyz.tironi.zoen.core.AccountDto
import xyz.tironi.zoen.core.ConnectionDto
import xyz.tironi.zoen.data.AppState

@OptIn(ExperimentalCoroutinesApi::class)
class GrowthReportsTest {
    private fun account(registered: Boolean = false, unlocked: Boolean = true) =
        AccountDto("identity-a", "device-a", "Ana", "ana", "http://relay-a", registered, unlocked)

    private fun online(account: AccountDto) = AppState(
        account = account,
        connection = ConnectionDto("online", true, 0uL, null),
    )

    @Test fun pendingEnrollmentWaitsForRegisteredUnlockedOnlineAndReportsOnce() = runTest {
        val owner = account()
        val state = MutableStateFlow(AppState(account = owner))
        var calls = 0
        val job = launch { reportGrowthWhenRegistered(state, owner) { calls++ } }
        runCurrent(); assertEquals(0, calls)
        state.value = online(account(registered = false))
        runCurrent(); assertEquals(0, calls)
        state.value = online(account(registered = true, unlocked = false))
        runCurrent(); assertEquals(0, calls)
        state.value = AppState(account = account(registered = true))
        runCurrent(); assertEquals(0, calls)
        state.value = online(account(registered = true))
        job.join(); assertEquals(1, calls)
        state.value = AppState(account = account(registered = true))
        state.value = online(account(registered = true))
        runCurrent(); assertEquals(1, calls)
    }

    @Test fun signingOutOrChangingIdentityDeviceOrRelayAbandonsTheOldPendingReport() = runTest {
        val owner = account()
        for (replacement in listOf(
            null,
            owner.copy(identityId = "identity-b"),
            owner.copy(deviceId = "device-b"),
            owner.copy(relayUrl = "http://relay-b"),
        )) {
            val state = MutableStateFlow(AppState(account = owner))
            var calls = 0
            val job = launch { reportGrowthWhenRegistered(state, owner) { calls++ } }
            runCurrent()
            state.value = AppState(account = replacement)
            job.join(); assertEquals(0, calls)
        }
    }

    @Test fun reportRefusalDoesNotTurnCompletedOnboardingIntoAUserErrorOrRetry() = runTest {
        val owner = account(registered = true)
        val state = MutableStateFlow(online(owner))
        var calls = 0
        reportGrowthWhenRegistered(state, owner) {
            calls++
            throw IllegalStateException("report refused (401 Unauthorized)")
        }
        state.value = AppState(account = owner)
        state.value = online(owner)
        runCurrent(); assertEquals(1, calls)
    }

    @Test fun optionalReportingPreservesCancellation() = runTest {
        val owner = account(registered = true)
        val state = MutableStateFlow(online(owner))
        val cancellation = CancellationException("account job stopped")
        try {
            reportGrowthWhenRegistered(state, owner) { throw cancellation }
            fail("report cancellation must propagate")
        } catch (actual: CancellationException) {
            assertSame(cancellation, actual)
        }
        state.value = AppState(account = account())
        var calls = 0
        val job = launch { reportGrowthWhenRegistered(state, owner) { calls++ } }
        runCurrent(); job.cancel(); job.join()
        state.value = online(owner)
        runCurrent(); assertEquals(0, calls)
    }
}
