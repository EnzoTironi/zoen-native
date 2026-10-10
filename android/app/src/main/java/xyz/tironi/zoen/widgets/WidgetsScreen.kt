package xyz.tironi.zoen.widgets

import android.appwidget.AppWidgetManager
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.*
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import xyz.tironi.zoen.R
import xyz.tironi.zoen.data.AppState
import xyz.tironi.zoen.miniapps.SnapshotCard
import xyz.tironi.zoen.miniapps.WidgetSnapshot
import xyz.tironi.zoen.ui.ScreenBar

@Composable
fun WidgetsScreen(state: AppState, back: () -> Unit) {
    val context = LocalContext.current
    val manager = AppWidgetManager.getInstance(context)
    Scaffold(topBar = { ScreenBar(stringResource(R.string.widgets), back) }) { padding -> LazyColumn(Modifier.padding(padding), contentPadding = PaddingValues(20.dp), verticalArrangement = Arrangement.spacedBy(14.dp)) {
        item { Text(stringResource(R.string.widget_instructions), style = MaterialTheme.typography.bodyLarge) }
        items(state.items.mapNotNull(WidgetSnapshot::from), key = { it.id }) { snapshot ->
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                SnapshotCard(snapshot, Modifier.fillMaxWidth())
                if (manager.isRequestPinAppWidgetSupported) Button(enabled = state.me != null && !state.keyMissing, modifier = Modifier.testTag("widget-pin:${snapshot.id}"), onClick = { ZoenWidgetProvider.pin(context, snapshot, checkNotNull(state.me).id, state.demo) }) { Text(stringResource(R.string.widget_add)) }
            }
        }
    } }
}
