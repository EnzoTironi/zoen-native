package xyz.tironi.zoen.media

import android.graphics.Bitmap
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.Image
import androidx.compose.foundation.gestures.detectDragGestures
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewmodel.compose.viewModel
import java.io.File
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import xyz.tironi.zoen.R

class MarkupSession : ViewModel() {
    val ink = InkHistory()
    var page by mutableIntStateOf(0)
    var revision by mutableIntStateOf(0)
    var busy by mutableStateOf(false)
    var error by mutableStateOf<String?>(null)
    var width by mutableFloatStateOf(.006f)
    var color by mutableIntStateOf(android.graphics.Color.rgb(194, 46, 39))
    var highlight by mutableStateOf(false)
    fun add(stroke: InkStroke) { ink.add(stroke); revision++ }
    fun undo() { ink.undo(); revision++ }
    fun redo() { ink.redo(); revision++ }
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
fun MarkupEditor(itemId: String, version: UInt, file: File, mime: String, bytes: ByteArray, onDismiss: () -> Unit, onSave: (MarkupSession, List<InkStroke>) -> Unit) {
    val session: MarkupSession = viewModel(key = "markup:$itemId:$version")
    val pdf by produceState<NativePdf?>(null, file.absolutePath) {
        if (mime == "application/pdf") try { value = withContext(Dispatchers.IO) { NativePdf(file) } } catch (error: Exception) { session.error = error.message }
    }
    DisposableEffect(pdf) { val document = pdf; onDispose { document?.close() } }
    val bitmap by produceState<Bitmap?>(null, file.absolutePath, pdf, session.page) {
        try { value = if (mime == "application/pdf") pdf?.let { withContext(Dispatchers.IO) { it.render(session.page, 1800) } } else DocumentMarkup.decodeImage(bytes, 1800) }
        catch (error: Exception) { session.error = error.message }
    }
    DisposableEffect(bitmap) { val image = bitmap; onDispose { image?.recycle() } }
    // Reading revision makes undo/redo replace the displayed immutable stroke list.
    val strokes = remember(session.revision) { session.ink.strokes }
    val colors = listOf(0xffc22e27.toInt() to R.string.media_ink_red, 0xff28784a.toInt() to R.string.media_ink_green, 0xff2267b5.toInt() to R.string.media_ink_blue, 0xffffd126.toInt() to R.string.media_ink_yellow, 0xff181c18.toInt() to R.string.media_ink_black)
    Dialog(onDismissRequest = { if (!session.busy) onDismiss() }, properties = DialogProperties(usePlatformDefaultWidth = false, dismissOnClickOutside = false, dismissOnBackPress = !session.busy)) {
        Surface(Modifier.widthIn(max = 900.dp).fillMaxWidth().fillMaxHeight().safeDrawingPadding(), shape = RoundedCornerShape(20.dp)) {
            Column(Modifier.fillMaxSize().padding(12.dp)) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Text(stringResource(R.string.media_markup), Modifier.weight(1f), style = MaterialTheme.typography.titleLarge)
                    IconButton(onClick = session::undo, enabled = session.ink.canUndo && !session.busy) { Icon(Icons.Rounded.Undo, stringResource(R.string.media_undo)) }
                    IconButton(onClick = session::redo, enabled = session.ink.canRedo && !session.busy) { Icon(Icons.Rounded.Redo, stringResource(R.string.media_redo)) }
                    IconButton(onClick = onDismiss, enabled = !session.busy) { Icon(Icons.Rounded.Close, stringResource(R.string.close)) }
                }
                FlowRow(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                    FilterChip(selected = !session.highlight, onClick = { session.highlight = false }, enabled = !session.busy, label = { Text(stringResource(R.string.media_pen)) }, leadingIcon = { Icon(Icons.Rounded.Edit, null, Modifier.size(18.dp)) })
                    FilterChip(selected = session.highlight, onClick = { session.highlight = true }, enabled = !session.busy, label = { Text(stringResource(R.string.media_highlighter)) }, leadingIcon = { Icon(Icons.Rounded.Highlight, null, Modifier.size(18.dp)) })
                    colors.forEach { (color, label) ->
                        val description = stringResource(label)
                        FilterChip(selected = session.color == color, onClick = { session.color = color }, enabled = !session.busy, label = { Canvas(Modifier.size(20.dp)) { drawCircle(Color(color)) } }, modifier = Modifier.testTag("ink:$label").semantics { contentDescription = description }, leadingIcon = if (session.color == color) { { Icon(Icons.Rounded.Check, null, Modifier.size(16.dp)) } } else null)
                    }
                }
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                    Text(stringResource(R.string.media_stroke_width), style = MaterialTheme.typography.labelSmall)
                    Slider(session.width, { session.width = it }, valueRange = 0.002f..0.025f, enabled = !session.busy, modifier = Modifier.weight(1f))
                }
                if (pdf != null) PdfPageControls(session.page, pdf!!.pages, { session.page = it })
                BoxWithConstraints(Modifier.weight(1f).fillMaxWidth(), contentAlignment = Alignment.Center) {
                    bitmap?.let { image ->
                        val ratio = image.width.toFloat() / image.height
                        val width = minOf(maxWidth, maxHeight * ratio)
                        DrawingPage(image, strokes.filter { it.page == session.page }, session.page, session.color, if (session.highlight) session.width * 3 else session.width, session.highlight, !session.busy,
                            Modifier.width(width).height(width / ratio).testTag("markup-canvas"), session::add)
                    } ?: CircularProgressIndicator()
                }
                session.error?.let { Text(it, color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodySmall) }
                Button(onClick = { onSave(session, strokes) }, enabled = strokes.isNotEmpty() && !session.busy, modifier = Modifier.fillMaxWidth().testTag("markup-save")) {
                    if (session.busy) { CircularProgressIndicator(Modifier.size(18.dp), strokeWidth = 2.dp, color = MaterialTheme.colorScheme.onPrimary); Spacer(Modifier.width(8.dp)) }
                    Text(stringResource(R.string.save))
                }
            }
        }
    }
}

@Composable
private fun DrawingPage(bitmap: Bitmap, strokes: List<InkStroke>, page: Int, color: Int, width: Float, highlight: Boolean, enabled: Boolean, modifier: Modifier, onStroke: (InkStroke) -> Unit) {
    var live by remember(page) { mutableStateOf(emptyList<InkPoint>()) }
    val onDraw by rememberUpdatedState(onStroke)
    val label = stringResource(R.string.media_draw_document)
    Box(modifier) {
        Image(bitmap.asImageBitmap(), label, Modifier.fillMaxSize(), contentScale = ContentScale.FillBounds)
        Canvas(Modifier.fillMaxSize().pointerInput(page, color, width, highlight, enabled) {
            fun point(offset: Offset) = InkPoint((offset.x / size.width).coerceIn(0f, 1f), (offset.y / size.height).coerceIn(0f, 1f))
            if (enabled) detectDragGestures(onDragStart = { live = listOf(point(it)) }, onDrag = { change, _ -> change.consume(); live = live + point(change.position) },
                onDragEnd = { if (live.isNotEmpty()) onDraw(InkStroke(page, color, width, live, highlight)); live = emptyList() }, onDragCancel = { live = emptyList() })
        }.pointerInput(page, color, width, highlight, enabled) {
            if (enabled) detectTapGestures { offset -> onDraw(InkStroke(page, color, width, listOf(InkPoint((offset.x / size.width).coerceIn(0f, 1f), (offset.y / size.height).coerceIn(0f, 1f))), highlight)) }
        }) {
            for (stroke in strokes + if (live.isEmpty()) emptyList() else listOf(InkStroke(page, color, width, live, highlight))) {
                val ink = Color(stroke.color).copy(alpha = if (stroke.highlight) .33f else 1f)
                val weight = stroke.width * size.width
                if (stroke.points.size == 1) drawCircle(ink, weight / 2, Offset(stroke.points[0].x * size.width, stroke.points[0].y * size.height))
                else {
                    val path = Path()
                    stroke.points.forEachIndexed { index, point -> if (index == 0) path.moveTo(point.x * size.width, point.y * size.height) else path.lineTo(point.x * size.width, point.y * size.height) }
                    drawPath(path, ink, style = Stroke(weight, cap = StrokeCap.Round, join = StrokeJoin.Round))
                }
            }
        }
    }
}
