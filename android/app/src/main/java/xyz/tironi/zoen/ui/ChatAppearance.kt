package xyz.tironi.zoen.ui

import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.ImageDecoder
import android.net.Uri
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.PickVisualMediaRequest
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import java.io.ByteArrayOutputStream
import java.nio.ByteBuffer
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import org.json.JSONObject
import xyz.tironi.zoen.R
import xyz.tironi.zoen.ZoenViewModel
import xyz.tironi.zoen.core.BackgroundDto
import xyz.tironi.zoen.core.MediaRefDto
import xyz.tironi.zoen.data.FileAccess
import xyz.tironi.zoen.data.ZoenRepository

object ChatAppearanceStore {
    fun default() = BackgroundDto("none", null, 1000u, 0, 0, null, 0u, "auto")
    private fun key(repository: ZoenRepository, space: String) = repository.localKey("background", space)
    fun isLocal(repository: ZoenRepository, space: String) = repository.preferences.contains(key(repository, space))

    suspend fun load(repository: ZoenRepository, space: String): BackgroundDto {
        val local = repository.preferences.getString(key(repository, space), null)
        return local?.let(::decode) ?: repository.query { it.background(space) } ?: default()
    }

    fun decode(raw: String): BackgroundDto? = runCatching {
        val j = JSONObject(raw)
        val sha = j.optString("sha")
        val ref = if (sha.matches(Regex("[a-f0-9]{64}"))) MediaRefDto(sha, j.optString("mime", "image/jpeg"), j.optInt("width").toUInt(), j.optInt("height").toUInt(), j.optLong("bytes").toULong()) else null
        BackgroundDto(j.getString("style"), ref, j.optInt("zoom", 1000).coerceIn(1000, 4000).toUInt(),
            j.optInt("x").coerceIn(-1000, 1000), j.optInt("y").coerceIn(-1000, 1000),
            if (j.has("dim")) j.getInt("dim").coerceIn(0, 900).toUInt() else null,
            j.optInt("blur").coerceIn(0, 1000).toUInt(), j.optString("appearance", "auto").takeIf { it in listOf("auto", "light", "dark") } ?: "auto")
    }.getOrNull()

    fun encode(background: BackgroundDto): String = JSONObject().apply {
        put("style", background.style); put("zoom", background.zoomPm.toInt()); put("x", background.offsetXPm); put("y", background.offsetYPm)
        background.dimPm?.let { put("dim", it.toInt()) }; put("blur", background.blurPm.toInt()); put("appearance", background.appearance)
        background.media?.let { put("sha", it.sha256); put("mime", it.mime); put("bytes", it.bytes.toLong()); put("width", it.width.toInt()); put("height", it.height.toInt()) }
    }.toString()

    suspend fun save(repository: ZoenRepository, space: String, background: BackgroundDto, local: Boolean) {
        if (local) repository.preferences.edit().putString(key(repository, space), encode(background)).commit()
        else { repository.change { it.setBackground(space, background) }; repository.preferences.edit().remove(key(repository, space)).commit() }
        repository.refresh()
    }
}

@Composable
fun ChatBackdrop(model: ZoenViewModel, space: String, revision: Long, modifier: Modifier = Modifier) {
    val background by produceState(ChatAppearanceStore.default(), space, revision) { value = ChatAppearanceStore.load(model.repository, space) }
    BackgroundPreview(model, background, modifier)
}

@Composable
private fun BackgroundPreview(model: ZoenViewModel, background: BackgroundDto, modifier: Modifier) {
    val systemDark = isSystemInDarkTheme()
    val dark = background.appearance == "dark" || background.appearance == "auto" && systemDark
    val colors = mapOf("butter" to 0xFFFBEFC8, "mint" to 0xFFD6F2E5, "sky" to 0xFFDCEBFF, "lilac" to 0xFFE9E2FF, "blush" to 0xFFFFE3DC, "cardboard" to 0xFFF1E2CC)
    val gradients = mapOf("dawn" to listOf(0xFFFFE1C7, 0xFFF6D3F0), "forest" to listOf(0xFFE3F5D8, 0xFFBFE3C9), "ocean" to listOf(0xFFD9F1FF, 0xFFC3D8FF), "dusk" to listOf(0xFFE8E0FF, 0xFFFFD9E4), "paper" to listOf(0xFFFFFCF5, 0xFFF2E6D0))
    val token = background.style.split(':', limit = 2)
    val name = token.getOrElse(1) { "" }
    val scene = when (name) { "meadow" -> R.drawable.bg_meadow; "coast" -> R.drawable.bg_coast; "trail" -> R.drawable.bg_trail; "dusk" -> R.drawable.bg_dusk; "studio" -> R.drawable.bg_studio; else -> R.drawable.bg_paper }
    val bytes by produceState<ByteArray?>(null, background.media?.sha256) {
        value = background.media?.let { ref -> model.repository.query { it.media(ref.sha256) } }
    }
    val photo by produceState<Bitmap?>(null, bytes, background.blurPm) {
        value = withContext(Dispatchers.Default) { bytes?.let { BitmapFactory.decodeByteArray(it, 0, it.size) }?.let { blurPhoto(it, background.blurPm.toInt()) } }
    }
    Box(modifier.background(MaterialTheme.colorScheme.background)) {
        when (token[0]) {
            "color" -> Box(Modifier.matchParentSize().background(Color(colors[name] ?: 0xFFF1E2CC)))
            "gradient" -> Box(Modifier.matchParentSize().background(Brush.verticalGradient((gradients[name] ?: gradients.getValue("paper")).map(::Color))))
            "doodles" -> {
                Box(Modifier.matchParentSize().background(Color(colors[if (name == "trail") "mint" else if (name == "picnic") "butter" else "lilac"]!!)))
                Canvas(Modifier.matchParentSize()) {
                    val gap = 86.dp.toPx()
                    for (row in 0..(size.height / gap).toInt()) for (column in 0..(size.width / gap).toInt()) {
                        val p = Offset(column * gap + if (row % 2 == 0) 18.dp.toPx() else 48.dp.toPx(), row * gap + 26.dp.toPx())
                        val tint = Color(0xFF456347).copy(alpha = .18f)
                        when ((row + column) % 3) {
                            0 -> { drawCircle(tint, 9.dp.toPx(), p, style = Stroke(2.dp.toPx())); for (i in 0..7) { val a = i * Math.PI / 4; val d = Offset(kotlin.math.cos(a).toFloat(), kotlin.math.sin(a).toFloat()); drawLine(tint, p + d * 13.dp.toPx(), p + d * 17.dp.toPx(), 2.dp.toPx()) } }
                            1 -> drawPath(Path().apply { moveTo(p.x - 13, p.y + 12); lineTo(p.x, p.y - 18); lineTo(p.x + 19, p.y + 12); close() }, tint, style = Stroke(2.dp.toPx()))
                            else -> drawRoundRect(tint, p, androidx.compose.ui.geometry.Size(20.dp.toPx(), 15.dp.toPx()), androidx.compose.ui.geometry.CornerRadius(4.dp.toPx()), style = Stroke(2.dp.toPx()))
                        }
                    }
                }
            }
            "builtin", "photo" -> Box(Modifier.matchParentSize()) {
                val transform = Modifier.fillMaxSize().graphicsLayer {
                    val zoom = background.zoomPm.toFloat() / 1000
                    scaleX = zoom; scaleY = zoom
                    translationX = background.offsetXPm.toFloat() / 1000 * size.width * (zoom - 1) / 2
                    translationY = background.offsetYPm.toFloat() / 1000 * size.height * (zoom - 1) / 2
                }
                if (token[0] == "builtin") Image(painterResource(scene), null, transform, contentScale = ContentScale.Crop)
                else photo?.let { Image(it.asImageBitmap(), null, transform, contentScale = ContentScale.Crop) }
                val dim = background.dimPm?.toFloat()?.div(1000) ?: if (dark) .45f else .12f
                Box(Modifier.matchParentSize().background(Color.Black.copy(alpha = dim)))
            }
        }
        if (dark && token[0] in listOf("color", "gradient", "doodles")) Box(Modifier.matchParentSize().background(Color.Black.copy(alpha = .78f)))
    }
}

private fun blurPhoto(source: Bitmap, amount: Int): Bitmap {
    if (amount <= 0) return source
    val scale = (1 - amount.coerceIn(0, 1000) / 1050f).coerceIn(.035f, 1f)
    val small = Bitmap.createScaledBitmap(source, (source.width * scale).toInt().coerceAtLeast(8), (source.height * scale).toInt().coerceAtLeast(8), true)
    val blurred = Bitmap.createScaledBitmap(small, source.width, source.height, true)
    if (small !== source) small.recycle()
    return blurred
}

@Composable
fun ChatAppearanceScreen(model: ZoenViewModel, space: String, back: () -> Unit) {
    var background by remember { mutableStateOf(ChatAppearanceStore.default()) }
    var loaded by remember { mutableStateOf(false) }
    var local by rememberSaveable(space) { mutableStateOf(ChatAppearanceStore.isLocal(model.repository, space)) }
    var saving by remember { mutableStateOf(false) }
    val context = LocalContext.current
    LaunchedEffect(space) { background = ChatAppearanceStore.load(model.repository, space); loaded = true }
    val picker = rememberLauncherForActivityResult(ActivityResultContracts.PickVisualMedia()) { uri -> if (uri != null) model.launch {
        val image = withContext(Dispatchers.IO) { prepareBackgroundPhoto(context, uri) }
        val ref = model.repository.change { it.putMedia(image.first, "image/jpeg", image.second.first.toUInt(), image.second.second.toUInt()) }
        background = background.copy(style = "photo", media = ref)
    } }
    Scaffold(topBar = { ScreenBar(stringResource(R.string.chat_appearance), back, actions = {
        TextButton(enabled = loaded && !saving, onClick = { model.launch { saving = true; try { ChatAppearanceStore.save(model.repository, space, background, local); back() } finally { saving = false } } }) { Text(stringResource(R.string.save)) }
    }) }) { padding -> LazyColumn(Modifier.padding(padding), contentPadding = PaddingValues(20.dp), verticalArrangement = Arrangement.spacedBy(14.dp)) {
        item { Box(Modifier.fillMaxWidth().height(220.dp)) { BackgroundPreview(model, background, Modifier.matchParentSize()); Card(Modifier.align(Alignment.Center).padding(24.dp)) { Text(stringResource(R.string.chat_appearance_preview), Modifier.padding(20.dp)) } } }
        item { Row(verticalAlignment = Alignment.CenterVertically) { Column(Modifier.weight(1f)) { Text(stringResource(R.string.background_only_me)); Text(stringResource(R.string.background_shared_detail), style = MaterialTheme.typography.bodySmall) }; Switch(local, { local = it }) } }
        item { OutlinedButton(onClick = { background = ChatAppearanceStore.default() }, modifier = Modifier.testTag("background:default")) { Text(stringResource(R.string.background_default)) }; OutlinedButton(onClick = { picker.launch(PickVisualMediaRequest(ActivityResultContracts.PickVisualMedia.ImageOnly)) }) { Text(stringResource(R.string.choose_photo)) } }
        listOf("color" to listOf("butter", "mint", "sky", "lilac", "blush", "cardboard"), "gradient" to listOf("dawn", "forest", "ocean", "dusk", "paper"), "doodles" to listOf("trail", "picnic", "studio"), "builtin" to listOf("meadow", "coast", "trail", "dusk", "studio", "paper")).forEach { (type, names) ->
            item { Text(stringResource(when(type) { "color" -> R.string.background_colors; "gradient" -> R.string.background_gradients; "doodles" -> R.string.background_doodles; else -> R.string.background_scenes }), style = MaterialTheme.typography.titleMedium) }
            item { LazyRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) { items(names) { name -> FilterChip(background.style == "$type:$name", { background = background.copy(style = "$type:$name", media = null) }, label = { Text(stringResource(backgroundName(name))) }, modifier = Modifier.testTag("background:$type:$name")) } } }
        }
        if (background.style == "photo" || background.style.startsWith("builtin:")) {
            item { Text(stringResource(R.string.background_zoom)); Slider(background.zoomPm.toFloat(), { background = background.copy(zoomPm = it.toUInt()) }, valueRange = 1000f..4000f) }
            item { Text(stringResource(R.string.background_horizontal)); Slider(background.offsetXPm.toFloat(), { background = background.copy(offsetXPm = it.toInt()) }, valueRange = -1000f..1000f) }
            item { Text(stringResource(R.string.background_vertical)); Slider(background.offsetYPm.toFloat(), { background = background.copy(offsetYPm = it.toInt()) }, valueRange = -1000f..1000f) }
            item { Text(stringResource(R.string.background_dim)); Slider((background.dimPm ?: 120u).toFloat(), { background = background.copy(dimPm = it.toUInt()) }, valueRange = 0f..900f) }
            if (background.style == "photo") item { Text(stringResource(R.string.background_blur)); Slider(background.blurPm.toFloat(), { background = background.copy(blurPm = it.toUInt()) }, valueRange = 0f..1000f) }
        }
        item { Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) { listOf("auto", "light", "dark").forEach { appearance -> FilterChip(background.appearance == appearance, { background = background.copy(appearance = appearance) }, label = { Text(stringResource(when(appearance) { "light" -> R.string.background_light; "dark" -> R.string.background_dark; else -> R.string.background_auto })) }) } } }
    } }
}

private fun backgroundName(name: String) = when(name) {
    "butter" -> R.string.bg_butter; "mint" -> R.string.bg_mint; "sky" -> R.string.bg_sky; "lilac" -> R.string.bg_lilac; "blush" -> R.string.bg_blush; "cardboard" -> R.string.bg_cardboard
    "dawn" -> R.string.bg_dawn; "forest" -> R.string.bg_forest; "ocean" -> R.string.bg_ocean; "dusk" -> R.string.bg_dusk; "paper" -> R.string.bg_paper
    "trail" -> R.string.bg_trail; "picnic" -> R.string.bg_picnic; "studio" -> R.string.bg_studio; "meadow" -> R.string.bg_meadow; else -> R.string.bg_coast
}

private fun prepareBackgroundPhoto(context: android.content.Context, uri: Uri): Pair<ByteArray, Pair<Int, Int>> {
    val raw = context.contentResolver.openInputStream(uri)?.use { input ->
        val output = ByteArrayOutputStream()
        val buffer = ByteArray(8192)
        while (true) {
            val read = input.read(buffer)
            if (read < 0) break
            require(output.size() + read <= FileAccess.MAX_BYTES) { context.getString(R.string.file_too_large) }
            output.write(buffer, 0, read)
        }
        output.toByteArray()
    } ?: error(context.getString(R.string.something_wrong))
    require(raw.size <= FileAccess.MAX_BYTES) { context.getString(R.string.file_too_large) }
    val bitmap = ImageDecoder.decodeBitmap(ImageDecoder.createSource(ByteBuffer.wrap(raw))) { decoder, info, _ ->
        val scale = (2048f / maxOf(info.size.width, info.size.height)).coerceAtMost(1f)
        decoder.allocator = ImageDecoder.ALLOCATOR_SOFTWARE
        decoder.setTargetSize((info.size.width * scale).toInt().coerceAtLeast(1), (info.size.height * scale).toInt().coerceAtLeast(1))
    }
    return try { ByteArrayOutputStream().use { bitmap.compress(Bitmap.CompressFormat.JPEG, 82, it); it.toByteArray() to (bitmap.width to bitmap.height) } } finally { bitmap.recycle() }
}
