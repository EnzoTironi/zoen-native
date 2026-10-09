package xyz.tironi.zoen

import android.app.Application
import android.app.NotificationChannel
import android.app.NotificationManager
import xyz.tironi.zoen.data.ZoenRepository

class ZoenApplication : Application() {
    val repository by lazy { ZoenRepository(this) }
    override fun onCreate() {
        super.onCreate()
        getSystemService(NotificationManager::class.java).createNotificationChannel(
            NotificationChannel("activity", getString(R.string.activity), NotificationManager.IMPORTANCE_DEFAULT)
        )
    }
}
