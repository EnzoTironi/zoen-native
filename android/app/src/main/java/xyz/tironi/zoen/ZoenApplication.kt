package xyz.tironi.zoen

import android.app.Application
import android.app.NotificationChannel
import android.app.NotificationManager
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import androidx.core.content.ContextCompat
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch
import xyz.tironi.zoen.background.ActivityNotifications
import xyz.tironi.zoen.data.ZoenRepository

class ZoenApplication : Application() {
    val repository by lazy { ZoenRepository(this) }
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    override fun onCreate() {
        super.onCreate()
        getSystemService(NotificationManager::class.java).createNotificationChannel(
            NotificationChannel("activity", getString(R.string.activity), NotificationManager.IMPORTANCE_DEFAULT)
        )
        val notifications = ActivityNotifications(this)
        ContextCompat.registerReceiver(this, object : BroadcastReceiver() {
            override fun onReceive(context: Context, intent: Intent) {
                xyz.tironi.zoen.widgets.ZoenWidgetProvider.updateAll(this@ZoenApplication, forceLocked =
                    if (intent.action == Intent.ACTION_SCREEN_OFF) true else null)
            }
        }, IntentFilter().apply { addAction(Intent.ACTION_SCREEN_OFF); addAction(Intent.ACTION_USER_PRESENT); addAction(Intent.ACTION_TIME_TICK) }, ContextCompat.RECEIVER_NOT_EXPORTED)
        scope.launch { repository.state.collect {
            notifications.update(it, repository.appVisible, repository.activeSpace)
            xyz.tironi.zoen.widgets.ZoenWidgetProvider.updateAll(this@ZoenApplication)
        } }
    }
}
