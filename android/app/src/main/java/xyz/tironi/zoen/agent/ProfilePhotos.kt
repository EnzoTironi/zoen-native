package xyz.tironi.zoen.agent

import android.content.Context
import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.ImageDecoder
import android.graphics.Paint
import android.graphics.RectF
import android.net.Uri
import androidx.compose.foundation.Image
import androidx.compose.foundation.gestures.detectTransformGestures
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import java.io.ByteArrayOutputStream
import java.nio.ByteBuffer
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import xyz.tironi.zoen.R

object ProfilePhotos {
    suspend fun read(context: Context, uri: Uri): Bitmap = withContext(Dispatchers.IO) {
        val bytes = context.contentResolver.openInputStream(uri)?.use { input ->
            val output = ByteArrayOutputStream()
            val buffer = ByteArray(8192)
            while (true) {
                val count = input.read(buffer)
                if (count < 0) break
                require(output.size() + count <= 20 * 1024 * 1024) { context.getString(R.string.agent_photo_max) }
                output.write(buffer, 0, count)
            }
            output.toByteArray()
        } ?: error(context.getString(R.string.agent_photo_error))
        try {
            ImageDecoder.decodeBitmap(ImageDecoder.createSource(ByteBuffer.wrap(bytes))) { decoder, info, _ ->
                decoder.allocator = ImageDecoder.ALLOCATOR_SOFTWARE
                val scale = 2048f / maxOf(info.size.width, info.size.height).coerceAtLeast(2048)
                decoder.setTargetSize((info.size.width * scale).toInt().coerceAtLeast(1), (info.size.height * scale).toInt().coerceAtLeast(1))
            }
        } finally { bytes.fill(0) }
    }

    fun crop(source: Bitmap, zoom: Float, x: Float, y: Float, size: Int): Bitmap {
        val target = Bitmap.createBitmap(size, size, Bitmap.Config.ARGB_8888)
        val canvas = Canvas(target)
        val scale = size.toFloat() / minOf(source.width, source.height) * zoom.coerceIn(1f, 4f)
        val width = source.width * scale; val height = source.height * scale
        val left = (size - width) / 2f - x.coerceIn(-1f, 1f) * (width - size) / 2f
        val top = (size - height) / 2f - y.coerceIn(-1f, 1f) * (height - size) / 2f
        canvas.drawBitmap(source, null, RectF(left, top, left + width, top + height), Paint(Paint.ANTI_ALIAS_FLAG or Paint.FILTER_BITMAP_FLAG))
        return target
    }

    suspend fun jpeg(source: Bitmap, zoom: Float, x: Float, y: Float): ByteArray = withContext(Dispatchers.Default) {
        val cropped = crop(source, zoom, x, y, 1024)
        try { ByteArrayOutputStream().use { output -> cropped.compress(Bitmap.CompressFormat.JPEG, 90, output); output.toByteArray() } }
        finally { cropped.recycle() }
    }
}

@Composable
fun ProfilePhotoCrop(source: Bitmap, apply: (Float, Float, Float) -> Unit, cancel: () -> Unit) {
    var zoom by remember { mutableFloatStateOf(1f) }
    var x by remember { mutableFloatStateOf(0f) }
    var y by remember { mutableFloatStateOf(0f) }
    val preview = remember(source, zoom, x, y) { ProfilePhotos.crop(source, zoom, x, y, 512) }
    DisposableEffect(preview) { onDispose { preview.recycle() } }
    Dialog(onDismissRequest = cancel) {
        Surface(shape = MaterialTheme.shapes.extraLarge) {
            Column(Modifier.widthIn(max = 540.dp).verticalScroll(rememberScrollState()).padding(20.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                Text(stringResource(R.string.agent_photo_crop), style = MaterialTheme.typography.titleLarge)
                Text(stringResource(R.string.agent_photo_crop_detail), style = MaterialTheme.typography.bodySmall)
                Image(preview.asImageBitmap(), stringResource(R.string.agent_photo_crop), Modifier.fillMaxWidth().aspectRatio(1f).clip(CircleShape)
                    .pointerInput(source) { detectTransformGestures { _, pan, change, _ ->
                        zoom = (zoom * change).coerceIn(1f, 4f)
                        x = (x - pan.x / size.width * 2f).coerceIn(-1f, 1f)
                        y = (y - pan.y / size.height * 2f).coerceIn(-1f, 1f)
                    } })
                Text(stringResource(R.string.agent_photo_zoom)); Slider(zoom, { zoom = it }, valueRange = 1f..4f)
                Text(stringResource(R.string.agent_photo_horizontal)); Slider(x, { x = it }, valueRange = -1f..1f)
                Text(stringResource(R.string.agent_photo_vertical)); Slider(y, { y = it }, valueRange = -1f..1f)
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    TextButton(onClick = cancel, Modifier.weight(1f)) { Text(stringResource(R.string.cancel)) }
                    Button(onClick = { apply(zoom, x, y) }, Modifier.weight(1f)) { Text(stringResource(R.string.agent_photo_crop_apply)) }
                }
            }
        }
    }
}
