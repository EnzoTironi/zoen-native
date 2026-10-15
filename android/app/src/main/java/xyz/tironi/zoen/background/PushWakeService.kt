package xyz.tironi.zoen.background

import android.content.Context
import com.google.firebase.FirebaseApp
import com.google.firebase.FirebaseOptions
import com.google.firebase.messaging.FirebaseMessaging
import com.google.firebase.messaging.FirebaseMessagingService
import com.google.firebase.messaging.RemoteMessage
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeoutOrNull
import xyz.tironi.zoen.BuildConfig
import xyz.tironi.zoen.ZoenApplication
import xyz.tironi.zoen.data.ZoenRepository

/** Firebase settings from the build; all empty means push is off. */
data class PushConfig(val projectId: String, val appId: String, val apiKey: String, val senderId: String) {
    val complete get() = listOf(projectId, appId, apiKey, senderId).all { it.isNotBlank() }

    companion object {
        val build get() = PushConfig(BuildConfig.FIREBASE_PROJECT_ID, BuildConfig.FIREBASE_APP_ID,
            BuildConfig.FIREBASE_API_KEY, BuildConfig.FIREBASE_SENDER_ID)
    }
}

/** The relay's wake is exactly {"wake":"1"} with no notification and no content. */
object PushWake {
    fun isWake(data: Map<String, String>, hasNotification: Boolean) =
        !hasNotification && data.keys == setOf("wake") && data["wake"] == "1"
}

object Push {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)

    /** Starts Firebase when configured and sends this device's token to the relay. */
    fun start(context: Context, repository: ZoenRepository, config: PushConfig = PushConfig.build) {
        if (!config.complete) return
        if (FirebaseApp.getApps(context).isEmpty()) {
            FirebaseApp.initializeApp(context, FirebaseOptions.Builder().setProjectId(config.projectId)
                .setApplicationId(config.appId).setApiKey(config.apiKey).setGcmSenderId(config.senderId).build())
        }
        FirebaseMessaging.getInstance().token.addOnSuccessListener { register(repository, it) }
    }

    fun register(repository: ZoenRepository, token: String) {
        scope.launch {
            runCatching {
                repository.boot()
                withTimeoutOrNull(30_000) { repository.state.first { it.ready && it.account != null && !it.demo } } ?: return@launch
                repository.network { it.registerPush("fcm", token) }
            }
        }
    }
}

/**
 * Wakes the core for ~20 s: it syncs and decrypts on the device, and ActivityNotifications
 * shows what arrived (and skips muted chats), exactly as with the app open.
 */
class PushWakeService : FirebaseMessagingService() {
    private val repository get() = (application as ZoenApplication).repository

    override fun onNewToken(token: String) = Push.register(repository, token)

    override fun onMessageReceived(message: RemoteMessage) {
        if (!PushWake.isWake(message.data, message.notification != null)) return
        // The always-on connection already has the messages; don't take its slot.
        if (repository.preferences.getBoolean(MessagingService.PREFERENCE, false)) return
        val owner = Any()
        runBlocking {
            runCatching {
                repository.boot()
                repository.setBackgroundConnection(true, owner)
                withTimeoutOrNull(WAKE_MS) { repository.state.first { it.connection.state == "online" && it.connection.synced } }
                delay(1_500)
            }
            repository.setBackgroundConnection(false, owner)
        }
    }

    companion object { const val WAKE_MS = 18_000L }
}
