package xyz.tironi.zoen.ui

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.withContext
import org.json.JSONArray
import org.json.JSONObject
import java.text.NumberFormat
import xyz.tironi.zoen.R
import xyz.tironi.zoen.ZoenViewModel
import xyz.tironi.zoen.core.*
import xyz.tironi.zoen.data.AppState
import xyz.tironi.zoen.miniapps.McpAppView

val nativeMiniApps = setOf("pet", "poll", "list", "hike", "maptap", "recipe", "countdown")

private fun JSONObject.rows(key: String): List<JSONObject> = optJSONArray(key)?.let { rows -> (0 until rows.length()).mapNotNull { rows.optJSONObject(it) } } ?: emptyList()
private typealias AppAction = (String, JSONObject) -> Unit
private data class Confirmation(val tool: String, val args: JSONObject, val outcome: AppCallOutcome)

@Composable
fun MiniAppScreen(model: ZoenViewModel, state: AppState, item: ItemDetail, modifier: Modifier = Modifier, onClose: () -> Unit = {}) {
    val app = checkNotNull(item.app)
    var hasHtml by remember(app.appId) { mutableStateOf(app.appId in setOf("pet", "poll", "list", "hike")) }
    val preferences = model.repository.preferences
    var html by remember(app.appId, state.revision) { mutableStateOf(preferences.getBoolean("miniapps.html:${app.appId}", app.appId !in nativeMiniApps) || app.appId !in nativeMiniApps) }
    LaunchedEffect(app.appId) { hasHtml = model.repository.query { core -> core.appSpecs().any { it.id == app.appId && it.hasView } } }
    if (hasHtml && html) {
        Column(modifier.fillMaxSize()) {
            if (app.appId in nativeMiniApps) Row(Modifier.fillMaxWidth().padding(horizontal = 16.dp), horizontalArrangement = Arrangement.End) {
                TextButton(onClick = { html = false; preferences.edit().putBoolean("miniapps.html:${app.appId}", false).apply() }) { Text(stringResource(R.string.miniapp_native_view)) }
            }
            McpAppView(model, state, item, Modifier.weight(1f).fillMaxWidth(), onClose = onClose)
        }
        return
    }
    if (hasHtml) {
        Column(modifier.fillMaxSize()) {
            Row(Modifier.fillMaxWidth().padding(horizontal = 16.dp), horizontalArrangement = Arrangement.End) {
                TextButton(onClick = { html = true; preferences.edit().putBoolean("miniapps.html:${app.appId}", true).apply() }) { Text(stringResource(R.string.miniapp_html_view)) }
            }
            NativeMiniAppScreen(model, state, item, Modifier.weight(1f))
        }
    } else NativeMiniAppScreen(model, state, item, modifier)
}

@Composable
private fun NativeMiniAppScreen(model: ZoenViewModel, state: AppState, item: ItemDetail, modifier: Modifier) {
    val app = checkNotNull(item.app)
    val data = remember(app.viewJson) { JSONObject(app.viewJson) }
    var confirmation by remember { mutableStateOf<Confirmation?>(null) }
    var busy by remember { mutableStateOf(false) }
    val action: AppAction = { tool, args ->
        if (!busy) {
            busy = true
            model.launch {
                try {
                    val outcome = model.repository.change { it.appCallTool(item.id, tool, args.toString(), false) }
                    when (outcome.status) {
                        AppCallStatus.NEEDS_CONFIRMATION -> confirmation = Confirmation(tool, args, outcome)
                        AppCallStatus.DENIED -> model.notify(outcome.message)
                        AppCallStatus.DONE -> {
                            if (outcome.message.isNotBlank()) model.notify(outcome.message)
                            if (tool == "hike_decide") model.planHikeIfReady(item.id)
                        }
                    }
                } finally { busy = false }
            }
        }
    }
    Box(modifier.fillMaxSize(), contentAlignment = Alignment.TopCenter) {
        Column(Modifier.fillMaxHeight().widthIn(max = 720.dp).fillMaxWidth()) {
            if (busy) LinearProgressIndicator(Modifier.fillMaxWidth())
            when (app.appId) {
                "pet" -> PetApp(data, action, Modifier.weight(1f))
                "poll" -> PollApp(data, action, Modifier.weight(1f))
                "list" -> ListApp(data, action, Modifier.weight(1f))
                "recipe" -> RecipeApp(data, action, Modifier.weight(1f))
                "maptap" -> MapTapApp(data, state.me?.name.orEmpty(), action, Modifier.weight(1f))
                "hike" -> HikeApp(data, action, Modifier.weight(1f))
                "countdown" -> CountdownApp(data, Modifier.weight(1f))
                else -> EmptyState(app.name, app.headline)
            }
        }
    }
    confirmation?.let { pending -> AlertDialog(onDismissRequest = { confirmation = null }, title = { Text(pending.outcome.confirmTitle ?: stringResource(R.string.confirm_action)) }, text = { Text(pending.outcome.confirmDetail ?: pending.outcome.message) }, confirmButton = {
        TextButton(onClick = {
            confirmation = null
            model.launch {
                val result = model.repository.change { it.appCallTool(item.id, pending.tool, pending.args.toString(), true) }
                model.notify(result.message)
                if (result.status == AppCallStatus.DONE && pending.tool == "hike_decide") model.planHikeIfReady(item.id)
            }
        }) { Text(stringResource(R.string.approve)) }
    }, dismissButton = { TextButton(onClick = { confirmation = null }) { Text(stringResource(R.string.cancel)) } }) }
}

@Composable
private fun AppLog(data: JSONObject) {
    data.rows("log").take(6).forEach { log -> Text(log.optString("who") + " " + log.optString("what"), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant, modifier = Modifier.padding(vertical = 4.dp)) }
}

@Composable
private fun PetApp(data: JSONObject, action: AppAction, modifier: Modifier) {
    var rename by remember { mutableStateOf(false) }
    var name by rememberSaveable { mutableStateOf("") }
    var dash by rememberSaveable { mutableStateOf(false) }
    val asleep = data.optBoolean("asleep")
    val released = data.optBoolean("released")
    LazyColumn(modifier, contentPadding = PaddingValues(24.dp), verticalArrangement = Arrangement.spacedBy(18.dp)) {
        item {
            Column(Modifier.fillMaxWidth(), horizontalAlignment = Alignment.CenterHorizontally) {
                PixelDonkey(Modifier.fillMaxWidth().height(180.dp), asleep)
                Text(data.optString("name"), style = MaterialTheme.typography.displaySmall)
                Text(data.optString("mood"), color = MaterialTheme.colorScheme.onSurfaceVariant)
            }
        }
        item { Card(colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.tertiaryContainer)) {
            Column(Modifier.fillMaxWidth().padding(20.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
                listOf("fullness" to R.string.food, "joy" to R.string.mood, "energy" to R.string.rest).forEach { (key, label) ->
                    val progress = (data.optDouble(key, 0.0) / 100).toFloat().coerceIn(0f, 1f)
                    Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) { Text(stringResource(label)); Text("${(progress * 100).toInt()}%", style = MaterialTheme.typography.labelMedium) }
                    LinearProgressIndicator(progress = { progress }, modifier = Modifier.fillMaxWidth())
                }
            }
        } }
        if (!released) {
            item { Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                FilledTonalButton(onClick = { action("pet_feed", JSONObject()) }, enabled = !asleep, modifier = Modifier.weight(1f)) { Text(stringResource(R.string.feed)) }
                FilledTonalButton(onClick = { action(if (asleep) "pet_wake" else "pet_nap", JSONObject()) }, modifier = Modifier.weight(1f)) { Text(stringResource(if (asleep) R.string.wake else R.string.nap)) }
                FilledTonalButton(onClick = { action("pet_play", JSONObject()) }, enabled = !asleep, modifier = Modifier.weight(1f)) { Text(stringResource(R.string.play)) }
            } }
            item { OutlinedButton(onClick = { dash = true }, Modifier.fillMaxWidth(), enabled = !asleep) { Icon(Icons.Rounded.SportsEsports, null); Spacer(Modifier.width(8.dp)); Text(stringResource(R.string.dash)) } }
            item { Row { TextButton(onClick = { name = data.optString("name"); rename = true }) { Text(stringResource(R.string.rename)) }; Spacer(Modifier.weight(1f)); TextButton(onClick = { action("pet_release", JSONObject()) }) { Text(stringResource(R.string.release_pet), color = MaterialTheme.colorScheme.error) } } }
        }
        item { AppLog(data) }
        val best = data.optJSONObject("dash")?.optJSONObject("best")
        if (best != null) item { best.keys().asSequence().toList().sortedByDescending { best.optJSONObject(it)?.optInt("meters") ?: 0 }.forEach { person ->
            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) { Text(person); Text(stringResource(R.string.score, best.optJSONObject(person)?.optInt("meters") ?: 0)) }
        } }
    }
    if (rename) AlertDialog(onDismissRequest = { rename = false }, title = { Text(stringResource(R.string.rename)) }, text = { OutlinedTextField(name, { name = it.take(18) }, label = { Text(stringResource(R.string.name)) }) }, confirmButton = { TextButton(onClick = { rename = false; action("pet_rename", JSONObject().put("name", name)) }, enabled = name.isNotBlank()) { Text(stringResource(R.string.save)) } }, dismissButton = { TextButton(onClick = { rename = false }) { Text(stringResource(R.string.cancel)) } })
    if (dash) androidx.compose.ui.window.Dialog(onDismissRequest = { dash = false }, properties = androidx.compose.ui.window.DialogProperties(usePlatformDefaultWidth = false)) {
        Surface(Modifier.widthIn(max = 720.dp).fillMaxWidth().padding(16.dp), shape = MaterialTheme.shapes.extraLarge) { DonkeyDash(onClose = { dash = false }, onScore = { meters, carrots -> action("pet_dash_score", JSONObject().put("meters", meters).put("carrots", carrots)) }) }
    }
}

@Composable
fun PixelDonkey(modifier: Modifier = Modifier, asleep: Boolean = false) {
    Canvas(modifier.semantics { contentDescription = "Donkey" }) {
        val scale = minOf(size.width / 32, size.height / 27)
        val left = (size.width - 32 * scale) / 2
        val top = (size.height - 27 * scale) / 2
        fun block(x: Int, y: Int, w: Int, h: Int, color: Color) = drawRect(color, Offset(left + x * scale, top + y * scale), Size(w * scale, h * scale))
        val ink = Color(0xFF4B3B36); val fur = Color(0xFFA0958C); val light = Color(0xFFD3C5B4)
        block(6, 22, 21, 2, Color(0x1A443830))
        block(8, 10, 15, 9, ink); block(9, 11, 13, 7, fur)
        block(20, 5, 9, 10, ink); block(21, 6, 7, 8, fur)
        block(21, 0, 2, 7, ink); block(22, 1, 1, 5, light)
        block(26, 0, 2, 7, ink); block(26, 1, 1, 5, light)
        block(25, 11, 6, 4, light); block(29, 12, 1, 1, ink)
        block(8, 18, 3, 5, ink); block(11, 18, 2, 4, fur); block(18, 18, 3, 5, ink); block(21, 18, 2, 4, fur)
        block(5, 11, 3, 2, ink); block(4, 12, 2, 4, ink)
        block(23, 8, if (asleep) 3 else 2, if (asleep) 1 else 2, ink)
        if (!asleep) block(23, 8, 1, 1, Color.White)
        block(20, 4, 6, 2, ink)
    }
}

@Composable
private fun PollApp(data: JSONObject, action: AppAction, modifier: Modifier) {
    var add by remember { mutableStateOf(false) }
    var text by rememberSaveable { mutableStateOf("") }
    val options = data.rows("options")
    val total = options.sumOf { it.optJSONArray("votes")?.length() ?: 0 }.coerceAtLeast(1)
    val closed = data.optBoolean("closed")
    LazyColumn(modifier, contentPadding = PaddingValues(24.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
        item { Icon(Icons.Rounded.Poll, null, Modifier.size(48.dp), tint = MaterialTheme.colorScheme.primary); Text(data.optString("question"), style = MaterialTheme.typography.displaySmall) }
        items(options, key = { it.optString("id") }) { option ->
            Card(onClick = { action("poll_vote", JSONObject().put("option", option.optString("id"))) }, enabled = !closed) {
                Column(Modifier.fillMaxWidth().padding(20.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                    Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) { Text(option.optString("text"), Modifier.weight(1f), style = MaterialTheme.typography.titleMedium); Text((option.optJSONArray("votes")?.length() ?: 0).toString()) }
                    LinearProgressIndicator(progress = { (option.optJSONArray("votes")?.length() ?: 0).toFloat() / total }, modifier = Modifier.fillMaxWidth())
                }
            }
        }
        if (!closed) item { Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) { OutlinedButton(onClick = { add = true }) { Text(stringResource(R.string.add_option)) }; Button(onClick = { action("poll_close", JSONObject()) }) { Text(stringResource(R.string.close_poll)) } } }
        else if (!data.isNull("winner")) item { Text(data.optString("winner"), style = MaterialTheme.typography.titleLarge, color = MaterialTheme.colorScheme.primary) }
        item { AppLog(data) }
    }
    if (add) AddTextDialog(stringResource(R.string.add_option), text, { text = it }, { add = false }, { action("poll_add_option", JSONObject().put("text", text)); text = ""; add = false })
}

@Composable
private fun ListApp(data: JSONObject, action: AppAction, modifier: Modifier) {
    var text by rememberSaveable { mutableStateOf("") }
    LazyColumn(modifier.imePadding(), contentPadding = PaddingValues(24.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        item { Icon(Icons.Rounded.Checklist, null, Modifier.size(48.dp), tint = MaterialTheme.colorScheme.primary); Text(data.optString("title"), style = MaterialTheme.typography.displaySmall) }
        items(data.rows("items"), key = { it.optString("id") }) { row ->
            Card { Row(Modifier.fillMaxWidth().padding(8.dp), verticalAlignment = Alignment.CenterVertically) {
                Checkbox(row.optBoolean("done"), onCheckedChange = { action("list_toggle", JSONObject().put("id", row.optString("id"))) })
                Column(Modifier.weight(1f)) { Text(row.optString("text")); if (!row.isNull("by")) Text(row.optString("by"), style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurfaceVariant) }
            } }
        }
        item { Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            OutlinedTextField(text, { text = it }, Modifier.weight(1f), placeholder = { Text(stringResource(R.string.add_item)) })
            FilledIconButton(onClick = { action("list_add", JSONObject().put("text", text)); text = "" }, enabled = text.isNotBlank()) { Icon(Icons.Rounded.Add, stringResource(R.string.add_item)) }
        } }
        item { AppLog(data) }
    }
}

@Composable
private fun RecipeApp(data: JSONObject, action: AppAction, modifier: Modifier) {
    val servings = data.optInt("servings", 2)
    val ingredients = data.rows("ingredients")
    val steps = data.rows("steps")
    val cooking = data.optJSONObject("cooking")
    val currentStep = cooking?.optInt("step", 0) ?: 0
    var timerEnd by rememberSaveable { mutableLongStateOf(0L) }
    var remaining by remember { mutableLongStateOf(0L) }
    LaunchedEffect(timerEnd) { while (timerEnd > 0) { remaining = ((timerEnd - System.currentTimeMillis()) / 1000).coerceAtLeast(0); if (remaining == 0L) break; delay(1000) } }
    LazyColumn(modifier, contentPadding = PaddingValues(24.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
        item { Icon(Icons.Rounded.Restaurant, null, Modifier.size(48.dp), tint = MaterialTheme.colorScheme.tertiary); Text(data.optString("title"), style = MaterialTheme.typography.displaySmall); Text(data.optString("subtitle"), color = MaterialTheme.colorScheme.onSurfaceVariant) }
        item { Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
            Text(stringResource(R.string.servings), Modifier.weight(1f))
            OutlinedIconButton(onClick = { action("recipe_servings", JSONObject().put("servings", servings - 1)) }, enabled = servings > 1) { Icon(Icons.Rounded.Remove, null) }
            Text(servings.toString(), style = MaterialTheme.typography.titleLarge)
            OutlinedIconButton(onClick = { action("recipe_servings", JSONObject().put("servings", servings + 1)) }, enabled = servings < 12) { Icon(Icons.Rounded.Add, null) }
        } }
        item { SectionLabel(stringResource(R.string.ingredients)) }
        items(ingredients, key = { it.optString("id") }) { ingredient ->
            Row(verticalAlignment = Alignment.CenterVertically) {
                Checkbox(ingredient.optBoolean("done"), onCheckedChange = { action("recipe_check", JSONObject().put("id", ingredient.optString("id"))) })
                val quantity = ingredient.optDouble("qty") * servings / data.optInt("baseServings", 2).coerceAtLeast(1)
                Text(NumberFormat.getNumberInstance().format(quantity) + " " + ingredient.optString("unit") + " " + ingredient.optString("name"))
            }
        }
        if (cooking == null) item { Button(onClick = { action("recipe_cook", JSONObject().put("step", 0)) }, Modifier.fillMaxWidth()) { Text(stringResource(R.string.cook)) } }
        else item { Card(colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.tertiaryContainer)) {
            Column(Modifier.fillMaxWidth().padding(20.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                val step = steps.getOrNull(currentStep)
                Text(if (step == null) stringResource(R.string.done) else "${currentStep + 1}/${steps.size}", style = MaterialTheme.typography.titleMedium)
                Text(step?.optString("text").orEmpty(), style = MaterialTheme.typography.bodyLarge)
                if (step != null) {
                    val seconds = step.optLong("minutes", 0) * 60
                    if (seconds > 0) OutlinedButton(onClick = { timerEnd = System.currentTimeMillis() + seconds * 1000 }) { Icon(Icons.Rounded.Timer, null); Spacer(Modifier.width(8.dp)); Text(if (remaining > 0) "${remaining / 60}:${(remaining % 60).toString().padStart(2, '0')}" else "${seconds / 60} min") }
                    Button(onClick = { action("recipe_cook", JSONObject().put("step", currentStep + 1)); timerEnd = 0 }) { Text(stringResource(R.string.next_step)) }
                }
            }
        } }
        if (cooking == null) items(steps) { step -> Text(step.optString("text"), color = MaterialTheme.colorScheme.onSurfaceVariant) }
        item { AppLog(data) }
    }
}

@Composable
private fun MapTapApp(data: JSONObject, me: String, action: AppAction, modifier: Modifier) {
    val places = data.rows("places")
    val guesses = data.optJSONObject("guesses")?.optJSONObject(me) ?: JSONObject()
    var round by rememberSaveable { mutableIntStateOf(0) }
    var lat by rememberSaveable { mutableFloatStateOf(0f) }
    var lon by rememberSaveable { mutableFloatStateOf(0f) }
    var selected by rememberSaveable { mutableStateOf(false) }
    val guess = guesses.optJSONObject(round.toString())
    val place = places.getOrNull(round)
    LazyColumn(modifier, contentPadding = PaddingValues(24.dp), verticalArrangement = Arrangement.spacedBy(14.dp)) {
        item { Text("MapTap", style = MaterialTheme.typography.displaySmall); Text("${round + 1}/${places.size}", color = MaterialTheme.colorScheme.onSurfaceVariant) }
        if (place != null) item { Text(place.optString("name"), style = MaterialTheme.typography.headlineMedium); Text(place.optString("hint"), color = MaterialTheme.colorScheme.onSurfaceVariant) }
        item { WorldMap(lat, lon, selected, Modifier.fillMaxWidth().aspectRatio(1.8f)) { latitude, longitude -> lat = latitude; lon = longitude; selected = true } }
        if (guess == null) {
            item { Text(stringResource(R.string.map_hint), style = MaterialTheme.typography.bodySmall); Text("${"%.1f".format(lat)}°, ${"%.1f".format(lon)}°") }
            item { Slider(lat, { lat = it; selected = true }, valueRange = -90f..90f, modifier = Modifier.semantics { contentDescription = "Latitude" }); Slider(lon, { lon = it; selected = true }, valueRange = -180f..180f, modifier = Modifier.semantics { contentDescription = "Longitude" }) }
            item { Button(onClick = { action("maptap_guess", JSONObject().put("round", round).put("lat", lat).put("lon", lon)) }, enabled = selected, modifier = Modifier.fillMaxWidth()) { Text(stringResource(R.string.guess)) } }
        } else item { Card(colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.primaryContainer)) { Column(Modifier.fillMaxWidth().padding(20.dp)) { Text("${guess.optInt("points")} pts", style = MaterialTheme.typography.headlineMedium); Text("${guess.optInt("km")} km · ${place?.optString("country").orEmpty()}"); if (round < places.lastIndex) TextButton(onClick = { round++; selected = false; lat = 0f; lon = 0f }) { Text(stringResource(R.string.next_step)) } } } }
        item { AppLog(data) }
    }
}

@Composable
private fun WorldMap(lat: Float, lon: Float, selected: Boolean, modifier: Modifier, choose: (Float, Float) -> Unit) {
    val context = LocalContext.current
    val land by produceState<List<List<Offset>>>(emptyList()) {
        value = withContext(Dispatchers.IO) {
            val json = JSONArray(context.assets.open("land110.json").bufferedReader().use { it.readText() })
            (0 until json.length()).map { i ->
                val coordinates = json.getJSONArray(i)
                (0 until coordinates.length() - 1 step 2).map { j -> Offset((coordinates.getDouble(j).toFloat() + 180) / 360, (90 - coordinates.getDouble(j + 1).toFloat()) / 180) }
            }
        }
    }
    val water = MaterialTheme.colorScheme.secondaryContainer
    val fill = MaterialTheme.colorScheme.primary.copy(alpha = .42f)
    val ink = MaterialTheme.colorScheme.primary
    Canvas(modifier.pointerInput(Unit) { detectTapGestures { choose((90 - it.y / size.height * 180).coerceIn(-90f, 90f), (it.x / size.width * 360 - 180).coerceIn(-180f, 180f)) } }) {
        drawRoundRect(water, cornerRadius = androidx.compose.ui.geometry.CornerRadius(18.dp.toPx()))
        for (x in 1..5) drawLine(ink.copy(alpha = .1f), Offset(size.width * x / 6, 0f), Offset(size.width * x / 6, size.height))
        for (y in 1..3) drawLine(ink.copy(alpha = .1f), Offset(0f, size.height * y / 4), Offset(size.width, size.height * y / 4))
        land.forEach { polygon ->
            val path = Path().apply { polygon.forEachIndexed { i, point -> if (i == 0) moveTo(point.x * size.width, point.y * size.height) else lineTo(point.x * size.width, point.y * size.height) }; close() }
            drawPath(path, fill); drawPath(path, ink.copy(alpha = .65f), style = Stroke(1f))
        }
        if (selected) {
            val point = Offset((lon + 180) / 360 * size.width, (90 - lat) / 180 * size.height)
            drawCircle(Color.White, 7.dp.toPx(), point); drawCircle(ink, 4.dp.toPx(), point)
        }
    }
}

@Composable
private fun HikeApp(data: JSONObject, action: AppAction, modifier: Modifier) {
    val trails = data.rows("trails")
    val decided = !data.isNull("decided")
    LazyColumn(modifier, contentPadding = PaddingValues(24.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
        item { Icon(Icons.Rounded.Terrain, null, Modifier.size(64.dp), tint = MaterialTheme.colorScheme.primary); Text(data.optString("title"), style = MaterialTheme.typography.displaySmall); Text(data.optString("day") + " · " + data.optString("area"), color = MaterialTheme.colorScheme.onSurfaceVariant) }
        items(trails, key = { it.optString("id") }) { trail ->
            Card(onClick = { action("hike_vote", JSONObject().put("trail", trail.optString("id"))) }, enabled = !decided) {
                Column(Modifier.fillMaxWidth().padding(20.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) { Text(trail.optString("name"), style = MaterialTheme.typography.titleLarge); Text("${trail.optJSONArray("votes")?.length() ?: 0} " + stringResource(R.string.vote), color = MaterialTheme.colorScheme.primary); if (data.optString("decided") == trail.optString("id")) Icon(Icons.Rounded.CheckCircle, stringResource(R.string.done), tint = MaterialTheme.colorScheme.primary) }
            }
        }
        if (!decided) item { Button(onClick = { action("hike_decide", JSONObject()) }, enabled = trails.any { (it.optJSONArray("votes")?.length() ?: 0) > 0 }, modifier = Modifier.fillMaxWidth()) { Text(stringResource(R.string.hike_decide)) } }
        items(data.rows("itinerary")) { row -> Text(row.optString("time") + " · " + row.optString("text")) }
        item { AppLog(data) }
    }
}

@Composable
private fun CountdownApp(data: JSONObject, modifier: Modifier) {
    var now by remember { mutableLongStateOf(System.currentTimeMillis()) }
    LaunchedEffect(Unit) { while (true) { delay(1000); now = System.currentTimeMillis() } }
    val seconds = ((data.optLong("target") - now) / 1000).coerceAtLeast(0)
    Column(modifier.fillMaxWidth().padding(32.dp), verticalArrangement = Arrangement.spacedBy(24.dp), horizontalAlignment = Alignment.CenterHorizontally) {
        ZoenMascot(Modifier.size(200.dp), animated = true, pose = 6)
        Text(data.optString("title"), style = MaterialTheme.typography.displaySmall)
        if (!data.isNull("place")) Text(data.optString("place"), color = MaterialTheme.colorScheme.onSurfaceVariant)
        Text(stringResource(R.string.days_left, (seconds / 86400).toInt()), style = MaterialTheme.typography.headlineMedium)
        Text("${seconds / 3600 % 24} : ${seconds / 60 % 60} : ${seconds % 60}", style = MaterialTheme.typography.titleLarge)
    }
}

@Composable
private fun AddTextDialog(title: String, text: String, changed: (String) -> Unit, dismiss: () -> Unit, save: () -> Unit) {
    AlertDialog(onDismissRequest = dismiss, title = { Text(title) }, text = { OutlinedTextField(text, changed) }, confirmButton = { TextButton(onClick = save, enabled = text.isNotBlank()) { Text(stringResource(R.string.save)) } }, dismissButton = { TextButton(onClick = dismiss) { Text(stringResource(R.string.cancel)) } })
}
