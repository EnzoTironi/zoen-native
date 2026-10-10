package xyz.tironi.zoen.miniapps

import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.delay
import xyz.tironi.zoen.R
import xyz.tironi.zoen.ui.PixelDonkey
import xyz.tironi.zoen.ui.appIcon
import xyz.tironi.zoen.ui.ZoenDoodle
import xyz.tironi.zoen.ui.ink.DoodleArt

/** All seven templates render natively; arbitrary HTML can only publish this validated DTO. */
@Composable
fun SnapshotCard(snapshot: WidgetSnapshot, modifier: Modifier = Modifier, live: Boolean = true) {
    val accent = Color(android.graphics.Color.parseColor(snapshot.accentHex))
    val darkArt = snapshot.template == WidgetSnapshot.Template.PHOTO || snapshot.art in setOf("pet.asleep", "globe", "trip")
    val ink = if (darkArt) Color.White else MaterialTheme.colorScheme.onSurface
    val background = if (darkArt) Color(0xFF18251D) else MaterialTheme.colorScheme.surfaceContainerLow
    var now by remember { mutableLongStateOf(System.currentTimeMillis()) }
    LaunchedEffect(snapshot.targetMs, live) { if (live && snapshot.targetMs != null) while (true) { now = System.currentTimeMillis(); delay(60_000) } }
    val spoken = listOfNotNull(snapshot.title, snapshot.value, snapshot.detail, snapshot.eyebrow).joinToString(". ")
    val artColors = when (snapshot.art) {
        "trip" -> listOf(Color(0xFFF9B67A), Color(0xFFE9846B), Color(0xFF3B6C8F))
        "hike" -> listOf(Color(0xFFDDEFD2), Color(0xFFB9DDB0))
        "pot" -> listOf(Color(0xFFFBE3C8), Color(0xFFF4B98A))
        "globe" -> listOf(Color.Black, Color.Black)
        else -> listOf(accent.copy(alpha = if (darkArt) .3f else .17f), background)
    }
    Box(modifier.clip(RoundedCornerShape(24.dp)).background(Brush.verticalGradient(artColors))
        .semantics(mergeDescendants = true) { contentDescription = spoken }) {
        if (snapshot.template == WidgetSnapshot.Template.PHOTO) {
            Image(painterResource(when (snapshot.photo) {
                "hike-steep" -> R.drawable.miniapp_hike_steep
                "hike-lands" -> R.drawable.miniapp_hike_lands
                else -> R.drawable.miniapp_hike_tomales
            }), null, Modifier.fillMaxSize(), contentScale = ContentScale.Crop)
            Box(Modifier.fillMaxSize().background(Brush.verticalGradient(listOf(Color.Transparent, Color.Black.copy(alpha = .7f)))))
        }
        Column(Modifier.fillMaxSize().padding(14.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
            snapshot.eyebrow?.let { Text(it, style = MaterialTheme.typography.labelSmall, color = ink.copy(alpha = .8f), maxLines = 1, overflow = TextOverflow.Ellipsis) }
            when (snapshot.template) {
                WidgetSnapshot.Template.TICKET -> {
                    Text(snapshot.title, style = MaterialTheme.typography.labelMedium, color = ink)
                    Spacer(Modifier.weight(1f))
                    Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) {
                        snapshot.codes.take(2).forEach { Text(it, style = MaterialTheme.typography.headlineMedium, fontWeight = FontWeight.Bold, color = ink) }
                    }
                    Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) {
                        snapshot.places.take(2).forEach { Text(it, style = MaterialTheme.typography.labelSmall, color = ink.copy(alpha = .7f), maxLines = 1) }
                    }
                    Row(verticalAlignment = Alignment.CenterVertically) { HorizontalDivider(Modifier.weight(1f)); Icon(Icons.Rounded.DirectionsCar, null, Modifier.padding(horizontal = 6.dp).size(18.dp), tint = accent); HorizontalDivider(Modifier.weight(1f)) }
                    Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) {
                        snapshot.times.take(2).forEach { Text(it, style = MaterialTheme.typography.titleSmall, color = ink) }
                    }
                    Spacer(Modifier.weight(1f))
                }
                WidgetSnapshot.Template.PHOTO -> {
                    Spacer(Modifier.weight(1f))
                    Text(snapshot.title, color = ink, style = MaterialTheme.typography.titleMedium, maxLines = 2, fontWeight = FontWeight.Bold, overflow = TextOverflow.Ellipsis)
                    if (snapshot.targetMs != null) SnapshotCountdown(snapshot, now, ink)
                }
                WidgetSnapshot.Template.COUNTDOWN -> {
                    SnapshotArt(snapshot, accent, Modifier.weight(1f).fillMaxWidth(), live)
                    Text(snapshot.title, style = MaterialTheme.typography.titleSmall, color = ink, maxLines = 1, overflow = TextOverflow.Ellipsis)
                    SnapshotCountdown(snapshot, now, ink)
                }
                WidgetSnapshot.Template.PROGRESS -> {
                    SnapshotArt(snapshot, accent, Modifier.weight(1f).fillMaxWidth(), live)
                    Text(snapshot.title, style = MaterialTheme.typography.titleSmall, color = ink, maxLines = 1, overflow = TextOverflow.Ellipsis)
                    snapshot.bars.take(3).forEach { bar ->
                        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                            Text(bar.label, Modifier.widthIn(max = 60.dp), color = ink, style = MaterialTheme.typography.labelSmall, maxLines = 1)
                            LinearProgressIndicator(progress = { bar.value.toFloat() }, Modifier.weight(1f), color = accent, trackColor = ink.copy(alpha = .1f))
                        }
                    }
                }
                WidgetSnapshot.Template.LIST -> {
                    Icon(appIcon(snapshot.appId), null, Modifier.size(26.dp), tint = accent)
                    Text(snapshot.title, style = MaterialTheme.typography.titleSmall, color = ink, maxLines = 1, overflow = TextOverflow.Ellipsis)
                    snapshot.rows.take(4).forEach { row -> Row(horizontalArrangement = Arrangement.spacedBy(5.dp), verticalAlignment = Alignment.CenterVertically) {
                        Icon(if (row.done) Icons.Rounded.CheckBox else Icons.Rounded.CheckBoxOutlineBlank, null, Modifier.size(16.dp), tint = accent)
                        Text(row.text, style = MaterialTheme.typography.bodySmall, color = ink.copy(alpha = if (row.done) .5f else 1f), maxLines = 1, overflow = TextOverflow.Ellipsis)
                    } }
                }
                WidgetSnapshot.Template.STAT -> {
                    SnapshotArt(snapshot, accent, Modifier.weight(1f).fillMaxWidth(), live)
                    Row(verticalAlignment = Alignment.Bottom, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        snapshot.value?.let { Text(it, style = MaterialTheme.typography.headlineMedium, color = ink, fontWeight = FontWeight.Bold, maxLines = 1) }
                        Text(snapshot.title, style = MaterialTheme.typography.titleSmall, color = ink, maxLines = 2, overflow = TextOverflow.Ellipsis)
                    }
                    snapshot.detail?.let { Text(it, style = MaterialTheme.typography.bodySmall, color = ink.copy(alpha = .75f), maxLines = 1, overflow = TextOverflow.Ellipsis) }
                }
                WidgetSnapshot.Template.CAPTION -> {
                    SnapshotArt(snapshot, accent, Modifier.weight(1f).fillMaxWidth(), live)
                    Text(snapshot.title, style = MaterialTheme.typography.titleMedium, color = ink, maxLines = 2, overflow = TextOverflow.Ellipsis)
                    snapshot.detail?.let { Text(it, style = MaterialTheme.typography.bodySmall, color = ink.copy(alpha = .75f), maxLines = 2, overflow = TextOverflow.Ellipsis) }
                }
            }
        }
    }
}

@Composable
private fun SnapshotCountdown(snapshot: WidgetSnapshot, now: Long, ink: Color) {
    val remaining = snapshot.remaining(now)
    Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) {
        listOf(remaining.days to R.string.miniapp_days, remaining.hours to R.string.miniapp_hours, remaining.minutes to R.string.miniapp_minutes).forEach { (value, label) ->
            Column(horizontalAlignment = Alignment.CenterHorizontally) {
                Text(value.toString(), style = MaterialTheme.typography.titleLarge, color = ink, fontWeight = FontWeight.Bold)
                Text(stringResource(label), style = MaterialTheme.typography.labelSmall, color = ink.copy(alpha = .8f))
            }
        }
    }
}

@Composable
private fun SnapshotArt(snapshot: WidgetSnapshot, accent: Color, modifier: Modifier, live: Boolean) {
    when (snapshot.art) {
        "pet", "pet.asleep", "pet.gone" -> PixelDonkey(modifier, snapshot.art == "pet.asleep", snapshot.art == "pet.gone", live)
        "globe" -> NativeGlobe(modifier = modifier, interactive = false, spin = live)
        "pot", "ballot", "notepad", "trip", "hike" -> ZoenDoodle(
            DoodleArt.entries.single { it.name.equals(snapshot.art, true) }, modifier, live)
        else -> Box(modifier, contentAlignment = Alignment.Center) { Icon(appIcon(snapshot.appId), null, Modifier.size(52.dp), tint = accent) }
    }
}
