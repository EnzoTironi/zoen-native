package xyz.tironi.zoen.ui

import androidx.compose.foundation.Canvas
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
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.delay
import org.json.JSONArray
import org.json.JSONObject
import xyz.tironi.zoen.R
import xyz.tironi.zoen.ZoenViewModel
import xyz.tironi.zoen.core.*
import xyz.tironi.zoen.data.AppState
import xyz.tironi.zoen.miniapps.McpAppView
import xyz.tironi.zoen.miniapps.NativeHike
import xyz.tironi.zoen.miniapps.MiniAppPins
import xyz.tironi.zoen.miniapps.NativeGlobe

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
                "maptap" -> MapTapApp(data, state, action, Modifier.weight(1f))
                "hike" -> NativeHike(model, state, item, data, action, Modifier.weight(1f))
                "countdown" -> CountdownApp(model, item, data, Modifier.weight(1f))
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
    var emote by remember { mutableStateOf<String?>(null) }
    val emoteLabel = stringResource(R.string.miniapp_emotes)
    LaunchedEffect(emote) { if (emote != null) { delay(1_800); emote = null } }
    val asleep = data.optBoolean("asleep")
    val released = data.optBoolean("released")
    LazyColumn(modifier, contentPadding = PaddingValues(24.dp), verticalArrangement = Arrangement.spacedBy(18.dp)) {
        item {
            Column(Modifier.fillMaxWidth(), horizontalAlignment = Alignment.CenterHorizontally) {
                PixelDonkey(Modifier.fillMaxWidth().height(180.dp), asleep)
                Text(data.optString("name"), style = MaterialTheme.typography.displaySmall)
                Text(data.optString("mood"), color = MaterialTheme.colorScheme.onSurfaceVariant)
                if (emote != null) Text(emote.orEmpty(), style = MaterialTheme.typography.displayMedium)
                Row(Modifier.semantics { contentDescription = emoteLabel }) {
                    listOf("❤️", "🥕", "😂", "🫶", "😴").forEach { emoji -> TextButton(onClick = { emote = emoji }, enabled = !asleep && !released) { Text(emoji, style = MaterialTheme.typography.headlineSmall) } }
                }
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
        if (best != null && best.length() > 0) item { Text(stringResource(R.string.miniapp_leaderboard), style = MaterialTheme.typography.titleLarge); best.keys().asSequence().toList().sortedByDescending { best.optJSONObject(it)?.optInt("meters") ?: 0 }.forEachIndexed { rank, person ->
            Row(Modifier.fillMaxWidth().padding(vertical = 8.dp), horizontalArrangement = Arrangement.SpaceBetween) { Text("${rank + 1}. $person"); Text(stringResource(R.string.score, best.optJSONObject(person)?.optInt("meters") ?: 0)) }
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
        drawPixelDonkey(asleep) { x, y, width, height, color -> block(x, y, width, height, Color(color)) }
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
        item { OutlinedButton(onClick = { action("list_send_whatsapp", JSONObject()) }, Modifier.fillMaxWidth()) { Icon(Icons.Rounded.Share, null); Spacer(Modifier.width(8.dp)); Text(stringResource(R.string.miniapp_send_whatsapp)) } }
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
            OutlinedIconButton(onClick = { action("recipe_servings", JSONObject().put("servings", servings - 1)) }, enabled = servings > 1) { Icon(Icons.Rounded.Remove, stringResource(R.string.miniapp_servings_less)) }
            Text(servings.toString(), style = MaterialTheme.typography.titleLarge)
            OutlinedIconButton(onClick = { action("recipe_servings", JSONObject().put("servings", servings + 1)) }, enabled = servings < 12) { Icon(Icons.Rounded.Add, stringResource(R.string.miniapp_servings_more)) }
        } }
        item { SectionLabel(stringResource(R.string.ingredients)) }
        items(ingredients, key = { it.optString("id") }) { ingredient ->
            Row(verticalAlignment = Alignment.CenterVertically) {
                Checkbox(ingredient.optBoolean("done"), onCheckedChange = { action("recipe_check", JSONObject().put("id", ingredient.optString("id"))) })
                val quantity = ingredient.optDouble("qty") * servings / data.optInt("baseServings", 2).coerceAtLeast(1)
                Text(recipeAmount(quantity, ingredient.optString("unit")) + " " + ingredient.optString("name"), textDecoration = if (ingredient.optBoolean("done")) TextDecoration.LineThrough else null)
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
                    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        OutlinedButton(onClick = { action("recipe_cook", JSONObject().put("step", currentStep - 1)) }, enabled = currentStep > 0) { Text(stringResource(R.string.miniapp_back_step)) }
                        Button(onClick = { action("recipe_cook", JSONObject().put("step", currentStep + 1)); timerEnd = 0 }) { Text(stringResource(if (currentStep + 1 >= steps.size) R.string.miniapp_finish_cooking else R.string.next_step)) }
                    }
                }
                if (step == null) OutlinedButton(onClick = { action("recipe_cook", JSONObject().put("step", 0)) }) { Text(stringResource(R.string.retry)) }
            }
        } }
        if (timerEnd > 0) item { Card { Row(Modifier.fillMaxWidth().padding(16.dp), verticalAlignment = Alignment.CenterVertically) {
            Column(Modifier.weight(1f)) {
                Text("${remaining / 60}:${(remaining % 60).toString().padStart(2, '0')}", style = MaterialTheme.typography.titleLarge)
                Text(stringResource(if (remaining == 0L) R.string.miniapp_timer_done else R.string.miniapp_kitchen_timer), style = MaterialTheme.typography.bodySmall)
            }
            IconButton(onClick = { timerEnd = 0 }) { Icon(Icons.Rounded.Close, stringResource(R.string.miniapp_timer_stop)) }
        } } }
        if (cooking == null) items(steps) { step -> Text(step.optString("text"), color = MaterialTheme.colorScheme.onSurfaceVariant) }
        item { AppLog(data) }
    }
}

@Composable
private fun MapTapApp(data: JSONObject, state: AppState, action: AppAction, modifier: Modifier) {
    val me = state.me?.name.orEmpty()
    val places = data.rows("places")
    val guesses = data.optJSONObject("guesses")?.optJSONObject(me) ?: JSONObject()
    var round by rememberSaveable(me) { mutableIntStateOf((0 until places.size).firstOrNull { guesses.isNull(it.toString()) } ?: places.lastIndex.coerceAtLeast(0)) }
    var board by rememberSaveable(me) { mutableStateOf(guesses.length() >= places.size && places.isNotEmpty()) }
    var lat by rememberSaveable { mutableFloatStateOf(0f) }
    var lon by rememberSaveable { mutableFloatStateOf(0f) }
    var selected by rememberSaveable { mutableStateOf(false) }
    val guess = guesses.optJSONObject(round.toString())
    val place = places.getOrNull(round)
    val latitude = stringResource(R.string.miniapp_latitude)
    val longitude = stringResource(R.string.miniapp_longitude)
    LazyColumn(modifier, contentPadding = PaddingValues(24.dp), verticalArrangement = Arrangement.spacedBy(14.dp)) {
        item {
            Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                Text("MapTap", Modifier.weight(1f), style = MaterialTheme.typography.headlineLarge)
                TextButton(onClick = { board = !board }) { Text(stringResource(if (board) R.string.play else R.string.miniapp_leaderboard)) }
            }
        }
        if (board) {
            val all = data.optJSONObject("guesses") ?: JSONObject()
            val players = all.keys().asSequence().map { name -> name to (all.optJSONObject(name) ?: JSONObject()) }.toList()
                .sortedByDescending { (_, rounds) -> rounds.keys().asSequence().sumOf { rounds.optJSONObject(it)?.optInt("points") ?: 0 } }
            items(players, key = { it.first }) { (name, rounds) ->
                val points = rounds.keys().asSequence().sumOf { rounds.optJSONObject(it)?.optInt("points") ?: 0 }
                val person = state.me?.takeIf { it.name == name } ?: state.spaces.flatMap { it.members }.firstOrNull { it.name == name }
                Card { Row(Modifier.fillMaxWidth().padding(16.dp), horizontalArrangement = Arrangement.spacedBy(12.dp), verticalAlignment = Alignment.CenterVertically) {
                    Text((players.indexOfFirst { it.first == name } + 1).toString(), style = MaterialTheme.typography.labelLarge)
                    Avatar(person, size = 32)
                    Column(Modifier.weight(1f)) { Text(name, style = MaterialTheme.typography.titleMedium); Text("${rounds.length()}/${places.size}", style = MaterialTheme.typography.labelSmall) }
                    Text(stringResource(R.string.miniapp_score_points, points), style = MaterialTheme.typography.labelLarge)
                } }
            }
            if (players.isEmpty()) item { Text(stringResource(R.string.miniapp_no_scores)) }
        } else {
            item { Text("${round + 1}/${places.size}", color = MaterialTheme.colorScheme.onSurfaceVariant) }
            if (place != null) item { Text(place.optString("name"), style = MaterialTheme.typography.headlineMedium); Text(place.optString("hint"), color = MaterialTheme.colorScheme.onSurfaceVariant) }
            item {
                NativeGlobe(
                    guess = if (guess != null) Offset(guess.optDouble("lon").toFloat(), guess.optDouble("lat").toFloat()) else if (selected) Offset(lon, lat) else null,
                    actual = if (guess != null && place != null) Offset(place.optDouble("lon").toFloat(), place.optDouble("lat").toFloat()) else null,
                    modifier = Modifier.fillMaxWidth().aspectRatio(1f),
                ) { latitudeValue, longitudeValue -> lat = latitudeValue; lon = longitudeValue; selected = true }
            }
            if (guess == null) {
                item { Text(stringResource(R.string.map_hint), style = MaterialTheme.typography.bodySmall); Text("${"%.1f".format(lat)}°, ${"%.1f".format(lon)}°") }
                item { Slider(lat, { lat = it; selected = true }, valueRange = -90f..90f, modifier = Modifier.semantics { contentDescription = latitude }); Slider(lon, { lon = it; selected = true }, valueRange = -180f..180f, modifier = Modifier.semantics { contentDescription = longitude }) }
                item { Button(onClick = { action("maptap_guess", JSONObject().put("round", round).put("lat", lat).put("lon", lon)) }, enabled = selected, modifier = Modifier.fillMaxWidth()) { Text(stringResource(R.string.guess)) } }
            } else item { Card(colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.primaryContainer)) { Column(Modifier.fillMaxWidth().padding(20.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                Text(stringResource(R.string.miniapp_score_points, guess.optInt("points")), style = MaterialTheme.typography.headlineMedium)
                Text("${guess.optInt("km")} km · ${place?.optString("country").orEmpty()}")
                Button(onClick = { if (round < places.lastIndex) { round++; selected = false; lat = 0f; lon = 0f } else board = true }) { Text(stringResource(if (round < places.lastIndex) R.string.next_step else R.string.miniapp_leaderboard)) }
            } } }
        }
        item { AppLog(data) }
    }
}

@Composable
private fun CountdownApp(model: ZoenViewModel, item: ItemDetail, data: JSONObject, modifier: Modifier) {
    var now by remember { mutableLongStateOf(System.currentTimeMillis()) }
    LaunchedEffect(Unit) { while (true) { delay(1000); now = System.currentTimeMillis() } }
    val target = data.optLong("target")
    val seconds = ((target - now) / 1000).coerceAtLeast(0)
    val pins = remember(model.repository.preferences) { MiniAppPins(model.repository.preferences) }
    var pinned by remember(item.id) { mutableStateOf(item.id !in pins.hidden()) }
    LazyColumn(modifier, contentPadding = PaddingValues(24.dp), verticalArrangement = Arrangement.spacedBy(24.dp)) {
        item { Card(colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.tertiaryContainer)) {
            Column(Modifier.fillMaxWidth().padding(20.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                ZoenMascot(Modifier.fillMaxWidth().height(180.dp), animated = true, pose = 6)
                Text(data.optString("title"), style = MaterialTheme.typography.headlineLarge)
                if (!data.isNull("place")) Text(data.optString("place"), style = MaterialTheme.typography.titleLarge)
                Text(java.text.DateFormat.getDateTimeInstance(java.text.DateFormat.FULL, java.text.DateFormat.SHORT).format(java.util.Date(target)), color = MaterialTheme.colorScheme.onTertiaryContainer)
            }
        } }
        item {
            val values = listOf(seconds / 86400 to R.string.miniapp_days, seconds / 3600 % 24 to R.string.miniapp_hours, seconds / 60 % 60 to R.string.miniapp_minutes, seconds % 60 to R.string.miniapp_seconds)
            val columns = if (androidx.compose.ui.platform.LocalDensity.current.fontScale > 1.3f) 2 else 4
            Column(verticalArrangement = Arrangement.spacedBy(10.dp)) { values.chunked(columns).forEach { row ->
                Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(8.dp)) { row.forEach { (value, label) ->
                    Card(Modifier.weight(1f)) { Column(Modifier.fillMaxWidth().padding(vertical = 16.dp), horizontalAlignment = Alignment.CenterHorizontally) { Text(value.toString(), style = MaterialTheme.typography.headlineMedium); Text(stringResource(label), style = MaterialTheme.typography.labelSmall) } }
                } }
            } }
        }
        item { OutlinedButton(onClick = { pinned = !pinned; if (pinned) pins.show(item.id) else pins.hide(item.id); model.launch { model.repository.refresh() } }, Modifier.fillMaxWidth()) { Icon(Icons.Rounded.PushPin, null); Spacer(Modifier.width(8.dp)); Text(stringResource(if (pinned) R.string.miniapp_unpin_home else R.string.miniapp_pin_home)) } }
        item { AppLog(data) }
    }
}

private fun recipeAmount(quantity: Double, unit: String): String {
    if (unit == "ml") return "${kotlin.math.round(quantity / 10).toInt() * 10} ml"
    val whole = quantity.toInt(); val fraction = quantity - whole
    val part = when { fraction < .13 -> ""; fraction < .38 -> "¼"; fraction < .63 -> "½"; fraction < .88 -> "¾"; else -> "" }
    val number = if (fraction >= .88) (whole + 1).toString() else if (whole == 0 && part.isNotEmpty()) part else "$whole$part"
    val plural = if (quantity > 1.13) when (unit) { "colher" -> "colheres"; "maço" -> "maços"; "bunch" -> "bunches"; else -> unit } else unit
    return "$number $plural"
}

@Composable
private fun AddTextDialog(title: String, text: String, changed: (String) -> Unit, dismiss: () -> Unit, save: () -> Unit) {
    AlertDialog(onDismissRequest = dismiss, title = { Text(title) }, text = { OutlinedTextField(text, changed) }, confirmButton = { TextButton(onClick = save, enabled = text.isNotBlank()) { Text(stringResource(R.string.save)) } }, dismissButton = { TextButton(onClick = dismiss) { Text(stringResource(R.string.cancel)) } })
}
