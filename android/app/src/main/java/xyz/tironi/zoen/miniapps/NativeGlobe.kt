package xyz.tironi.zoen.miniapps

import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.gestures.detectDragGestures
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Text
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.PathEffect
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.drawscope.clipPath
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.CustomAccessibilityAction
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.customActions
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.LocalLifecycleOwner
import xyz.tironi.zoen.ui.rememberMotionEnabled
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.double
import xyz.tironi.zoen.R
import kotlin.math.*

internal data class GlobePoint(val lat: Double, val lon: Double)
internal data class GlobeProjection(val x: Double, val y: Double, val z: Double)

internal object OrthographicGlobe {
    private const val RADIANS = PI / 180
    fun longitude(lon: Double): Double = ((lon + 180) % 360 + 360) % 360 - 180

    fun project(point: GlobePoint, center: GlobePoint): GlobeProjection {
        val lat = point.lat * RADIANS; val delta = (point.lon - center.lon) * RADIANS
        val lat0 = center.lat * RADIANS
        return GlobeProjection(cos(lat) * sin(delta), cos(lat0) * sin(lat) - sin(lat0) * cos(lat) * cos(delta),
            sin(lat0) * sin(lat) + cos(lat0) * cos(lat) * cos(delta))
    }

    fun inverse(x: Double, y: Double, center: GlobePoint): GlobePoint? {
        if (!x.isFinite() || !y.isFinite()) return null
        val rho = hypot(x, y)
        if (rho > 1) return null
        if (rho < 1e-9) return center
        val angle = asin(rho); val lat0 = center.lat * RADIANS
        val lat = asin((cos(angle) * sin(lat0) + y * sin(angle) * cos(lat0) / rho).coerceIn(-1.0, 1.0))
        val lon = center.lon * RADIANS + atan2(x * sin(angle), rho * cos(angle) * cos(lat0) - y * sin(angle) * sin(lat0))
        return GlobePoint(lat / RADIANS, longitude(lon / RADIANS))
    }

    fun slerp(a: GlobePoint, b: GlobePoint, progress: Double): GlobePoint {
        val t = progress.coerceIn(0.0, 1.0)
        if (t == 0.0) return a
        if (t == 1.0) return b
        fun vector(point: GlobePoint): DoubleArray {
            val lat = point.lat * RADIANS; val lon = point.lon * RADIANS
            return doubleArrayOf(cos(lat) * cos(lon), cos(lat) * sin(lon), sin(lat))
        }
        val av = vector(a); val bv = vector(b)
        val dot = av.indices.sumOf { av[it] * bv[it] }.coerceIn(-1.0, 1.0)
        val angle = acos(dot)
        if (angle < 1e-9) return a
        var tangent = DoubleArray(3) { bv[it] - dot * av[it] }
        var length = sqrt(tangent.sumOf { it * it })
        if (length < 1e-9) {
            val axis = av.indices.minBy { abs(av[it]) }
            tangent = DoubleArray(3) { if (it == axis) 1.0 - av[axis] * av[it] else -av[axis] * av[it] }
            length = sqrt(tangent.sumOf { it * it })
        }
        val value = DoubleArray(3) { cos(angle * t) * av[it] + sin(angle * t) * tangent[it] / length }
        return GlobePoint(atan2(value[2], hypot(value[0], value[1])) / RADIANS, atan2(value[1], value[0]) / RADIANS)
    }

    fun land(text: String): List<List<GlobePoint>> = Json.parseToJsonElement(text).jsonArray.map { polygon ->
        val numbers = polygon.jsonArray
        require(numbers.size >= 6 && numbers.size % 2 == 0)
        numbers.indices.step(2).map { index ->
            val lon = numbers[index].jsonPrimitive.double; val lat = numbers[index + 1].jsonPrimitive.double
            require(lon.isFinite() && lat.isFinite() && lon in -180.0..180.0 && lat in -90.0..90.0)
            GlobePoint(lat, lon)
        }
    }
}

/** Offset values use x=longitude and y=latitude, in degrees. */
@Composable
fun NativeGlobe(guess: Offset? = null, actual: Offset? = null, modifier: Modifier = Modifier,
                interactive: Boolean = true, spin: Boolean = false, onGuess: (lat: Float, lon: Float) -> Unit = { _, _ -> }) {
    val context = LocalContext.current
    val haptics = LocalHapticFeedback.current
    val choose by rememberUpdatedState(onGuess)
    var latitude by rememberSaveable { mutableFloatStateOf(25f) }
    var longitude by rememberSaveable { mutableFloatStateOf(10f) }
    val lifecycle by LocalLifecycleOwner.current.lifecycle.currentStateFlow.collectAsState()
    val motion = rememberMotionEnabled()
    val rotation = if (spin && motion && lifecycle.isAtLeast(Lifecycle.State.RESUMED)) {
        val turn by rememberInfiniteTransition(label = "snapshot globe").animateFloat(0f, 360f,
            infiniteRepeatable(tween(120_000, easing = LinearEasing)), label = "longitude")
        turn
    } else 0f
    val center = GlobePoint(latitude.toDouble(), OrthographicGlobe.longitude((longitude + rotation).toDouble()))
    val contours by produceState<List<List<GlobePoint>>?>(null, context.applicationContext) {
        value = try { withContext(Dispatchers.IO) { context.assets.open("world/land110.json").bufferedReader().use { OrthographicGlobe.land(it.readText()) } } }
        catch (e: Exception) { if (e is CancellationException) throw e; emptyList() }
    }
    val selected = guess?.takeIf { it.x.isFinite() && it.y.isFinite() }?.let { GlobePoint(it.y.coerceIn(-90f, 90f).toDouble(), OrthographicGlobe.longitude(it.x.toDouble())) }
    val answer = actual?.takeIf { it.x.isFinite() && it.y.isFinite() }?.let { GlobePoint(it.y.coerceIn(-90f, 90f).toDouble(), OrthographicGlobe.longitude(it.x.toDouble())) }
    val arc = remember { Animatable(0f) }
    LaunchedEffect(answer, motion) {
        if (answer == null) { arc.snapTo(0f); return@LaunchedEffect }
        val target = OrthographicGlobe.slerp(selected ?: answer, answer, .5)
        if (!motion) {
            latitude = target.lat.toFloat()
            longitude = OrthographicGlobe.longitude(target.lon).toFloat()
            arc.snapTo(1f)
            return@LaunchedEffect
        }
        val fromLat = latitude; val fromLon = longitude
        val delta = OrthographicGlobe.longitude(target.lon - fromLon).toFloat()
        coroutineScope {
            launch {
                Animatable(0f).animateTo(1f, tween(1000)) {
                    latitude = fromLat + (target.lat.toFloat() - fromLat) * value
                    longitude = OrthographicGlobe.longitude((fromLon + delta * value).toDouble()).toFloat()
                }
            }
            launch { arc.snapTo(0f); delay(200); arc.animateTo(1f, tween(1100)) }
        }
    }
    val description = stringResource(R.string.globe_description)
    val position = stringResource(R.string.globe_center, latitude, longitude)
    val choice = selected?.let { stringResource(R.string.globe_guess, it.lat, it.lon) }.orEmpty()
    val revealed = answer?.let { stringResource(R.string.globe_answer, it.lat, it.lon) }.orEmpty()
    val west = stringResource(R.string.globe_west); val east = stringResource(R.string.globe_east)
    val north = stringResource(R.string.globe_north); val south = stringResource(R.string.globe_south)
    val selectCenter = stringResource(R.string.globe_select_center)
    val canChoose = interactive && answer == null && !contours.isNullOrEmpty()
    Box(modifier.fillMaxWidth().aspectRatio(1f), contentAlignment = Alignment.Center) {
        Canvas(Modifier.matchParentSize().testTag("native-globe").semantics {
            contentDescription = description
            stateDescription = listOf(position, choice, revealed).filter { it.isNotBlank() }.joinToString(". ")
            customActions = if (canChoose) listOf(
                CustomAccessibilityAction(west) { longitude = OrthographicGlobe.longitude((longitude - 30f).toDouble()).toFloat(); true },
                CustomAccessibilityAction(east) { longitude = OrthographicGlobe.longitude((longitude + 30f).toDouble()).toFloat(); true },
                CustomAccessibilityAction(north) { latitude = (latitude + 20).coerceAtMost(80f); true },
                CustomAccessibilityAction(south) { latitude = (latitude - 20).coerceAtLeast(-80f); true },
                CustomAccessibilityAction(selectCenter) { choose(latitude, longitude); true }
            ) else emptyList()
        }.pointerInput(canChoose) {
            if (canChoose) detectDragGestures { change, pan ->
                change.consume()
                val side = min(size.width, size.height).coerceAtLeast(1)
                latitude = (latitude + pan.y / side * 140).coerceIn(-80f, 80f)
                longitude = OrthographicGlobe.longitude((longitude - pan.x / side * 160).toDouble()).toFloat()
            }
        }.pointerInput(canChoose) {
            if (canChoose) detectTapGestures { point ->
                val radius = min(size.width, size.height) * .47
                OrthographicGlobe.inverse((point.x - size.width / 2.0) / radius, -(point.y - size.height / 2.0) / radius,
                    GlobePoint(latitude.toDouble(), longitude.toDouble()))?.let {
                    choose(it.lat.toFloat(), it.lon.toFloat())
                    haptics.performHapticFeedback(HapticFeedbackType.TextHandleMove)
                }
            }
        }) { drawGlobe(contours.orEmpty(), center, selected, answer, arc.value) }
        if (contours == null) CircularProgressIndicator(color = Color.White)
        else if (contours!!.isEmpty()) Text(stringResource(R.string.globe_error), color = Color.White)
    }
}

private fun DrawScope.drawGlobe(contours: List<List<GlobePoint>>, center: GlobePoint, guess: GlobePoint?, actual: GlobePoint?, arc: Float) {
    val radius = size.minDimension * .47f
    val middle = Offset(size.width / 2, size.height / 2)
    val disc = Path().apply { addOval(Rect(middle.x - radius, middle.y - radius, middle.x + radius, middle.y + radius)) }
    drawCircle(Brush.radialGradient(listOf(Color(0x004AA3FF), Color(0x594AA3FF), Color.Transparent), center = middle, radius = radius * 1.08f), radius * 1.08f, middle)
    drawCircle(Brush.radialGradient(listOf(Color(0xFF1D4E8F), Color(0xFF0B2752), Color(0xFF050F24)), center = middle - Offset(radius * .35f, radius * .4f), radius = radius * 1.5f), radius, middle)
    fun screen(point: GlobePoint, horizon: Boolean = false): Pair<Offset, Double> {
        val projection = OrthographicGlobe.project(point, center)
        val scale = if (horizon && projection.z < 0) 1.0 / hypot(projection.x, projection.y).coerceAtLeast(1e-9) else 1.0
        return Offset(middle.x + (radius * projection.x * scale).toFloat(), middle.y - (radius * projection.y * scale).toFloat()) to projection.z
    }
    clipPath(disc) {
        val land = Path()
        for (polygon in contours) {
            var front = false
            val boundary = Path()
            polygon.forEachIndexed { index, point ->
                val (at, z) = screen(point, horizon = true)
                if (z > 0) front = true
                if (index == 0) boundary.moveTo(at.x, at.y) else boundary.lineTo(at.x, at.y)
            }
            if (front) { boundary.close(); land.addPath(boundary) }
        }
        drawPath(land, Brush.linearGradient(listOf(Color(0xFF9DB07A), Color(0xFFC9B98E), Color(0xFFD9C8A0)), start = middle - Offset(0f, radius), end = middle + Offset(0f, radius)))
        drawPath(land, Color(0x998FA36B), style = Stroke(.6f * density))
        val grid = Path()
        for (lat in -60..60 step 30) {
            var started = false
            for (lon in -180..180 step 6) {
                val (at, z) = screen(GlobePoint(lat.toDouble(), lon.toDouble()))
                if (z > 0) { if (started) grid.lineTo(at.x, at.y) else grid.moveTo(at.x, at.y); started = true }
                else started = false
            }
        }
        drawPath(grid, Color.White.copy(alpha = .07f), style = Stroke(.5f * density))
        drawCircle(Brush.radialGradient(listOf(Color.Transparent, Color.Transparent, Color.Black.copy(alpha = .55f)), center = middle - Offset(radius * .3f, radius * .35f), radius = radius * 1.45f), radius, middle)
        drawCircle(Brush.radialGradient(listOf(Color.White.copy(alpha = .16f), Color.Transparent), center = middle - Offset(radius * .42f, radius * .45f), radius = radius * .7f), radius, middle)
        if (guess != null && actual != null) {
            val path = Path()
            var started = false
            for (step in 0..(64 * arc.coerceIn(0f, 1f)).toInt()) {
                val (at, z) = screen(OrthographicGlobe.slerp(guess, actual, step / 64.0))
                if (z > 0) { if (started) path.lineTo(at.x, at.y) else path.moveTo(at.x, at.y); started = true }
                else started = false
            }
            drawPath(path, Color.White.copy(alpha = .85f), style = Stroke(1.5f * density, pathEffect = PathEffect.dashPathEffect(floatArrayOf(4 * density, 4 * density))))
        }
        actual?.let { val (at, z) = screen(it); if (z > 0) {
            drawCircle(Color(0xFFFFD60A), 6 * density, at)
            drawCircle(Color.Black.copy(alpha = .4f), 6 * density, at, style = Stroke(density))
        } }
        guess?.let { val (at, z) = screen(it); if (z > 0) {
            drawCircle(Color(0xFF5CE1E6).copy(alpha = .35f), 14 * density, at, style = Stroke(1.5f * density))
            drawCircle(Brush.radialGradient(listOf(Color(0xFF7FF7FF), Color(0x4D18C8D4)), at, 9 * density), 9 * density, at)
            drawCircle(Color.White, 11 * density, at, style = Stroke(2.5f * density))
        } }
    }
    drawCircle(Color.White.copy(alpha = .15f), radius, middle, style = Stroke(density))
}
