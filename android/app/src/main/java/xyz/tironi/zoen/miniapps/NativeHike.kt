package xyz.tironi.zoen.miniapps

import android.content.Intent
import android.graphics.BitmapFactory
import android.net.Uri
import android.util.Base64
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.Image
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import java.time.DayOfWeek
import java.time.ZonedDateTime
import java.time.temporal.TemporalAdjusters
import kotlin.math.*
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import org.json.JSONArray
import org.json.JSONObject
import xyz.tironi.zoen.R
import xyz.tironi.zoen.ZoenViewModel
import xyz.tironi.zoen.core.ItemDetail
import xyz.tironi.zoen.data.AppState
import xyz.tironi.zoen.ui.ScreenBar

private fun JSONObject.objects(key: String) = optJSONArray(key)?.let { array -> (0 until array.length()).mapNotNull { array.optJSONObject(it) } }.orEmpty()
private fun JSONObject.names(key: String) = optJSONArray(key)?.let { array -> (0 until array.length()).map { array.optString(it) } }.orEmpty()
private fun trailPhoto(id: String) = when (id) { "steep" -> R.drawable.miniapp_hike_steep; "lands" -> R.drawable.miniapp_hike_lands; else -> R.drawable.miniapp_hike_tomales }

@Composable
fun NativeHike(model: ZoenViewModel, state: AppState, item: ItemDetail, data: JSONObject, action: (String, JSONObject) -> Unit, modifier: Modifier = Modifier) {
    val context = LocalContext.current
    val access = rememberMiniAppNativeAccess(model, item)
    var trails by remember { mutableStateOf<List<JSONObject>>(emptyList()) }
    LaunchedEffect(Unit) { trails = withContext(Dispatchers.IO) {
        val json = JSONArray(context.assets.open("miniapp_hike_trails.json").bufferedReader().use { it.readText() })
        (0 until json.length()).map { json.getJSONObject(it) }
    } }
    var filter by rememberSaveable { mutableIntStateOf(0) }
    var detail by rememberSaveable { mutableStateOf<String?>(null) }
    var compare by rememberSaveable { mutableStateOf(false) }
    var busy by remember { mutableStateOf(false) }
    var here by remember { mutableStateOf<JSONObject?>(null) }
    var mine by remember(item.id) { mutableStateOf<List<JSONObject>>(emptyList()) }
    var share by remember { mutableStateOf(false) }
    val votes = data.objects("trails").associate { it.optString("id") to it.names("votes") }
    val decided = data.optString("decided").takeIf { it in setOf("tomales", "steep", "lands") }
    val shown = trails.filter { when (filter) { 1 -> it.optString("shade") != "None"; 2 -> it.optDouble("miles") < 5; 3 -> it.optString("id") != "steep"; else -> true } }.sortedBy { if (it.optString("id") == decided) 0 else 1 }
    fun native(capability: String, args: JSONObject, done: (Any) -> Unit) {
        if (!busy) model.launch { busy = true; try { done(access(capability, args)) } finally { busy = false } }
    }
    LazyColumn(modifier, contentPadding = PaddingValues(20.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
        item {
            Text(data.optString("title"), style = MaterialTheme.typography.headlineLarge)
            Text(data.optString("day") + " · " + data.optString("area"), color = MaterialTheme.colorScheme.onSurfaceVariant)
            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.End) { TextButton(onClick = { compare = true }) { Icon(Icons.Rounded.CompareArrows, null); Text(stringResource(R.string.miniapp_compare)) } }
            Row(Modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                listOf(R.string.miniapp_all_hikes, R.string.miniapp_shady, R.string.miniapp_short_hikes, R.string.miniapp_ocean).forEachIndexed { index, label -> FilterChip(filter == index, onClick = { filter = index }, label = { Text(stringResource(label)) }) }
            }
        }
        items(shown, key = { it.optString("id") }) { trail ->
            val id = trail.optString("id")
            Card(onClick = { detail = id }) {
                Image(painterResource(trailPhoto(id)), trail.optString("name"), Modifier.fillMaxWidth().height(172.dp), contentScale = ContentScale.Crop)
                Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                    Text(trail.optString("name"), style = MaterialTheme.typography.titleLarge)
                    Text(trail.optString("park"), style = MaterialTheme.typography.bodySmall)
                    Text("${trail.optDouble("miles")} mi · ${trail.optString("time")} · ${trail.optString("difficulty")}", style = MaterialTheme.typography.bodyMedium)
                    Text(votes[id].orEmpty().joinToString().ifEmpty { stringResource(R.string.miniapp_no_votes) }, color = MaterialTheme.colorScheme.primary)
                    if (decided == id) Text(stringResource(R.string.miniapp_going), style = MaterialTheme.typography.labelLarge, color = MaterialTheme.colorScheme.primary)
                }
            }
        }
        if (decided == null) item { Button(onClick = { action("hike_decide", JSONObject()) }, enabled = votes.values.any { it.isNotEmpty() }, modifier = Modifier.fillMaxWidth()) { Text(stringResource(R.string.hike_decide)) } }
        if (data.objects("itinerary").isNotEmpty()) item { HikeItinerary(data.objects("itinerary")) }
        item { data.objects("log").take(6).forEach { Text(it.optString("who") + " " + it.optString("what"), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant) } }
    }
    detail?.let { id -> trails.firstOrNull { it.optString("id") == id }?.let { trail ->
        Dialog(onDismissRequest = { detail = null }, properties = DialogProperties(usePlatformDefaultWidth = false)) {
            Scaffold(topBar = { ScreenBar(trail.optString("name"), { detail = null }) }) { padding ->
                LazyColumn(Modifier.fillMaxSize().padding(padding), contentPadding = PaddingValues(bottom = 32.dp), verticalArrangement = Arrangement.spacedBy(18.dp)) {
                    item { Image(painterResource(trailPhoto(id)), trail.optString("name"), Modifier.fillMaxWidth().height(240.dp), contentScale = ContentScale.Crop) }
                    item { Column(Modifier.padding(horizontal = 20.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                        Text(trail.optString("name"), style = MaterialTheme.typography.headlineLarge)
                        Text(trail.optString("park"), color = MaterialTheme.colorScheme.onSurfaceVariant)
                        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) {
                            HikeStat("${trail.optDouble("miles")} mi", stringResource(R.string.miniapp_distance))
                            HikeStat("${trail.optInt("climbFt")} ft", stringResource(R.string.miniapp_climb))
                            HikeStat(trail.optString("time"), stringResource(R.string.miniapp_time))
                        }
                        HikeRoute(trail, Modifier.fillMaxWidth().height(220.dp))
                        OutlinedButton(onClick = { context.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse("geo:${trail.getJSONArray("trailhead").getDouble(1)},${trail.getJSONArray("trailhead").getDouble(0)}?q=${trail.getJSONArray("trailhead").getDouble(1)},${trail.getJSONArray("trailhead").getDouble(0)}(${Uri.encode(trail.optString("name"))})"))) }) { Icon(Icons.Rounded.Directions, null); Text(stringResource(R.string.miniapp_directions)) }
                        if (here == null) TextButton(onClick = { native("location.approximate", JSONObject()) { here = it as JSONObject } }, enabled = !busy) { Text(stringResource(R.string.miniapp_distance_from_you)) }
                        else Text(stringResource(R.string.miniapp_miles_away, distanceMiles(here!!, trail).roundToInt()))
                        Text(stringResource(R.string.miniapp_about_trail), style = MaterialTheme.typography.titleLarge)
                        Text(trail.optString("about"))
                        Text(trail.names("attributes").joinToString(" · "), style = MaterialTheme.typography.bodySmall)
                        Text(votes[id].orEmpty().joinToString(), color = MaterialTheme.colorScheme.primary)
                        if (decided == null) Button(onClick = { action("hike_vote", JSONObject().put("trail", id)) }, Modifier.fillMaxWidth()) { Text(stringResource(if (state.me?.name in votes[id].orEmpty()) R.string.miniapp_voted else R.string.vote)) }
                        TextButton(onClick = { context.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(trail.optString("link")))) }) { Icon(Icons.Rounded.OpenInNew, null); Text(stringResource(R.string.miniapp_open_trail_page)) }
                    } }
                    if (decided == id) {
                        if (data.objects("itinerary").isNotEmpty()) item { Box(Modifier.padding(horizontal = 20.dp)) { HikeItinerary(data.objects("itinerary")) } }
                        item { Column(Modifier.padding(horizontal = 20.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                            Button(onClick = {
                                val now = ZonedDateTime.now()
                                val saturday = now.with(TemporalAdjusters.nextOrSame(DayOfWeek.SATURDAY)).withHour(8).withMinute(0).withSecond(0).withNano(0)
                                val start = data.optLong("dayMs", (if (saturday <= now) saturday.plusWeeks(1) else saturday).toInstant().toEpochMilli())
                                native("calendar.add", JSONObject().put("title", "${data.optString("title")}: ${trail.optString("name")}").put("startMs", start).put("endMs", start + 7 * 3_600_000L).put("location", "${trail.optString("name")}, ${trail.optString("park")}").put("notes", data.objects("itinerary").joinToString("\n") { "${it.optString("time")} ${it.optString("text")}" })) {}
                            }, enabled = !busy) { Icon(Icons.Rounded.CalendarMonth, null); Text(stringResource(R.string.miniapp_calendar_add)) }
                            Text(stringResource(R.string.miniapp_album), style = MaterialTheme.typography.titleLarge)
                            Text(stringResource(R.string.miniapp_photos_private), style = MaterialTheme.typography.bodySmall)
                            LazyRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                                items(data.objects("album").takeLast(24)) { photo -> HikeAlbumPhoto(photo.optString("dataUrl"), stringResource(R.string.miniapp_photo_by, photo.optString("by"))) }
                                items(mine) { photo -> HikeAlbumPhoto(photo.optString("dataUrl"), stringResource(R.string.miniapp_photo_private)) }
                            }
                            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                                OutlinedButton(onClick = { native("photos.pick", JSONObject().put("max", 6 - mine.size).put("maxSide", 900)) { array -> mine = (mine + (array as JSONArray).let { (0 until it.length()).map { index -> it.getJSONObject(index) } }).take(6) } }, enabled = !busy && mine.size < 6) { Text(stringResource(R.string.miniapp_add_photos)) }
                                if (mine.isNotEmpty()) Button(onClick = { share = true }) { Text(stringResource(R.string.miniapp_share_photos, mine.size)) }
                            }
                            if (mine.isNotEmpty()) TextButton(onClick = { mine = emptyList() }) { Text(stringResource(R.string.miniapp_discard_photos)) }
                        } }
                    }
                }
            }
        }
    } }
    if (share) AlertDialog(onDismissRequest = { share = false }, title = { Text(stringResource(R.string.miniapp_share_photos, mine.size)) }, text = { Text(stringResource(R.string.miniapp_share_photos_detail, item.spaceTitle)) }, confirmButton = {
        TextButton(onClick = {
            share = false
            model.launch {
                busy = true
                try {
                    val result = model.repository.change { it.appCallTool(item.id, "hike_add_photos", JSONObject().put("photos", JSONArray(mine.map { photo -> photo.getString("dataUrl") })).toString(), false) }
                    check(result.status == xyz.tironi.zoen.core.AppCallStatus.DONE) { result.message }
                    mine = emptyList()
                    if (result.message.isNotBlank()) model.notify(result.message)
                } finally { busy = false }
            }
        }, enabled = !busy) { Text(stringResource(R.string.share)) }
    }, dismissButton = { TextButton(onClick = { share = false }) { Text(stringResource(R.string.cancel)) } })
    if (compare) Dialog(onDismissRequest = { compare = false }, properties = DialogProperties(usePlatformDefaultWidth = false)) {
        Scaffold(topBar = { ScreenBar(stringResource(R.string.miniapp_compare), { compare = false }) }) { padding ->
            val labels = listOf(R.string.miniapp_distance, R.string.miniapp_climb, R.string.miniapp_time, R.string.miniapp_difficulty, R.string.miniapp_route, R.string.miniapp_terrain, R.string.miniapp_shade, R.string.miniapp_best_for)
            LazyColumn(Modifier.fillMaxSize().padding(padding), contentPadding = PaddingValues(16.dp)) {
                item { Row(Modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                    Column(Modifier.width(110.dp)) { Spacer(Modifier.height(150.dp)); labels.forEach { Text(stringResource(it), Modifier.height(64.dp), style = MaterialTheme.typography.labelLarge) } }
                    trails.forEach { trail -> Column(Modifier.width(160.dp)) {
                        Image(painterResource(trailPhoto(trail.optString("id"))), null, Modifier.fillMaxWidth().height(100.dp).clip(MaterialTheme.shapes.medium), contentScale = ContentScale.Crop)
                        Text(trail.optString("name"), Modifier.height(50.dp), style = MaterialTheme.typography.titleMedium)
                        listOf("${trail.optDouble("miles")} mi", "${trail.optInt("climbFt")} ft", trail.optString("time"), trail.optString("difficulty"), trail.optString("route"), trail.optString("terrain"), trail.optString("shade"), trail.optString("bestFor")).forEach { Text(it, Modifier.height(64.dp), style = MaterialTheme.typography.bodyMedium) }
                        Text(votes[trail.optString("id")].orEmpty().joinToString(), style = MaterialTheme.typography.bodySmall)
                        if (decided == null) Button(onClick = { action("hike_vote", JSONObject().put("trail", trail.optString("id"))) }) { Text(stringResource(R.string.vote)) }
                        else if (decided == trail.optString("id")) Text(stringResource(R.string.miniapp_going), color = MaterialTheme.colorScheme.primary)
                    } }
                } }
            }
        }
    }
}

@Composable private fun HikeStat(value: String, label: String) { Column { Text(value, style = MaterialTheme.typography.titleLarge); Text(label, style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurfaceVariant) } }
@Composable private fun HikeItinerary(rows: List<JSONObject>) {
    Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
        Text(stringResource(R.string.miniapp_itinerary), style = MaterialTheme.typography.titleLarge)
        rows.forEach { row -> Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) { Text(row.optString("time"), Modifier.width(52.dp), style = MaterialTheme.typography.labelLarge); Text(row.optString("text"), Modifier.weight(1f)) } }
    }
}

@Composable private fun HikeRoute(trail: JSONObject, modifier: Modifier) {
    val label = stringResource(R.string.miniapp_route_preview, trail.optString("name"))
    val line = trail.getJSONArray("line").let { array -> (0 until array.length()).map { Offset(array.getJSONArray(it).getDouble(0).toFloat(), array.getJSONArray(it).getDouble(1).toFloat()) } }
    val minX = line.minOf { it.x }; val maxX = line.maxOf { it.x }; val minY = line.minOf { it.y }; val maxY = line.maxOf { it.y }
    val background = MaterialTheme.colorScheme.secondaryContainer
    val ink = MaterialTheme.colorScheme.primary
    val casing = MaterialTheme.colorScheme.surface
    Column {
        Canvas(modifier.clip(MaterialTheme.shapes.large).semantics { contentDescription = label }) {
            drawRect(background)
            val points = line.map { Offset(28.dp.toPx() + (it.x - minX) / (maxX - minX) * (size.width - 56.dp.toPx()), 28.dp.toPx() + (maxY - it.y) / (maxY - minY) * (size.height - 56.dp.toPx())) }
            val path = Path().apply { points.forEachIndexed { i, p -> if (i == 0) moveTo(p.x, p.y) else lineTo(p.x, p.y) } }
            drawPath(path, casing, style = Stroke(9.dp.toPx(), cap = androidx.compose.ui.graphics.StrokeCap.Round, join = androidx.compose.ui.graphics.StrokeJoin.Round))
            drawPath(path, ink, style = Stroke(4.dp.toPx(), cap = androidx.compose.ui.graphics.StrokeCap.Round, join = androidx.compose.ui.graphics.StrokeJoin.Round))
            drawCircle(ink, 8.dp.toPx(), points.first()); drawCircle(casing, 3.dp.toPx(), points.first())
        }
        Text(label, style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
    }
}

@Composable private fun HikeAlbumPhoto(url: String, description: String) {
    val bitmap by produceState<android.graphics.Bitmap?>(null, url) { value = withContext(Dispatchers.IO) { runCatching {
        require(url.startsWith("data:image/jpeg;base64,") && url.length <= 150_000)
        val bytes = Base64.decode(url.substringAfter(','), Base64.DEFAULT)
        val options = BitmapFactory.Options().apply { inJustDecodeBounds = true }
        BitmapFactory.decodeByteArray(bytes, 0, bytes.size, options)
        require(options.outWidth in 1..4_096 && options.outHeight in 1..4_096)
        options.inJustDecodeBounds = false; options.inSampleSize = (maxOf(options.outWidth, options.outHeight) / 512).coerceAtLeast(1)
        BitmapFactory.decodeByteArray(bytes, 0, bytes.size, options)
    }.getOrNull() } }
    bitmap?.let { Image(it.asImageBitmap(), description, Modifier.size(112.dp).clip(MaterialTheme.shapes.medium), contentScale = ContentScale.Crop) }
}

private fun distanceMiles(here: JSONObject, trail: JSONObject): Double {
    val head = trail.getJSONArray("trailhead")
    val lat = Math.toRadians(here.getDouble("lat")); val lon = Math.toRadians(here.getDouble("lon"))
    val targetLat = Math.toRadians(head.getDouble(1)); val targetLon = Math.toRadians(head.getDouble(0))
    val h = sin((targetLat - lat) / 2).pow(2) + cos(lat) * cos(targetLat) * sin((targetLon - lon) / 2).pow(2)
    return 7_917.6 * asin(sqrt(h.coerceIn(0.0, 1.0)))
}
