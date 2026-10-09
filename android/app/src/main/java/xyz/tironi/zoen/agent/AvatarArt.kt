package xyz.tironi.zoen.agent

import android.content.Context
import android.graphics.BitmapFactory
import android.graphics.ImageDecoder
import android.graphics.drawable.AnimatedImageDrawable
import android.provider.Settings
import android.widget.ImageView
import androidx.compose.foundation.Image
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.lifecycle.compose.currentStateAsState
import org.json.JSONObject
import xyz.tironi.zoen.R
import xyz.tironi.zoen.core.Persona

data class AvatarAsset(val id: String, val name: String, val kind: String, val tags: List<String>)

object AvatarArt {
    private var catalog: List<AvatarAsset>? = null
    @Synchronized fun assets(context: Context): List<AvatarAsset> = catalog ?: context.assets.open("avatars/manifest.json").use { stream ->
        val rows = JSONObject(stream.bufferedReader().readText()).getJSONArray("assets")
        (0 until rows.length()).map { index -> val row = rows.getJSONObject(index); val tags = row.getJSONArray("tags")
            AvatarAsset(row.getString("id"), row.getString("name"), row.getString("kind"), (0 until tags.length()).map(tags::getString)) }
    }.also { catalog = it }

    fun choose(context: Context, kind: String, seed: String, tags: List<String> = emptyList()): AvatarAsset? {
        val preferences = context.getSharedPreferences("zoen", Context.MODE_PRIVATE)
        val pool = assets(context).filter { it.kind == kind }
        val saved = preferences.getString("avatar.$kind.$seed", null)
        pool.firstOrNull { it.id == saved }?.let { return it }
        pool.maxByOrNull { asset -> asset.tags.count { tag -> tags.any { it == tag || it.contains(tag) || tag.contains(it) } } }
            ?.takeIf { asset -> asset.tags.any { tag -> tags.any { it == tag || it.contains(tag) || tag.contains(it) } } }?.let { return it }
        var hash = 2166136261u
        seed.codePoints().forEach { hash = (hash xor it.toUInt()) * 16777619u }
        return pool.getOrNull(((hash % 9973u).toInt() % pool.size.coerceAtLeast(1)))
    }

    fun agent(context: Context, persona: Persona): AvatarAsset? {
        val tags = when (persona.handle.lowercase()) {
            "zoen" -> listOf("mascot", "assistant", "general")
            "financeiro", "finance" -> listOf("finance", "budget", "money")
            "organizador", "organizer" -> listOf("tasks", "productivity", "todo", "calendar")
            "guia", "guide" -> listOf("travel", "trips", "planner")
            else -> emptyList()
        }
        return choose(context, "agent", persona.id.ifEmpty { persona.handle }, tags)
    }

    fun group(context: Context, id: String, title: String): AvatarAsset? {
        val name = folded(title)
        val rules = listOf(
            listOf("viajantes", "litoral", "coastal", "praia", "beach") to listOf("beach", "trip", "vacation", "summer"),
            listOf("hike", "trail", "trilha", "turma", "sabado", "saturday", "crew", "pedra") to listOf("hiking", "mountains", "outdoors"),
            listOf("paraty", "feriado", "viagem") to listOf("road trip", "travel"),
            listOf("produto", "product", "work", "trabalho", "office", "zoen ·") to listOf("work", "team", "office", "laptop"),
            listOf("jantar", "dinner", "cook", "food") to listOf("cooking", "food", "dinner"),
            listOf("music", "musica", "band") to listOf("music", "band"),
            listOf("pet", "dog", "cat", "pacoca") to listOf("pets"),
            listOf("study", "estudo", "school") to listOf("study", "school"),
            listOf("party", "festa") to listOf("party", "celebration"),
            listOf("game", "jogo") to listOf("games", "gaming"),
            listOf("bike", "pedal") to listOf("cycling", "bike"),
            listOf("gym", "treino") to listOf("gym", "fitness"),
            listOf("book", "livro") to listOf("books", "reading"),
            listOf("camp", "acamp") to listOf("camping", "outdoors"),
            listOf("picnic") to listOf("picnic", "weekend"),
            listOf("road") to listOf("road trip", "travel"),
            listOf("city", "cidade") to listOf("city", "neighborhood"),
            listOf("plant", "jardim") to listOf("plants", "garden"),
            listOf("coffee", "cafe") to listOf("coffee", "friends"),
            listOf("family", "familia") to listOf("family", "dinner"),
            listOf("soccer", "futebol") to listOf("soccer", "sports"),
            listOf("birth", "anivers") to listOf("birthday", "celebration"))
        val tags = rules.firstOrNull { (words, _) -> words.any(name::contains) }?.second.orEmpty()
        return choose(context, "group", id.ifEmpty { title }, tags)
    }
}

@Composable
fun AvatarDrawing(asset: AvatarAsset, size: Int, animate: Boolean = true, modifier: Modifier = Modifier) {
    val context = LocalContext.current
    val lifecycle by LocalLifecycleOwner.current.lifecycle.currentStateAsState()
    val moving = animate && size >= 56 && lifecycle.isAtLeast(Lifecycle.State.STARTED) && Settings.Global.getFloat(context.contentResolver, Settings.Global.ANIMATOR_DURATION_SCALE, 1f) > 0f
    val still = remember(asset.id) { context.assets.open("avatars/stills/${asset.name}.png").use(BitmapFactory::decodeStream) }
    if (moving) {
        val drawable = remember(asset.id) { ImageDecoder.decodeDrawable(ImageDecoder.createSource(context.assets, "avatars/loops/${asset.name}.webp")) }
        DisposableEffect(drawable) { (drawable as? AnimatedImageDrawable)?.start(); onDispose { (drawable as? AnimatedImageDrawable)?.stop() } }
        AndroidView(factory = { ImageView(it).apply { scaleType = ImageView.ScaleType.CENTER_CROP; contentDescription = asset.name } },
            update = { it.setImageDrawable(drawable) }, modifier = modifier.size(size.dp).clip(CircleShape))
    } else if (still != null) Image(still.asImageBitmap(), asset.name, modifier.size(size.dp).clip(CircleShape), contentScale = ContentScale.Crop)
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun AvatarArtPicker(kind: String, id: String, selected: AvatarAsset?, close: () -> Unit) {
    val context = LocalContext.current
    var selection by remember { mutableStateOf(selected?.id) }
    val pool = remember(kind) { AvatarArt.assets(context).filter { it.kind == kind } }
    ModalBottomSheet(onDismissRequest = close) {
        Text(stringResource(R.string.agent_choose_drawing), Modifier.padding(24.dp), style = MaterialTheme.typography.titleLarge)
        LazyVerticalGrid(GridCells.Adaptive(88.dp), Modifier.fillMaxWidth().heightIn(max = 440.dp), contentPadding = PaddingValues(20.dp), verticalArrangement = Arrangement.spacedBy(16.dp), horizontalArrangement = Arrangement.spacedBy(16.dp)) {
            items(pool, key = { it.id }) { asset ->
                Card(onClick = { selection = asset.id; context.getSharedPreferences("zoen", Context.MODE_PRIVATE).edit().putString("avatar.$kind.$id", asset.id).apply() },
                    colors = CardDefaults.cardColors(containerColor = if (selection == asset.id) MaterialTheme.colorScheme.primaryContainer else MaterialTheme.colorScheme.surface)) {
                    AvatarDrawing(asset, 76, animate = false, modifier = Modifier.padding(4.dp))
                }
            }
        }
        TextButton(onClick = close, Modifier.fillMaxWidth().padding(12.dp)) { Text(stringResource(R.string.done)) }
    }
}
