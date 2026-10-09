package xyz.tironi.zoen.background

import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.IBinder
import androidx.core.app.NotificationCompat
import androidx.core.app.ServiceCompat
import androidx.core.content.ContextCompat
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.launch
import xyz.tironi.zoen.MainActivity
import xyz.tironi.zoen.R
import xyz.tironi.zoen.ZoenApplication

class MessagingService : Service() {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    private val repository get() = (application as ZoenApplication).repository

    override fun onCreate() {
        super.onCreate()
        val manager = getSystemService(NotificationManager::class.java)
        manager.createNotificationChannel(NotificationChannel("connection", getString(R.string.background_connection), NotificationManager.IMPORTANCE_LOW))
        ServiceCompat.startForeground(this, 1001, notification(getString(R.string.connecting)),
            if (Build.VERSION.SDK_INT >= 34) ServiceInfo.FOREGROUND_SERVICE_TYPE_REMOTE_MESSAGING else 0)
        scope.launch {
            repository.boot()
            repository.setBackgroundConnection(repository.preferences.getBoolean(PREFERENCE, false), this@MessagingService)
            repository.state.collect { state ->
                if (!repository.preferences.getBoolean(PREFERENCE, false) || state.demo || state.ready && (state.account == null || state.keyMissing)) {
                    stopSelf()
                } else {
                    val text = when {
                        state.connection.pending > 0uL -> getString(R.string.background_pending, state.connection.pending.toString())
                        state.connection.state == "online" -> getString(R.string.background_connected)
                        else -> getString(R.string.background_reconnecting)
                    }
                    manager.notify(1001, notification(text))
                }
            }
        }
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        if (intent?.action == STOP) {
            repository.preferences.edit().putBoolean(PREFERENCE, false).apply()
            scope.launch { repository.setBackgroundConnection(false, this@MessagingService) }
            stopSelf()
            return START_NOT_STICKY
        }
        return START_STICKY
    }

    override fun onBind(intent: Intent?): IBinder? = null
    override fun onDestroy() {
        repository.releaseBackgroundConnection(this)
        scope.cancel(); super.onDestroy()
    }
    override fun onTimeout(startId: Int, fgsType: Int) { stopSelf() }

    private fun notification(text: String) = NotificationCompat.Builder(this, "connection")
        .setSmallIcon(R.drawable.ic_notification).setContentTitle(getString(R.string.app_name)).setContentText(text)
        .setCategory(NotificationCompat.CATEGORY_SERVICE).setOngoing(true).setOnlyAlertOnce(true)
        .setContentIntent(PendingIntent.getActivity(this, 0, Intent(this, MainActivity::class.java), PendingIntent.FLAG_IMMUTABLE))
        .addAction(0, getString(R.string.stop_background_connection), PendingIntent.getService(this, 1,
            Intent(this, MessagingService::class.java).setAction(STOP), PendingIntent.FLAG_IMMUTABLE))
        .build()

    companion object {
        const val PREFERENCE = "background-connection"
        private const val STOP = "xyz.tironi.zoen.STOP_CONNECTION"
        fun start(context: Context) { ContextCompat.startForegroundService(context, Intent(context, MessagingService::class.java)) }
        fun stop(context: Context) { context.stopService(Intent(context, MessagingService::class.java)) }
    }
}
