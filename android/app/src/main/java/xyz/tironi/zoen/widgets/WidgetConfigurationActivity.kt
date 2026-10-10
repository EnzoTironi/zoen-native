package xyz.tironi.zoen.widgets

import android.appwidget.AppWidgetManager
import android.content.ComponentName
import android.content.Intent
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.*
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.lifecycleScope
import kotlinx.coroutines.launch
import xyz.tironi.zoen.R
import xyz.tironi.zoen.ZoenApplication
import xyz.tironi.zoen.miniapps.SnapshotCard
import xyz.tironi.zoen.miniapps.WidgetSnapshot
import xyz.tironi.zoen.theme.ZoenTheme
import xyz.tironi.zoen.ui.EmptyState
import xyz.tironi.zoen.ui.ScreenBar

class WidgetConfigurationActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setResult(RESULT_CANCELED)
        val id = intent.getIntExtra(AppWidgetManager.EXTRA_APPWIDGET_ID, AppWidgetManager.INVALID_APPWIDGET_ID)
        val manager = AppWidgetManager.getInstance(this)
        if (id == AppWidgetManager.INVALID_APPWIDGET_ID || manager.getAppWidgetInfo(id)?.provider != ComponentName(this, ZoenWidgetProvider::class.java)) { finish(); return }
        enableEdgeToEdge()
        val repository = (application as ZoenApplication).repository
        lifecycleScope.launch { repository.boot() }
        setContent { ZoenTheme {
            val state by repository.state.collectAsStateWithLifecycle()
            val snapshots = state.items.mapNotNull(WidgetSnapshot::from)
            Scaffold(topBar = { ScreenBar(stringResource(R.string.widget_choose), { finish() }) }) { padding ->
                if (!state.ready) Box(Modifier.fillMaxSize().padding(padding)) { CircularProgressIndicator() }
                else if (snapshots.isEmpty()) EmptyState(stringResource(R.string.widget_empty), stringResource(R.string.widget_empty_detail), Modifier.padding(padding))
                else LazyColumn(Modifier.padding(padding), contentPadding = PaddingValues(20.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                    items(snapshots, key = { it.id }) { snapshot -> Card(onClick = {
                        repository.preferences.edit().putString("widget:$id", snapshot.id).putString("widget-owner:$id", state.me?.id).commit()
                        ZoenWidgetProvider.updateAll(this@WidgetConfigurationActivity)
                        setResult(RESULT_OK, Intent().putExtra(AppWidgetManager.EXTRA_APPWIDGET_ID, id)); finish()
                    }) { SnapshotCard(snapshot, Modifier.fillMaxWidth().padding(12.dp)) } }
                }
            }
        } }
    }
}
