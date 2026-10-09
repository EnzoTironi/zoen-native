package xyz.tironi.zoen.widgets

import android.app.KeyguardManager
import android.app.AlarmManager
import android.app.PendingIntent
import android.appwidget.AppWidgetManager
import android.appwidget.AppWidgetProvider
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.view.View
import android.widget.RemoteViews
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import xyz.tironi.zoen.MainActivity
import xyz.tironi.zoen.R
import xyz.tironi.zoen.ZoenApplication
import xyz.tironi.zoen.data.AppState
import xyz.tironi.zoen.miniapps.WidgetSnapshot

class ZoenWidgetProvider : AppWidgetProvider() {
    override fun onUpdate(context: Context, manager: AppWidgetManager, ids: IntArray) {
        val pending = goAsync()
        CoroutineScope(SupervisorJob() + Dispatchers.IO).launch {
            try {
                val repository = (context.applicationContext as ZoenApplication).repository
                repository.boot()
                withContext(Dispatchers.Main) { updateAll(context) }
            } finally { pending.finish() }
        }
    }

    override fun onReceive(context: Context, intent: Intent) {
        super.onReceive(context, intent)
        if (intent.action == PINNED) {
            val nonce = intent.data?.lastPathSegment ?: return
            val prefs = context.getSharedPreferences("zoen", Context.MODE_PRIVATE)
            val selection = prefs.getString("widget-pin:$nonce", null) ?: return
            prefs.edit().remove("widget-pin:$nonce").apply()
            val id = intent.getIntExtra(AppWidgetManager.EXTRA_APPWIDGET_ID, AppWidgetManager.INVALID_APPWIDGET_ID)
            val manager = AppWidgetManager.getInstance(context)
            if (manager.getAppWidgetInfo(id)?.provider != ComponentName(context, ZoenWidgetProvider::class.java)) return
            val pending = goAsync()
            CoroutineScope(SupervisorJob() + Dispatchers.IO).launch {
                try {
                    val repository = (context.applicationContext as ZoenApplication).repository
                    repository.boot()
                    val selected = org.json.JSONObject(selection)
                    val state = repository.state.value
                    if (selected.getLong("expires") >= System.currentTimeMillis() && selected.getString("owner") == state.me?.id && state.items.any { it.id == selected.getString("item") && WidgetSnapshot.from(it) != null }) {
                        prefs.edit().putString("widget:$id", selected.getString("item")).putString("widget-owner:$id", state.me?.id).commit()
                    }
                    withContext(Dispatchers.Main) { updateAll(context) }
                } catch (_: Exception) {
                    withContext(Dispatchers.Main) { updateAll(context) }
                } finally { pending.finish() }
            }
            return
        }
        if (intent.action == REFRESH || intent.action == Intent.ACTION_TIME_CHANGED || intent.action == Intent.ACTION_TIMEZONE_CHANGED) {
            val manager = AppWidgetManager.getInstance(context)
            onUpdate(context, manager, manager.getAppWidgetIds(ComponentName(context, ZoenWidgetProvider::class.java)))
            return
        }
        if (intent.action != ACTION) return
        val id = intent.getIntExtra(AppWidgetManager.EXTRA_APPWIDGET_ID, AppWidgetManager.INVALID_APPWIDGET_ID)
        val tool = intent.getStringExtra("tool") ?: return
        val pending = goAsync()
        CoroutineScope(SupervisorJob() + Dispatchers.IO).launch {
            try {
                val repository = (context.applicationContext as ZoenApplication).repository
                repository.boot()
                val state = repository.state.value
                val itemId = repository.preferences.getString("widget:$id", null)
                val item = state.items.firstOrNull { it.id == itemId }
                val owner = repository.preferences.getString("widget-owner:$id", null)
                val snapshot = item?.let(WidgetSnapshot::from)
                val locked = context.getSystemService(KeyguardManager::class.java).isKeyguardLocked
                if (owner == state.me?.id && !state.keyMissing && !locked && snapshot?.actions?.any { it.tool == tool } == true) {
                    repository.change {
                        check(it.me().id == owner && repository.preferences.getString("widget-owner:$id", null) == owner)
                        it.appCallTool(item.id, tool, "{}", false)
                    }
                    withContext(Dispatchers.Main) { updateAll(context) }
                }
            } catch (_: Exception) {
                val repository = (context.applicationContext as ZoenApplication).repository
                withContext(Dispatchers.Main) { updateAll(context) }
            } finally { pending.finish() }
        }
    }

    override fun onDeleted(context: Context, ids: IntArray) {
        val prefs = context.getSharedPreferences("zoen", Context.MODE_PRIVATE)
        val editor = prefs.edit()
        ids.forEach { editor.remove("widget:$it").remove("widget-owner:$it") }
        editor.apply()
    }

    override fun onDisabled(context: Context) { context.getSystemService(AlarmManager::class.java).cancel(refreshIntent(context)) }

    companion object {
        private const val ACTION = "xyz.tironi.zoen.WIDGET_TOOL"
        private const val REFRESH = "xyz.tironi.zoen.WIDGET_REFRESH"
        private const val PINNED = "xyz.tironi.zoen.WIDGET_PINNED"

        fun pin(context: Context, snapshot: WidgetSnapshot, owner: String, demo: Boolean) {
            val manager = AppWidgetManager.getInstance(context)
            if (!manager.isRequestPinAppWidgetSupported) return
            val nonce = java.util.UUID.randomUUID().toString()
            val prefs = context.getSharedPreferences("zoen", Context.MODE_PRIVATE)
            val now = System.currentTimeMillis()
            val editor = prefs.edit()
            prefs.all.filterKeys { it.startsWith("widget-pin:") }.forEach { (key, value) ->
                if (runCatching { org.json.JSONObject(value as String).getLong("expires") }.getOrDefault(0L) < now) editor.remove(key)
            }
            val selected = org.json.JSONObject().put("item", snapshot.id).put("owner", owner).put("expires", now + 5 * 60_000).toString()
            editor.putString("widget-pin:$nonce", selected).commit()
            val callback = PendingIntent.getBroadcast(context, nonce.hashCode(), Intent(context, ZoenWidgetProvider::class.java).setAction(PINNED).setData(Uri.parse("zoen-widget-pin://selection/$nonce")), PendingIntent.FLAG_ONE_SHOT or PendingIntent.FLAG_MUTABLE)
            val preview = Bundle().apply { putParcelable(AppWidgetManager.EXTRA_APPWIDGET_PREVIEW, views(context, 0, snapshot, false, demo)) }
            if (!manager.requestPinAppWidget(ComponentName(context, ZoenWidgetProvider::class.java), preview, callback)) prefs.edit().remove("widget-pin:$nonce").apply()
        }

        private fun refreshIntent(context: Context) = PendingIntent.getBroadcast(context, 0,
            Intent(context, ZoenWidgetProvider::class.java).setAction(REFRESH), PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE)

        fun updateAll(context: Context, forceLocked: Boolean? = null) {
            val application = context.applicationContext as ZoenApplication
            if (Looper.myLooper() != Looper.getMainLooper()) {
                Handler(Looper.getMainLooper()).post { renderAll(context, application.repository.state.value, forceLocked) }
            } else renderAll(context, application.repository.state.value, forceLocked)
        }

        private fun renderAll(context: Context, state: AppState, forceLocked: Boolean?) {
            val manager = AppWidgetManager.getInstance(context)
            val ids = manager.getAppWidgetIds(ComponentName(context, ZoenWidgetProvider::class.java))
            if (ids.isEmpty()) return
            val prefs = context.getSharedPreferences("zoen", Context.MODE_PRIVATE)
            val locked = forceLocked ?: context.getSystemService(KeyguardManager::class.java).isKeyguardLocked
            ids.forEach { id ->
                val item = state.items.firstOrNull { it.id == prefs.getString("widget:$id", null) }
                val owns = prefs.getString("widget-owner:$id", null) == state.me?.id
                val snapshot = item?.takeIf { owns && !state.keyMissing }?.let(WidgetSnapshot::from)
                manager.updateAppWidget(id, views(context, id, snapshot, locked, state.demo))
            }
            val timed = ids.any { id -> state.items.firstOrNull { it.id == prefs.getString("widget:$id", null) }?.let(WidgetSnapshot::from)?.targetMs?.let { it > System.currentTimeMillis() } == true }
            val alarms = context.getSystemService(AlarmManager::class.java)
            if (timed) alarms.set(AlarmManager.RTC, (System.currentTimeMillis() / 60_000 + 1) * 60_000, refreshIntent(context))
            else alarms.cancel(refreshIntent(context))
        }

        fun views(context: Context, widgetId: Int, snapshot: WidgetSnapshot?, locked: Boolean, demo: Boolean): RemoteViews {
            val views = RemoteViews(context.packageName, R.layout.zoen_widget)
            val hidden = snapshot == null || snapshot.sensitive && locked
            views.setTextViewText(R.id.widget_title, if (hidden) context.getString(R.string.app_name) else snapshot!!.title)
            views.setTextViewText(R.id.widget_eyebrow, if (hidden) context.getString(R.string.app_name) else if (demo) context.getString(R.string.demo) else snapshot?.eyebrow ?: context.getString(R.string.app_name))
            val photo = if (hidden) null else when (snapshot?.photo) {
                "hike-tomales" -> R.drawable.miniapp_hike_tomales
                "hike-steep" -> R.drawable.miniapp_hike_steep
                "hike-lands" -> R.drawable.miniapp_hike_lands
                else -> null
            }
            val art = if (hidden || photo != null) null else WidgetArt.bitmap(context, snapshot?.art)
            views.setViewVisibility(R.id.widget_photo, if (photo == null) View.GONE else View.VISIBLE)
            views.setViewVisibility(R.id.widget_art, if (art == null) View.GONE else View.VISIBLE)
            photo?.let { views.setImageViewResource(R.id.widget_photo, it) }
            art?.let { views.setImageViewBitmap(R.id.widget_art, it) }
            views.setTextViewText(R.id.widget_value, if (hidden) context.getString(R.string.widget_choose) else when (snapshot!!.template) {
                WidgetSnapshot.Template.COUNTDOWN, WidgetSnapshot.Template.PHOTO -> if (snapshot.targetMs != null) snapshot.remaining().let { context.getString(R.string.widget_countdown, it.days, it.hours, it.minutes) } else snapshot.value.orEmpty()
                WidgetSnapshot.Template.TICKET -> snapshot.codes.joinToString(" → ")
                else -> snapshot.value.orEmpty()
            })
            val detail = if (hidden) "" else when (snapshot!!.template) {
                WidgetSnapshot.Template.LIST -> snapshot.rows.joinToString("\n") { if (it.done) "✓ ${it.text}" else "○ ${it.text}" }
                WidgetSnapshot.Template.TICKET -> snapshot.places.zip(snapshot.times).joinToString("\n") { (place, time) -> "$place · $time" }
                else -> snapshot.detail.orEmpty()
            }
            views.setTextViewText(R.id.widget_detail, detail)
            views.setViewVisibility(R.id.widget_progress, if (!hidden && snapshot!!.bars.isNotEmpty()) View.VISIBLE else View.GONE)
            snapshot?.bars?.firstOrNull()?.let { views.setProgressBar(R.id.widget_progress, 1000, (it.value * 1000).toInt(), false) }
            views.setTextViewText(R.id.widget_bars, if (hidden) "" else snapshot!!.bars.joinToString(" · ") { "${it.label} ${(it.value * 100).toInt()}%" })
            val link = if (hidden) "zoen://widgets" else snapshot!!.deepLink
            val open = Intent(context, MainActivity::class.java).setAction(Intent.ACTION_VIEW).setData(Uri.parse(link))
                .addFlags(Intent.FLAG_ACTIVITY_CLEAR_TOP or Intent.FLAG_ACTIVITY_SINGLE_TOP)
            views.setOnClickPendingIntent(R.id.widget_root, PendingIntent.getActivity(context, widgetId, open, PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE))
            val actions = if (hidden || locked) emptyList() else snapshot!!.actions.take(2)
            listOf(R.id.widget_action_one, R.id.widget_action_two).forEachIndexed { index, view ->
                val action = actions.getOrNull(index)
                views.setViewVisibility(view, if (action == null) View.GONE else View.VISIBLE)
                action?.let {
                    views.setTextViewText(view, it.label)
                    val intent = Intent(context, ZoenWidgetProvider::class.java).setAction(ACTION)
                        .setData(Uri.parse("zoen-widget://$widgetId/${it.tool}"))
                        .putExtra(AppWidgetManager.EXTRA_APPWIDGET_ID, widgetId).putExtra("tool", it.tool)
                    views.setOnClickPendingIntent(view, PendingIntent.getBroadcast(context, widgetId * 10 + index, intent, PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE))
                }
            }
            return views
        }
    }
}
