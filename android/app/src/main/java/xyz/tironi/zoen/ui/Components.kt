package xyz.tironi.zoen.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.Image
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.rounded.ArrowBack
import androidx.compose.material.icons.rounded.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import xyz.tironi.zoen.R
import xyz.tironi.zoen.ZoenApplication
import xyz.tironi.zoen.agent.AvatarArt
import xyz.tironi.zoen.agent.AvatarDrawing
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import xyz.tironi.zoen.core.*

@Composable
fun Avatar(persona: Persona?, modifier: Modifier = Modifier, size: Int = 52, onClick: (() -> Unit)? = null, contact: Boolean = false) {
    val context = LocalContext.current
    val repository = (context.applicationContext as? ZoenApplication)?.repository
    val revision = repository?.state?.collectAsStateWithLifecycle()?.value?.revision
    val photo by produceState<android.graphics.Bitmap?>(null, persona?.id, revision) {
        value = if (persona?.kind == PersonaKind.PERSON && repository != null) runCatching {
            val bytes = repository.query { core -> core.getProfile(persona.id).let { profile ->
                if (profile.photoReady) profile.photoSha256?.let(core::media) else null
            } }
            bytes?.let { withContext(Dispatchers.Default) {
                android.graphics.ImageDecoder.decodeBitmap(android.graphics.ImageDecoder.createSource(java.nio.ByteBuffer.wrap(it))) { decoder, info, _ ->
                    decoder.allocator = android.graphics.ImageDecoder.ALLOCATOR_SOFTWARE
                    val scale = 512f / maxOf(info.size.width, info.size.height).coerceAtLeast(512)
                    decoder.setTargetSize((info.size.width * scale).toInt().coerceAtLeast(1), (info.size.height * scale).toInt().coerceAtLeast(1))
                }
            } }
        }.getOrElse { if (it is kotlinx.coroutines.CancellationException) throw it; null } else null
    }
    val preferences = remember(context) { context.getSharedPreferences("zoen", android.content.Context.MODE_PRIVATE) }
    var drawingVersion by remember { mutableIntStateOf(0) }
    DisposableEffect(preferences) {
        val listener = android.content.SharedPreferences.OnSharedPreferenceChangeListener { _, key -> if (key?.startsWith("avatar.") == true) drawingVersion++ }
        preferences.registerOnSharedPreferenceChangeListener(listener)
        onDispose { preferences.unregisterOnSharedPreferenceChangeListener(listener) }
    }
    val drawing = remember(persona?.id, drawingVersion) { persona?.takeIf { it.kind == PersonaKind.AGENT && it.handle != "zoen" }?.let { AvatarArt.agent(context, it) } }
    val shape = if (persona?.kind == PersonaKind.AGENT && !contact) RoundedCornerShape((size * .32f).dp) else CircleShape
    val color = try { Color(android.graphics.Color.parseColor(persona?.tintHex ?: "#DDEFD2")) } catch (_: Exception) { MaterialTheme.colorScheme.primaryContainer }
    Box(modifier.size(size.dp).clip(shape).background(if (contact) MaterialTheme.colorScheme.surface else color.copy(alpha = .18f))
        .then(if (onClick != null) Modifier.clickable(onClick = onClick) else Modifier), contentAlignment = Alignment.Center) {
        if (photo != null) Image(photo!!.asImageBitmap(), persona?.name, Modifier.fillMaxSize(), contentScale = ContentScale.Crop)
        else if (persona?.handle == "zoen") ZoenMascot(Modifier.fillMaxSize().padding(2.dp))
        else if (drawing != null) AvatarDrawing(drawing, size)
        else if (persona?.kind == PersonaKind.AGENT) Icon(Icons.Rounded.AutoAwesome, persona.name, tint = MaterialTheme.colorScheme.primary)
        else Text(persona?.initials ?: "Z", style = MaterialTheme.typography.titleMedium, color = MaterialTheme.colorScheme.onSurface, fontWeight = FontWeight.SemiBold)
    }
}

@Composable
fun SpaceAvatar(space: SpaceSummary, modifier: Modifier = Modifier, size: Int = 52, onClick: (() -> Unit)? = null) {
    if (space.counterpart != null) { Avatar(space.counterpart, modifier, size, onClick); return }
    val context = LocalContext.current
    val preferences = remember(context) { context.getSharedPreferences("zoen", android.content.Context.MODE_PRIVATE) }
    var drawingVersion by remember { mutableIntStateOf(0) }
    DisposableEffect(preferences, space.id) {
        val listener = android.content.SharedPreferences.OnSharedPreferenceChangeListener { _, key -> if (key == "avatar.group.${space.id}") drawingVersion++ }
        preferences.registerOnSharedPreferenceChangeListener(listener)
        onDispose { preferences.unregisterOnSharedPreferenceChangeListener(listener) }
    }
    val drawing = remember(space.id, space.title, drawingVersion) { AvatarArt.group(context, space.id, space.title) }
    Box(modifier.then(if (onClick == null) Modifier else Modifier.clickable(onClick = onClick))) {
        if (drawing != null) AvatarDrawing(drawing, size)
        else Avatar(null, size = size)
    }
}

@Composable
fun SectionLabel(text: String, modifier: Modifier = Modifier) {
    Text(text, modifier.padding(top = 12.dp, bottom = 8.dp), style = MaterialTheme.typography.labelLarge, color = MaterialTheme.colorScheme.onSurfaceVariant)
}

@Composable
fun EmptyState(title: String, detail: String, modifier: Modifier = Modifier, action: (@Composable () -> Unit)? = null) {
    Column(modifier.fillMaxWidth().padding(28.dp), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(12.dp)) {
        ZoenMascot(Modifier.size(132.dp))
        Text(title, style = MaterialTheme.typography.titleLarge)
        Text(detail, style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
        action?.invoke()
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ScreenBar(title: String, back: (() -> Unit)? = null, actions: @Composable RowScope.() -> Unit = {}) {
    TopAppBar(title = { Text(title, maxLines = 1, overflow = TextOverflow.Ellipsis) }, navigationIcon = {
        if (back != null) IconButton(onClick = back) { Icon(Icons.AutoMirrored.Rounded.ArrowBack, stringResource(R.string.back)) }
    }, actions = actions, colors = TopAppBarDefaults.topAppBarColors(containerColor = MaterialTheme.colorScheme.surface))
}

@Composable
fun SettingsRow(icon: ImageVector, title: String, detail: String? = null, onClick: () -> Unit, trailing: (@Composable () -> Unit)? = null) {
    ListItem(
        headlineContent = { Text(title) },
        supportingContent = detail?.let { { Text(it) } },
        leadingContent = { Icon(icon, null, tint = MaterialTheme.colorScheme.primary) },
        trailingContent = trailing ?: { Icon(Icons.Rounded.ChevronRight, null, tint = MaterialTheme.colorScheme.onSurfaceVariant) },
        modifier = Modifier.clickable(onClick = onClick),
        colors = ListItemDefaults.colors(containerColor = Color.Transparent),
    )
}

fun appIcon(kind: String): ImageVector = when (kind) {
    "pet" -> Icons.Rounded.Pets
    "poll" -> Icons.Rounded.Poll
    "list" -> Icons.Rounded.Checklist
    "recipe" -> Icons.Rounded.Restaurant
    "maptap" -> Icons.Rounded.Public
    "hike" -> Icons.Rounded.Terrain
    "countdown" -> Icons.Rounded.HourglassBottom
    "page" -> Icons.Rounded.Article
    "file" -> Icons.Rounded.AttachFile
    else -> Icons.Rounded.TaskAlt
}

@Composable
fun ItemTile(item: ItemDetail, onClick: () -> Unit, modifier: Modifier = Modifier) {
    Card(onClick = onClick, modifier = modifier.width(180.dp), colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surfaceContainerLow)) {
        Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
            Icon(appIcon(item.app?.appId ?: item.kindId), null, tint = MaterialTheme.colorScheme.primary)
            Text(item.title, style = MaterialTheme.typography.titleSmall, maxLines = 2, overflow = TextOverflow.Ellipsis)
            Text(item.app?.headline ?: item.plan?.summary ?: item.kindLabel, style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant, maxLines = 2, overflow = TextOverflow.Ellipsis)
        }
    }
}

@Composable
fun PrivacyLabel(space: SpaceSummary, encrypted: Boolean) {
    val label = when {
        encrypted -> R.string.end_to_end
        space.privacy == PrivacyDto.PUBLIC -> R.string.public_space
        space.counterpart?.handle == "zoen" -> R.string.local_chat
        else -> R.string.closed_space
    }
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(4.dp)) {
        Icon(if (encrypted) Icons.Rounded.Lock else Icons.Rounded.Info, null, Modifier.size(12.dp))
        Text(stringResource(label), style = MaterialTheme.typography.labelSmall)
    }
}
