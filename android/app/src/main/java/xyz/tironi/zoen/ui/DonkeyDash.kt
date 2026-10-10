package xyz.tironi.zoen.ui

import android.graphics.Paint
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.Saver
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.drawscope.drawIntoCanvas
import androidx.compose.ui.graphics.nativeCanvas
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.unit.dp
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
import androidx.lifecycle.compose.LocalLifecycleOwner
import kotlinx.serialization.json.Json
import xyz.tironi.zoen.R

private val dashSaver = Saver<DashState, String>(
    save = { Json.encodeToString(it.copy(last = null)) }, restore = { Json.decodeFromString<DashState>(it) })

@Composable
fun DonkeyDash(onClose: () -> Unit, onScore: (Int, Int) -> Unit) {
    var game by rememberSaveable(stateSaver = dashSaver) { mutableStateOf(DashState()) }
    var width by remember { mutableDoubleStateOf(320.0) }
    val density = LocalDensity.current.density
    val haptics = rememberZoenHaptics()
    val score by rememberUpdatedState(onScore)
    val owner = LocalLifecycleOwner.current
    DisposableEffect(owner) {
        val observer = LifecycleEventObserver { _, event -> if (event == Lifecycle.Event.ON_STOP) game = game.pause() }
        owner.lifecycle.addObserver(observer)
        onDispose { owner.lifecycle.removeObserver(observer) }
    }
    LaunchedEffect(game.phase) {
        while (game.phase == DashState.Phase.Running) {
            val now = withFrameNanos { it } / 1_000_000_000.0
            val step = game.step(now, width)
            game = step.state
            repeat(step.collected) { haptics.perform(ZoenFeedback.Tap) }
            if (step.finished) { haptics.perform(ZoenFeedback.Dismiss); score(game.distance.toInt(), game.carrots) }
        }
    }
    val jump: () -> Unit = {
        val before = game
        game = game.jump()
        if (before.phase == DashState.Phase.Running && game != before) haptics.perform(ZoenFeedback.Selection)
    }
    val description = stringResource(R.string.dash)
    val meters = stringResource(R.string.score, game.distance.toInt())
    val carrots = stringResource(R.string.dash_carrots, game.carrots)
    val textPaint = remember { Paint(Paint.ANTI_ALIAS_FLAG).apply { typeface = android.graphics.Typeface.MONOSPACE } }
    Column(Modifier.verticalScroll(rememberScrollState()).padding(24.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
        Text(description, style = MaterialTheme.typography.headlineMedium)
        Text(stringResource(R.string.dash_hint), color = MaterialTheme.colorScheme.onSurfaceVariant)
        Text(meters, Modifier.testTag("dash-distance"), style = MaterialTheme.typography.titleLarge)
        Text(carrots, style = MaterialTheme.typography.titleMedium)
        Canvas(Modifier.widthIn(max = 480.dp).fillMaxWidth().aspectRatio(320f / 220f)
            .testTag("dash-canvas").onSizeChanged { width = it.width / density.toDouble() }
            .semantics { contentDescription = description; stateDescription = "$meters. $carrots" }
            .pointerInput(game.phase) { detectTapGestures { jump() } }) {
            val k = density
            val w = size.width / k; val h = size.height / k; val ground = h * .66f
            fun rect(x: Double, y: Double, width: Double, height: Double, color: Color) =
                drawRect(color, Offset((x * k).toFloat(), (y * k).toFloat()), Size((width * k).toFloat(), (height * k).toFloat()))
            drawRect(Brush.verticalGradient(listOf(Color(0xFFB9C7EE), Color(0xFFE9DCE9), Color(0xFFF6E7D2)), endY = ground * k))
            drawCircle(Brush.radialGradient(listOf(Color.White, Color.Transparent), Offset(w * .55f * k + 27 * k, h * .18f * k + 27 * k), 40 * k), 40 * k,
                Offset(w * .55f * k + 27 * k, h * .18f * k + 27 * k))
            val off = game.distance * 3 % 200
            for (i in -1..(w / 100).toInt() + 2) rect(i * 100 - off / 2, ground - 16.0 - i * 37 % 13, 34.0, 40.0, Color(0x809AA0A8))
            drawRect(Brush.verticalGradient(listOf(Color(0xFFE7D3B6), Color(0xFFF2DFCD)), startY = ground * k), Offset(0f, ground * k), Size(size.width, size.height - ground * k))
            rect(0.0, ground.toDouble(), w.toDouble(), 22.0, Color(0xFF4A4A4E))
            val dash = (game.distance * 8).toInt() % 28
            for (i in 0..(w / 28).toInt() + 1) rect(i * 28.0 - dash, ground + 10.0, 14.0, 2.0, Color.White.copy(alpha = .55f))
            rect(0.0, ground.toDouble(), w.toDouble(), 3.0, Color(0xFF2E2E32))
            for (i in 0..(w / 40).toInt() + 1) rect(i * 40 - game.distance * 18 % 40, ground + 14.0, 12.0, 3.0, Color(0xFF5E5E63))
            val green = Color(0xFF3E8E5A)
            game.obstacles.forEach { x ->
                rect(x, ground - 30.0, 10.0, 30.0, green); rect(x - 6, ground - 22.0, 6.0, 4.0, green)
                rect(x - 6, ground - 28.0, 4.0, 8.0, green); rect(x + 10, ground - 18.0, 6.0, 4.0, green)
                rect(x + 12, ground - 24.0, 4.0, 8.0, green)
            }
            game.food.forEach { food ->
                val y = ground - 22 - food.height
                rect(food.x + 3, y, 4.0, 4.0, green); rect(food.x + 1, y + 4, 8.0, 5.0, Color(0xFFF28C28))
                rect(food.x + 3, y + 9, 4.0, 5.0, Color(0xFFF28C28))
            }
            val now = game.last ?: 0.0
            drawIntoCanvas { drawing ->
                textPaint.textSize = 16 * k
                game.plusOnes.forEach { plus ->
                    val age = now - plus.time
                    textPaint.color = android.graphics.Color.rgb(242, 140, 40)
                    textPaint.alpha = ((1 - age / .8) * 255).toInt().coerceIn(0, 255)
                    drawing.nativeCanvas.drawText("+1", ((plus.x + 6) * k).toFloat(), ((ground - 40 - plus.height - age * 40) * k).toFloat(), textPaint)
                }
            }
            val walking = game.phase == DashState.Phase.Running && game.y == 0.0 && (now * 10).toInt() % 2 == 0
            drawPixelDonkey(false, walking = walking) { x, y, bw, bh, color ->
                rect(36 + x * 2.0, ground - 44 - game.y + 2 + y * 2, bw * 2.0 + .3, bh * 2.0 + .3, Color(color))
            }
        }
        if (game.toast != null && (game.last ?: 0.0) < game.toastUntil) Text(stringResource(
            if (game.toast == DashState.Toast.Carrots) R.string.dash_carrot_toast else R.string.dash_jump_toast, game.carrots))
        if (game.phase == DashState.Phase.Ready || game.phase == DashState.Phase.Over) {
            Button(jump, Modifier.fillMaxWidth().testTag("dash-start")) { Text(stringResource(if (game.phase == DashState.Phase.Over) R.string.retry else R.string.start)) }
        } else {
            Button(jump, Modifier.fillMaxWidth().testTag("dash-jump"), enabled = game.phase == DashState.Phase.Running) { Text(stringResource(R.string.jump)) }
            OutlinedButton(onClick = { game = if (game.phase == DashState.Phase.Paused) game.resume() else game.pause() },
                Modifier.fillMaxWidth().testTag(if (game.phase == DashState.Phase.Paused) "dash-continue" else "dash-pause")) {
                Text(stringResource(if (game.phase == DashState.Phase.Paused) R.string.miniapp_continue else R.string.miniapp_pause))
            }
        }
        TextButton(onClose) { Text(stringResource(R.string.close)) }
    }
}
