package xyz.tironi.zoen.ui

import android.graphics.Paint
import android.graphics.Path
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.foundation.layout.size
import androidx.compose.material3.IconButton
import androidx.compose.material3.LocalContentColor
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.drawscope.drawIntoCanvas
import androidx.compose.ui.graphics.nativeCanvas
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.platform.LocalLayoutDirection
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.LayoutDirection
import androidx.compose.ui.unit.dp
import xyz.tironi.zoen.ui.ink.*

@Composable
internal fun InkIconButton(glyph: ZoenGlyph, description: String, onClick: () -> Unit, modifier: Modifier = Modifier, enabled: Boolean = true) {
    val interactions = remember { MutableInteractionSource() }
    IconButton(onClick, modifier, enabled, interactionSource = interactions) {
        ZoenIcon(glyph, description, interactions = interactions)
    }
}

@Composable
internal fun ZoenIcon(glyph: ZoenGlyph, description: String? = null, modifier: Modifier = Modifier, selected: Boolean = false,
                      interactions: MutableInteractionSource? = null, tint: Color = LocalContentColor.current) {
    val art = ZoenIconArt.all.getValue(glyph)
    val motion = rememberMotionEnabled()
    val pressed = interactions?.collectIsPressedAsState()?.value ?: false
    val time = remember(glyph) { Animatable(660f) }
    val draw = remember(glyph) { Animatable(1f) }
    var wasSelected by remember { mutableStateOf(selected) }
    LaunchedEffect(pressed, selected, motion) {
        val select = selected && !wasSelected
        wasSelected = selected
        if (!motion) time.snapTo(660f)
        else if (pressed || select) { time.snapTo(0f); time.animateTo(660f, tween(660, easing = LinearEasing)) }
        else if (time.value < 660f) time.animateTo(660f, tween((660 - time.value).toInt().coerceAtLeast(1), easing = LinearEasing))
    }
    var initial by remember { mutableStateOf(true) }
    LaunchedEffect(selected, motion) {
        if (selected && motion && !initial) { draw.snapTo(0f); draw.animateTo(1f, tween(260)) } else draw.snapTo(1f)
        initial = false
    }
    val frame = if (motion && time.value < 660f) (time.value / 100).toInt() % 4 + 1 else 0
    val paths = remember(glyph, selected, frame, draw.value) {
        IconLayer.entries.associateWith { layer ->
            InkIconGeometry.paths(art, layer, 1.75 * if (selected) 1.08 else 1.0, draw.value.toDouble(), frame).map { points ->
                Path().apply { moveTo(points.first().x.toFloat(), points.first().y.toFloat()); points.drop(1).forEach { lineTo(it.x.toFloat(), it.y.toFloat()) }; close() }
            }
        }
    }
    val paint = remember { Paint(Paint.ANTI_ALIAS_FLAG) }
    val mirror = LocalLayoutDirection.current == LayoutDirection.Rtl && glyph in listOf(ZoenGlyph.Back, ZoenGlyph.Send)
    Canvas(modifier.size(24.dp).then(if (description != null) Modifier.semantics { contentDescription = description } else Modifier)) {
        val beat = InkIconGeometry.motion(art, time.value.toDouble())
        drawIntoCanvas { drawing ->
            val canvas = drawing.nativeCanvas
            val saved = canvas.save()
            val k = minOf(size.width, size.height) / 24f
            canvas.translate((size.width - k * 24) / 2, (size.height - k * 24) / 2); canvas.scale(k, k)
            if (mirror) { canvas.translate(24f, 0f); canvas.scale(-1f, 1f) }
            canvas.rotate(beat.rot.toFloat(), 12f, 12f); canvas.scale(beat.scale.toFloat(), beat.scale.toFloat(), 12f, 12f)
            for (layer in listOf(IconLayer.Wash, IconLayer.Base, IconLayer.Accent)) {
                if (layer == IconLayer.Wash && !selected) continue
                val layerSaved = canvas.save()
                if (layer == IconLayer.Accent) {
                    canvas.translate(beat.accentX.toFloat(), beat.accentY.toFloat())
                    canvas.rotate(beat.accentRot.toFloat(), art.pivot.x.toFloat(), art.pivot.y.toFloat())
                    canvas.scale(beat.accentScale.toFloat(), beat.accentScale.toFloat(), art.pivot.x.toFloat(), art.pivot.y.toFloat())
                }
                paint.color = tint.copy(alpha = tint.alpha * when (layer) { IconLayer.Wash -> .22f * draw.value; IconLayer.Accent -> beat.accentOpacity.toFloat(); else -> 1f }).toArgb()
                paths.getValue(layer).forEach { canvas.drawPath(it, paint) }
                canvas.restoreToCount(layerSaved)
            }
            canvas.restoreToCount(saved)
        }
    }
}
