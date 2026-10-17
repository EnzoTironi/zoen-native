package xyz.tironi.zoen.ui

import androidx.compose.foundation.layout.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import java.time.Instant
import java.time.ZoneId
import java.time.ZoneOffset
import java.util.Date
import java.text.DateFormat

internal fun livingDateSelection(previous: Long, dateUtc: Long, zone: ZoneId): Long = Instant.ofEpochMilli(dateUtc).atZone(ZoneOffset.UTC).toLocalDate().atTime(Instant.ofEpochMilli(previous).atZone(zone).toLocalTime()).atZone(zone).toInstant().toEpochMilli()

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun LivingDateTimeField(label: String, value: Long, changed: (Long) -> Unit) {
    var showDate by remember { mutableStateOf(false) }
    var showTime by remember { mutableStateOf(false) }
    val zone = ZoneId.systemDefault()
    val old = Instant.ofEpochMilli(value).atZone(zone)
    Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
        Text(label, style = MaterialTheme.typography.bodyLarge)
        Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
            TextButton({ showDate = true }) { Text(DateFormat.getDateInstance().format(Date(value))) }
            TextButton({ showTime = true }) { Text(DateFormat.getTimeInstance(DateFormat.SHORT).format(Date(value))) }
        }
    }
    if (showDate) {
        val picker = rememberDatePickerState(initialSelectedDateMillis = old.toLocalDate().atStartOfDay(ZoneOffset.UTC).toInstant().toEpochMilli())
        DatePickerDialog(onDismissRequest = { showDate = false }, confirmButton = { TextButton({ picker.selectedDateMillis?.let { changed(livingDateSelection(value, it, zone)) }; showDate = false }) { Text("Done") } }, dismissButton = { TextButton({ showDate = false }) { Text("Cancel") } }) { DatePicker(picker) }
    }
    if (showTime) {
        val picker = rememberTimePickerState(initialHour = old.hour, initialMinute = old.minute)
        AlertDialog(onDismissRequest = { showTime = false }, title = { Text(label) }, text = { TimePicker(picker) }, confirmButton = { TextButton({ changed(old.toLocalDate().atTime(picker.hour, picker.minute).atZone(zone).toInstant().toEpochMilli()); showTime = false }) { Text("Done") } }, dismissButton = { TextButton({ showTime = false }) { Text("Cancel") } })
    }
}
