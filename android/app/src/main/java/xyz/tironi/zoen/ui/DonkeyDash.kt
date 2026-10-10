package xyz.tironi.zoen.ui

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.layout.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
import androidx.lifecycle.compose.LocalLifecycleOwner
import kotlin.math.abs
import xyz.tironi.zoen.R

@Composable
fun DonkeyDash(onClose: () -> Unit, onScore: (Int, Int) -> Unit) {
    var running by remember { mutableStateOf(false) }
    var paused by remember { mutableStateOf(false) }
    var ended by remember { mutableStateOf(false) }
    var elapsed by remember { mutableFloatStateOf(0f) }
    var height by remember { mutableFloatStateOf(0f) }
    var velocity by remember { mutableFloatStateOf(0f) }
    var carrots by remember { mutableIntStateOf(0) }
    var collectedAt by remember { mutableIntStateOf(-1) }
    val owner = LocalLifecycleOwner.current
    DisposableEffect(owner) {
        val observer = LifecycleEventObserver { _, event -> if (event == Lifecycle.Event.ON_STOP && running) paused = true }
        owner.lifecycle.addObserver(observer)
        onDispose { owner.lifecycle.removeObserver(observer) }
    }
    LaunchedEffect(running, paused) {
        if (!running || paused) return@LaunchedEffect
        var last = withFrameNanos { it }
        while (running && !paused) {
            val now = withFrameNanos { it }
            val dt = ((now - last) / 1_000_000_000f).coerceIn(0f, .04f)
            last = now
            elapsed += dt
            velocity -= 650 * dt
            height = (height + velocity * dt).coerceAtLeast(0f)
            val rockX = 300 - (elapsed * 140 % 350)
            val cycle = (elapsed * 140 / 350).toInt()
            if (abs(rockX - 65) < 18 && height < 26) {
                running = false; ended = true; onScore((elapsed * 25).toInt(), carrots)
            } else if (abs(rockX - 115) < 15 && height > 35 && collectedAt != cycle) { carrots++; collectedAt = cycle }
        }
    }
    val jump: () -> Unit = { if (running && !paused && height == 0f) velocity = 310f }
    Column(Modifier.verticalScroll(rememberScrollState()).padding(24.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
        Text(stringResource(R.string.dash), style = MaterialTheme.typography.headlineMedium)
        Text(stringResource(R.string.dash_hint), color = MaterialTheme.colorScheme.onSurfaceVariant)
        Text(stringResource(R.string.score, (elapsed * 25).toInt()), style = MaterialTheme.typography.titleLarge)
        val backdrop = MaterialTheme.colorScheme.primaryContainer
        Canvas(Modifier.widthIn(max = 480.dp).fillMaxWidth().aspectRatio(320f / 220f).pointerInput(running) { detectTapGestures { jump() } }) {
            val scale = size.width / 320
            drawRoundRect(backdrop)
            val floor = size.height - 28 * scale
            drawLine(Color(0xFF6D8158), Offset(0f, floor), Offset(size.width, floor), 3 * scale)
            val donkey = Offset(48 * scale, floor - (height + 30) * scale)
            drawRect(Color(0xFF9A8D83), donkey, Size(36 * scale, 24 * scale))
            drawRect(Color(0xFF6B584D), donkey + Offset(26 * scale, -13 * scale), Size(13 * scale, 25 * scale))
            drawRect(Color(0xFF6B584D), donkey + Offset(29 * scale, -23 * scale), Size(4 * scale, 15 * scale))
            val rock = (300 - elapsed * 140 % 350) * scale
            drawRect(Color(0xFF6D7766), Offset(rock, floor - 24 * scale), Size(22 * scale, 24 * scale))
            drawOval(Color(0xFFE9A03B), Offset(rock - 45 * scale, floor - 70 * scale), Size(10 * scale, 20 * scale))
        }
        if (!running) Button(onClick = { elapsed = 0f; height = 0f; velocity = 0f; carrots = 0; collectedAt = -1; ended = false; paused = false; running = true }, Modifier.fillMaxWidth()) { Text(stringResource(if (ended) R.string.retry else R.string.start)) }
        else {
            Button(onClick = jump, Modifier.fillMaxWidth(), enabled = !paused) { Text(stringResource(R.string.jump)) }
            OutlinedButton(onClick = { paused = !paused }, Modifier.fillMaxWidth()) { Text(stringResource(if (paused) R.string.miniapp_continue else R.string.miniapp_pause)) }
        }
        TextButton(onClick = onClose) { Text(stringResource(R.string.close)) }
    }
}
