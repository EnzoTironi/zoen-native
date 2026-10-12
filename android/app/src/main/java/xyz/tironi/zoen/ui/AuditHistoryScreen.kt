package xyz.tironi.zoen.ui

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.GppBad
import androidx.compose.material.icons.rounded.Refresh
import androidx.compose.material.icons.rounded.VerifiedUser
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp
import java.text.DateFormat
import java.util.Date
import kotlinx.coroutines.CancellationException
import xyz.tironi.zoen.R
import xyz.tironi.zoen.ZoenViewModel
import xyz.tironi.zoen.core.LogEventDto
import xyz.tironi.zoen.core.LogReport
import xyz.tironi.zoen.data.AppState

private data class AuditHistoryData(
    val reports: List<LogReport> = emptyList(),
    val events: List<LogEventDto> = emptyList(),
    val loading: Boolean = true,
    val error: String? = null,
)

@Composable
fun AuditHistoryScreen(model: ZoenViewModel, state: AppState, back: () -> Unit) {
    var selected by rememberSaveable(state.me?.id) { mutableStateOf<String?>(null) }
    var retry by remember { mutableIntStateOf(0) }
    val selectedId = selected
    val loadFailure = stringResource(R.string.ap_audit_load_failed)
    val loaded by produceState(AuditHistoryData(), state.me?.id, state.revision, selectedId, retry) {
        value = AuditHistoryData()
        try {
            value = model.repository.query { core ->
                AuditHistoryData(core.verifyAll(), selectedId?.let(core::logEvents) ?: emptyList(), loading = false)
            }
        } catch (error: Exception) {
            if (error is CancellationException) throw error
            value = AuditHistoryData(loading = false, error = error.message ?: loadFailure)
        }
    }
    val snapshot = loaded
    val reports = snapshot.reports
    val events = snapshot.events
    val report = reports.firstOrNull { it.spaceId == selectedId }
    val locale = LocalConfiguration.current.locales[0]
    val time = remember(locale) { DateFormat.getDateTimeInstance(DateFormat.MEDIUM, DateFormat.MEDIUM, locale) }
    val goBack = { if (selectedId == null) back() else selected = null }
    BackHandler(enabled = selectedId != null, onBack = goBack)
    Scaffold(topBar = {
        ScreenBar(report?.spaceTitle ?: stringResource(R.string.signed_log), goBack) {
            IconButton(onClick = { retry++ }, enabled = !snapshot.loading) {
                Icon(Icons.Rounded.Refresh, stringResource(R.string.ap_audit_refresh))
            }
        }
    }) { padding ->
        LazyColumn(
            Modifier.padding(padding).fillMaxSize(),
            contentPadding = PaddingValues(24.dp),
            verticalArrangement = Arrangement.spacedBy(14.dp),
        ) {
            if (snapshot.loading) item { LinearProgressIndicator(Modifier.fillMaxWidth()) }
            if (snapshot.error != null) item {
                Card {
                    Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                        Text(stringResource(R.string.ap_audit_load_failed), color = MaterialTheme.colorScheme.error)
                        Text(snapshot.error, style = MaterialTheme.typography.bodySmall)
                        TextButton(onClick = { retry++ }) { Text(stringResource(R.string.ap_retry)) }
                    }
                }
            }
            if (selectedId == null) {
                if (!snapshot.loading && snapshot.error == null) {
                    item {
                        if (reports.isEmpty()) EmptyState(stringResource(R.string.ap_audit_empty), stringResource(R.string.ap_audit_explanation))
                        else AuditVerification(reports.all { it.valid }, stringResource(R.string.ap_audit_checked_chats, reports.size))
                    }
                    items(reports, key = { it.spaceId }) { entry ->
                        Card(onClick = { selected = entry.spaceId }) { AuditReport(entry) }
                    }
                }
                item { Text(stringResource(R.string.ap_audit_explanation), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant) }
            } else if (!snapshot.loading && snapshot.error == null) {
                if (report != null) item { Card { AuditReport(report) } }
                item { SectionLabel(stringResource(R.string.ap_audit_all_events, events.size)) }
                if (events.isEmpty()) item { Text(stringResource(R.string.ap_audit_no_events)) }
                itemsIndexed(events, key = { index, event -> "$index:${event.seq}:${event.hash}" }) { _, event ->
                    Card { AuditEvent(event, time.format(Date(event.atMs))) }
                }
            }
        }
    }
}

@Composable
private fun AuditVerification(valid: Boolean, detail: String) {
    val color = if (valid) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.error
    Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
        Icon(if (valid) Icons.Rounded.VerifiedUser else Icons.Rounded.GppBad, null, Modifier.size(36.dp), tint = color)
        Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
            Text(stringResource(if (valid) R.string.ap_audit_verified else R.string.ap_audit_invalid), style = MaterialTheme.typography.titleLarge, color = color)
            Text(detail, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
    }
}

@Composable
private fun AuditReport(report: LogReport) {
    Column(Modifier.fillMaxWidth().padding(16.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
        Text(report.spaceTitle, style = MaterialTheme.typography.titleMedium)
        AuditVerification(report.valid, stringResource(R.string.ap_audit_event_count, report.events.toString()))
        report.error?.let { Text(it, color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodySmall) }
        AuditValue(stringResource(R.string.ap_audit_head_hash), report.headHash)
    }
}

@Composable
private fun AuditEvent(event: LogEventDto, at: String) {
    Column(Modifier.fillMaxWidth().padding(16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        Text(stringResource(R.string.ap_audit_sequence, event.seq.toString(), event.label), style = MaterialTheme.typography.titleMedium)
        Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
            Avatar(event.author, size = 36)
            Column {
                Text(event.author.name, style = MaterialTheme.typography.bodyMedium)
                Text(at, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
            }
        }
        AuditValue(stringResource(R.string.ap_audit_hash), event.hash)
        AuditValue(stringResource(R.string.ap_audit_previous), event.prev)
        AuditValue(stringResource(R.string.ap_audit_signature), event.signature)
    }
}

@Composable
private fun AuditValue(label: String, value: String) {
    Column(verticalArrangement = Arrangement.spacedBy(3.dp)) {
        Text(label, style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
        SelectionContainer { Text(value, fontFamily = FontFamily.Monospace, style = MaterialTheme.typography.bodySmall) }
    }
}
