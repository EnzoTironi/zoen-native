package xyz.tironi.zoen.background

import android.Manifest
import android.os.Build
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import xyz.tironi.zoen.R
import xyz.tironi.zoen.ZoenViewModel

@Composable
fun BackgroundConnectionSettings(model: ZoenViewModel) {
    val context = LocalContext.current
    val enabled = booleanPreference(model, MessagingService.PREFERENCE)
    val set: (Boolean) -> Unit = { value ->
        model.repository.preferences.edit().putBoolean(MessagingService.PREFERENCE, value).apply()
        if (value) model.repository.preferences.edit().putBoolean("notifications", true).apply()
        if (value) MessagingService.start(context) else MessagingService.stop(context)
        model.launch { model.repository.refresh() }
    }
    val denied = stringResource(R.string.notifications_permission_needed)
    val permission = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { allowed ->
        if (allowed) set(true) else model.notify(denied)
    }
    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
        Column(Modifier.weight(1f)) {
            Text(stringResource(R.string.background_connection), style = MaterialTheme.typography.titleMedium)
            Text(stringResource(R.string.background_connection_detail), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
        Switch(enabled, modifier = Modifier.testTag("background-connection"), onCheckedChange = { value ->
            if (value && Build.VERSION.SDK_INT >= 33) permission.launch(Manifest.permission.POST_NOTIFICATIONS) else set(value)
        })
    }
}

@Composable
private fun booleanPreference(model: ZoenViewModel, key: String): Boolean {
    val preferences = model.repository.preferences
    var value by remember(preferences, key) { mutableStateOf(preferences.getBoolean(key, false)) }
    DisposableEffect(preferences, key) {
        val listener = android.content.SharedPreferences.OnSharedPreferenceChangeListener { _, changed ->
            if (changed == null || changed == key) value = preferences.getBoolean(key, false)
        }
        preferences.registerOnSharedPreferenceChangeListener(listener)
        onDispose { preferences.unregisterOnSharedPreferenceChangeListener(listener) }
    }
    return value
}

@Composable
fun NotificationSettings(model: ZoenViewModel) {
    val enabled = booleanPreference(model, "notifications")
    val set: (Boolean) -> Unit = { value ->
        model.repository.preferences.edit().putBoolean("notifications", value).apply()
        model.launch { model.repository.refresh() }
    }
    val permission = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { set(it) }
    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
        Column(Modifier.weight(1f)) {
            Text(stringResource(R.string.enable_notifications), style = MaterialTheme.typography.titleMedium)
            Text(stringResource(R.string.notifications_detail), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
        Switch(enabled, modifier = Modifier.testTag("activity-notifications"), onCheckedChange = { value ->
            if (value && Build.VERSION.SDK_INT >= 33) permission.launch(Manifest.permission.POST_NOTIFICATIONS) else set(value)
        })
    }
}
