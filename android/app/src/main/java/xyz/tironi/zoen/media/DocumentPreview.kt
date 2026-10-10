package xyz.tironi.zoen.media

import android.graphics.Bitmap
import androidx.compose.foundation.Image
import androidx.compose.foundation.gestures.detectTransformGestures
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.rounded.NavigateBefore
import androidx.compose.material.icons.automirrored.rounded.NavigateNext
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import java.io.File
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import xyz.tironi.zoen.R

@Composable
fun DocumentPreview(file: File, mime: String, bytes: ByteArray, modifier: Modifier = Modifier) {
    var page by rememberSaveable(file.absolutePath) { mutableIntStateOf(0) }
    var error by remember(file.absolutePath) { mutableStateOf<String?>(null) }
    val pdf by produceState<NativePdf?>(null, file.absolutePath, mime) {
        if (mime == "application/pdf") {
            try { value = withContext(Dispatchers.IO) { NativePdf(file) } }
            catch (failure: Exception) { error = failure.message }
        }
    }
    DisposableEffect(pdf) { val document = pdf; onDispose { document?.close() } }
    val bitmap by produceState<Bitmap?>(null, file.absolutePath, pdf, page) {
        try { value = if (mime == "application/pdf") pdf?.let { withContext(Dispatchers.IO) { it.render(page) } } else DocumentMarkup.decodeImage(bytes, 1600) }
        catch (failure: Exception) { error = failure.message }
    }
    DisposableEffect(bitmap) { val image = bitmap; onDispose { image?.recycle() } }
    Column(modifier, verticalArrangement = Arrangement.spacedBy(8.dp)) {
        when {
            error != null -> Text(stringResource(if (mime == "application/pdf") R.string.media_pdf_unavailable else R.string.unavailable), color = MaterialTheme.colorScheme.error)
            bitmap != null -> ZoomImage(checkNotNull(bitmap), file.name, Modifier.fillMaxWidth().heightIn(min = 180.dp, max = 580.dp))
            else -> Box(Modifier.fillMaxWidth().height(200.dp), contentAlignment = Alignment.Center) { CircularProgressIndicator() }
        }
        if (pdf != null) PdfPageControls(page, pdf!!.pages, onPage = { page = it }, Modifier.fillMaxWidth())
    }
}

@Composable
fun PdfPageControls(page: Int, total: Int, onPage: (Int) -> Unit, modifier: Modifier = Modifier) {
    Row(modifier, verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.SpaceBetween) {
        IconButton(onClick = { onPage(page - 1) }, enabled = page > 0) { Icon(Icons.AutoMirrored.Rounded.NavigateBefore, stringResource(R.string.media_previous_page)) }
        Text(stringResource(R.string.media_page_number, page + 1, total), style = MaterialTheme.typography.labelLarge)
        IconButton(onClick = { onPage(page + 1) }, enabled = page < total - 1) { Icon(Icons.AutoMirrored.Rounded.NavigateNext, stringResource(R.string.media_next_page)) }
    }
}

@Composable
private fun ZoomImage(bitmap: Bitmap, name: String, modifier: Modifier) {
    var zoom by rememberSaveable(name) { mutableFloatStateOf(1f) }
    var offset by remember(name) { mutableStateOf(Offset.Zero) }
    Column(modifier) {
        Box(Modifier.fillMaxWidth().weight(1f, fill = false).aspectRatio(bitmap.width.toFloat() / bitmap.height).clip(RoundedCornerShape(16.dp)).pointerInput(bitmap) {
            detectTransformGestures { _, pan, factor, _ ->
                zoom = (zoom * factor).coerceIn(1f, 4f)
                val limitX = size.width * (zoom - 1) / 2; val limitY = size.height * (zoom - 1) / 2
                offset = Offset((offset.x + pan.x).coerceIn(-limitX, limitX), (offset.y + pan.y).coerceIn(-limitY, limitY))
            }
        }) {
            Image(bitmap.asImageBitmap(), name, Modifier.fillMaxSize().graphicsLayer(scaleX = zoom, scaleY = zoom, translationX = offset.x, translationY = offset.y), contentScale = ContentScale.Fit)
        }
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
            Text(stringResource(R.string.media_zoom), style = MaterialTheme.typography.labelSmall)
            Slider(zoom, { zoom = it; offset = Offset.Zero }, valueRange = 1f..4f, modifier = Modifier.weight(1f).testTag("document-zoom"))
        }
    }
}
