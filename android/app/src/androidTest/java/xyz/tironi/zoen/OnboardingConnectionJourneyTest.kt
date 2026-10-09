package xyz.tironi.zoen

import android.app.Notification
import android.app.NotificationManager
import android.content.Intent
import android.os.Build
import android.view.WindowManager
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createEmptyComposeRule
import androidx.lifecycle.Lifecycle
import androidx.test.core.app.ActivityScenario
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import java.io.File
import java.util.UUID
import kotlinx.coroutines.runBlocking
import org.junit.Assert.*
import org.junit.Assume.assumeTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import xyz.tironi.zoen.background.MessagingService
import xyz.tironi.zoen.core.*
import xyz.tironi.zoen.data.AndroidSecretVault

@RunWith(AndroidJUnit4::class)
class OnboardingConnectionJourneyTest {
    @get:Rule val compose = createEmptyComposeRule()

    @Test fun createsAllEightAreasThenReceivesEncryptedMessagesThroughTheNativeForegroundConnection() {
        val relay = InstrumentationRegistry.getArguments().getString("zoenRelay")
        assumeTrue("Requires the isolated test relay", !relay.isNullOrBlank())
        val application = ApplicationProvider.getApplicationContext<ZoenApplication>()
        val repository = application.repository
        val wasDemo = repository.preferences.getBoolean("demo", false)
        val wasOnboarded = repository.preferences.getBoolean("onboarded", false)
        val wasBackground = repository.preferences.getBoolean(MessagingService.PREFERENCE, false)
        val suffix = UUID.randomUUID().toString().replace("-", "").take(10)
        var ownIdentity: String? = null
        val peerFolder = File(application.noBackupFilesDir, "onboard-peer-$suffix").apply { mkdirs() }
        val peerVault = AndroidSecretVault(application, "onboard-peer-vault-$suffix")
        val peer = RodaEngine.open(File(peerFolder, "peer.sqlite").absolutePath, "en")
        var scenario: ActivityScenario<MainActivity>? = null
        var journeyFailure: Throwable? = null
        val automation = InstrumentationRegistry.getInstrumentation().uiAutomation
        fun shell(command: String) { automation.executeShellCommand(command).use { android.os.ParcelFileDescriptor.AutoCloseInputStream(it).readBytes() } }
        try {
            runBlocking { repository.boot(); if (repository.state.value.demo) repository.leaveDemo() }
            assumeTrue("Never modify a pre-existing real account", repository.state.value.account == null)
            shell("input keyevent KEYCODE_WAKEUP"); shell("wm dismiss-keyguard")
            if (Build.VERSION.SDK_INT >= 33) shell("pm grant ${application.packageName} android.permission.POST_NOTIFICATIONS")
            repository.preferences.edit().putBoolean("onboarded", false).commit()
            scenario = ActivityScenario.launch(Intent(application, MainActivity::class.java))
            scenario.onActivity { it.window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON) }
            fun step(name: String) { compose.waitUntil(20_000) { compose.onAllNodesWithTag("onboarding:$name").fetchSemanticsNodes().isNotEmpty() } }
            fun next() { compose.onNodeWithTag("onboarding-next").performClick() }
            step("hello"); next(); step("profile")
            compose.onNodeWithTag("onboarding-name").performTextReplacement("Native Onboarding")
            compose.onNodeWithTag("onboarding-handle").performTextReplacement("on_$suffix")
            compose.onNodeWithText(application.getString(R.string.connection_settings)).performScrollTo().performClick()
            compose.onNodeWithTag("onboarding-relay").performScrollTo().performTextReplacement(relay!!)
            shell("input keyevent KEYCODE_BACK")
            next(); step("areas")
            ownIdentity = repository.state.value.me!!.id
            val areaResources = listOf(R.string.life_travel, R.string.life_money, R.string.life_home, R.string.life_food, R.string.life_friends, R.string.life_work, R.string.life_health, R.string.life_family)
            areaResources.forEach { compose.onNodeWithText(application.getString(it)).performScrollTo().performClick() }
            next(); step("plan")
            compose.waitUntil(20_000) { !compose.onNodeWithTag("onboarding-next").fetchSemanticsNode().config.contains(androidx.compose.ui.semantics.SemanticsProperties.Disabled) }
            areaResources.forEach { compose.onNodeWithText(application.getString(it)).assertExists() }
            scenario.recreate(); step("plan")
            areaResources.forEach { compose.onNodeWithText(application.getString(it)).assertExists() }
            next(); step("agents"); next(); step("notifications")
            compose.onNodeWithText(application.getString(R.string.not_now)).performClick()
            step("location"); compose.onNodeWithText(application.getString(R.string.not_now)).performClick()
            step("done"); next()
            compose.waitUntil(20_000) { repository.preferences.getBoolean("onboarded", false) && compose.onAllNodesWithTag("composer").fetchSemanticsNodes().isNotEmpty() }
            val made = repository.state.value.items.first { it.plan?.sections?.size == 8 }
            assertEquals(areaResources.map(application::getString), made.plan!!.sections.map { it.title })
            compose.waitUntil(30_000) { repository.state.value.account?.registered == true && repository.state.value.connection.state == "online" }
            compose.onNodeWithContentDescription(application.getString(R.string.back)).performClick()
            compose.onNodeWithTag("open-context").performClick()
            compose.onNodeWithTag("background-connection").performScrollTo().performClick()
            compose.onNodeWithTag("background-connection").assertIsOn()
            val notifications = application.getSystemService(NotificationManager::class.java)
            compose.waitUntil(10_000) { notifications.activeNotifications.any { it.id == 1001 && it.notification.flags and Notification.FLAG_ONGOING_EVENT != 0 } }
            val peerAccount = peer.createAccount("Native Peer", "op_$suffix", relay, peerVault)
            peer.startSync(null)
            compose.waitUntil(30_000) { peer.account()?.registered == true && peer.connection().synced }
            val discovered = runBlocking {
                repository.network { it.findPeople(peerAccount.handle).single { person -> person.id == peerAccount.identityId } }
            }
            val direct = runBlocking { repository.network { it.startDirect(discovered.id) } }
            compose.waitUntil(30_000) { peer.spaces().any { it.id == direct } && peer.groupKeys(direct) != null }
            var activityIdentity = 0
            var task = 0
            scenario.onActivity { activityIdentity = System.identityHashCode(it); task = it.taskId }
            scenario.moveToState(Lifecycle.State.CREATED)
            assertFalse(repository.appVisible)
            peer.sendMessage(direct, "Encrypted message while the Android app is in the background")
            compose.waitUntil(30_000) { notifications.activeNotifications.any { it.tag == "chat:$direct" } }
            val incoming = notifications.activeNotifications.first { it.tag == "chat:$direct" }.notification
            assertEquals(Notification.VISIBILITY_PRIVATE, incoming.visibility)
            incoming.contentIntent.send()
            compose.waitUntil(10_000) { repository.appVisible && repository.activeSpace == direct }
            assertEquals(Lifecycle.State.RESUMED, scenario.state)
            scenario.onActivity { assertEquals(activityIdentity, System.identityHashCode(it)); assertEquals(task, it.taskId) }
            compose.waitUntil(10_000) { notifications.activeNotifications.none { it.tag == "chat:$direct" } }
            compose.onNodeWithContentDescription(application.getString(R.string.back)).performClick()
            compose.onNodeWithTag("background-connection").performScrollTo().performClick()
            compose.waitUntil(10_000) { notifications.activeNotifications.none { it.id == 1001 } }
            assertFalse(repository.preferences.getBoolean(MessagingService.PREFERENCE, false))
            scenario.moveToState(Lifecycle.State.CREATED)
            compose.waitUntil(10_000) { repository.state.value.connection.state == "offline" }
            scenario.moveToState(Lifecycle.State.RESUMED)
            compose.waitUntil(30_000) { repository.state.value.connection.state == "online" }
            assertTrue(runBlocking { repository.query { it.verifyAll().all { log -> log.valid } } })
        } catch (failure: Throwable) {
            journeyFailure = failure
            throw failure
        } finally {
            try {
                val current = repository.state.value.account?.identityId
                val ownsCurrent = current != null && (current == ownIdentity || repository.state.value.account?.handle == "on_$suffix")
                if (ownsCurrent) {
                    MessagingService.stop(application)
                    repository.preferences.edit().putBoolean(MessagingService.PREFERENCE, false).commit()
                }
                peer.stopSync(); peer.eraseDevice(peerVault); peer.destroy(); peerFolder.deleteRecursively()
                File(application.noBackupFilesDir, "onboard-peer-vault-$suffix").deleteRecursively()
                if (ownsCurrent) runBlocking { repository.signOut() }
                try { scenario?.close() }
                finally {
                    repository.preferences.edit().putBoolean("demo", wasDemo).putBoolean("onboarded", wasOnboarded).putBoolean(MessagingService.PREFERENCE, wasBackground).commit()
                    if (wasDemo && !repository.state.value.demo) runBlocking { repository.useDemo() }
                }
            } catch (cleanupFailure: Throwable) {
                val original = journeyFailure
                if (original == null) throw cleanupFailure
                original.addSuppressed(cleanupFailure)
            }
        }
    }
}
