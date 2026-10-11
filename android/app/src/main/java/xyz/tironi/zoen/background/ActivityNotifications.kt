package xyz.tironi.zoen.background

import android.Manifest
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import androidx.core.app.NotificationCompat
import androidx.core.content.ContextCompat
import xyz.tironi.zoen.MainActivity
import xyz.tironi.zoen.R
import xyz.tironi.zoen.core.RequestStatus
import xyz.tironi.zoen.data.AppState

class ActivityNotifications(private val context: Context) {
    private var previous: AppState? = null
    private val manager = context.getSystemService(NotificationManager::class.java)

    fun update(state: AppState, visible: Boolean, activeSpace: String?) {
        if (!state.ready || state.me == null) { if (previous != null) clear(); previous = null; return }
        val old = previous
        previous = state
        if (old == null) return
        if (old.me?.id != state.me.id || old.demo != state.demo) { clear(); return }
        (old.spaces.map { it.id }.toSet() - state.spaces.map { it.id }.toSet()).forEach { manager.cancel("chat:$it", 1) }
        (old.requests.map { it.id }.toSet() - state.requests.map { it.id }.toSet()).forEach { manager.cancel("request:$it", 1) }
        if (!context.getSharedPreferences("zoen", Context.MODE_PRIVATE).getBoolean("notifications", false)) { clear(); return }
        if (Build.VERSION.SDK_INT >= 33 && ContextCompat.checkSelfPermission(context, Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED) return
        if (!manager.areNotificationsEnabled()) return
        val oldSpaces = old.spaces.associateBy { it.id }
        state.spaces.forEach { space ->
            if (space.unread == 0u) { manager.cancel("chat:${space.id}", 1); return@forEach }
            val former = oldSpaces[space.id]
            if (former != null && space.eventCount <= former.eventCount || space.lastAuthor?.isMe != false) return@forEach
            if (context.getSharedPreferences("zoen", Context.MODE_PRIVATE).getBoolean("muted.person:${state.me.id}:${space.lastAuthor?.id}", false)) return@forEach
            if (visible && activeSpace == space.id) return@forEach
            val preview = if (xyz.tironi.zoen.media.VoiceNoteRef.parse(space.lastPreview) != null) context.getString(R.string.media_voice_message) else space.lastPreview
            notify("chat:${space.id}", space.title, preview, "zoen://chat/${space.id}", NotificationCompat.CATEGORY_MESSAGE)
        }
        val before = old.requests.associateBy { it.id }
        state.requests.forEach { request ->
            if (request.status != RequestStatus.PENDING || !request.agent.isMine) { manager.cancel("request:${request.id}", 1); return@forEach }
            if (before[request.id]?.status == RequestStatus.PENDING) return@forEach
            notify("request:${request.id}", context.getString(R.string.approval_needed), request.title,
                "zoen://request/${request.id}", NotificationCompat.CATEGORY_REMINDER)
        }
    }

    private fun clear() {
        manager.activeNotifications.filter { it.tag?.startsWith("chat:") == true || it.tag?.startsWith("request:") == true }.forEach { manager.cancel(it.tag, it.id) }
    }

    private fun notify(tag: String, title: String, text: String, link: String, category: String) {
        val intent = Intent(context, MainActivity::class.java).setAction(Intent.ACTION_VIEW)
            .setData(Uri.parse(link)).addFlags(Intent.FLAG_ACTIVITY_CLEAR_TOP or Intent.FLAG_ACTIVITY_SINGLE_TOP)
        val content = PendingIntent.getActivity(context, tag.hashCode(), intent, PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE)
        val public = NotificationCompat.Builder(context, "activity").setSmallIcon(R.drawable.ic_notification)
            .setContentTitle(context.getString(R.string.app_name)).setContentText(context.getString(R.string.new_activity)).build()
        manager.notify(tag, 1, NotificationCompat.Builder(context, "activity")
            .setSmallIcon(R.drawable.ic_notification).setContentTitle(title.take(120)).setContentText(text.take(300))
            .setStyle(NotificationCompat.BigTextStyle().bigText(text.take(1000)))
            .setContentIntent(content).setAutoCancel(true).setVisibility(NotificationCompat.VISIBILITY_PRIVATE)
            .setPublicVersion(public).setCategory(category).setGroup("zoen.activity").build())
    }
}
