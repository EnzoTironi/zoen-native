package xyz.tironi.zoen.agent

import androidx.compose.foundation.layout.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import xyz.tironi.zoen.R
import xyz.tironi.zoen.ZoenViewModel

@Composable
fun OnDeviceAiSettings(model: ZoenViewModel) {
    val availability by model.planner.availability.collectAsStateWithLifecycle()
    LaunchedEffect(model.planner) { model.planner.refreshAvailability() }
    Card(colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surfaceContainerLow)) {
        Column(Modifier.fillMaxWidth().padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Text(stringResource(R.string.agent_ai_title), style = MaterialTheme.typography.titleMedium)
            Text(stringResource(when (availability.status) {
                ModelStatus.Ready -> R.string.agent_ai_ready
                ModelStatus.Checking -> R.string.agent_ai_checking
                ModelStatus.Downloadable -> R.string.agent_ai_downloadable
                ModelStatus.Downloading -> R.string.agent_ai_downloading
                ModelStatus.Unavailable -> R.string.agent_ai_unavailable
                ModelStatus.Failed -> R.string.agent_ai_failed
            }, availability.downloadedBytes / (1024 * 1024)), style = MaterialTheme.typography.bodyMedium)
            Text(stringResource(R.string.agent_ai_privacy), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
            if (availability.status in setOf(ModelStatus.Checking, ModelStatus.Downloading)) LinearProgressIndicator(Modifier.fillMaxWidth())
            else if (availability.status == ModelStatus.Downloadable) Button(onClick = { model.launch { model.planner.downloadModel() } }) { Text(stringResource(R.string.agent_ai_download)) }
            else if (availability.status in setOf(ModelStatus.Failed, ModelStatus.Unavailable)) TextButton(onClick = { model.launch { model.planner.refreshAvailability() } }) { Text(stringResource(R.string.agent_ai_retry)) }
        }
    }
}
