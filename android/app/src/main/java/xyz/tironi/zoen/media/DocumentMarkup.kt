package xyz.tironi.zoen.media

import android.content.Context
import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.ImageDecoder
import android.graphics.Paint
import android.graphics.Path
import android.graphics.pdf.PdfRenderer
import android.os.ParcelFileDescriptor
import com.tom_roush.pdfbox.android.PDFBoxResourceLoader
import com.tom_roush.pdfbox.pdmodel.PDDocument
import com.tom_roush.pdfbox.pdmodel.PDPageContentStream
import com.tom_roush.pdfbox.pdmodel.graphics.state.PDExtendedGraphicsState
import java.io.ByteArrayOutputStream
import java.io.File
import java.nio.ByteBuffer
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlin.math.max
import kotlin.math.min

/** Points and widths use page fractions so zooming never changes the saved geometry. */
data class InkPoint(val x: Float, val y: Float) {
    init { require(x.isFinite() && y.isFinite() && x in 0f..1f && y in 0f..1f) }
}
data class InkStroke(val page: Int, val color: Int, val width: Float, val points: List<InkPoint>, val highlight: Boolean = false)

class InkHistory {
    var strokes: List<InkStroke> = emptyList()
        private set
    private val undo = mutableListOf<List<InkStroke>>()
    private val redo = mutableListOf<List<InkStroke>>()
    val canUndo get() = undo.isNotEmpty()
    val canRedo get() = redo.isNotEmpty()
    fun add(stroke: InkStroke) {
        if (stroke.points.isEmpty()) return
        undo += strokes; redo.clear(); strokes += stroke
    }
    fun undo() { if (undo.isNotEmpty()) { redo += strokes; strokes = undo.removeAt(undo.lastIndex) } }
    fun redo() { if (redo.isNotEmpty()) { undo += strokes; strokes = redo.removeAt(redo.lastIndex) } }
}

class NativePdf(file: File) : AutoCloseable {
    private val descriptor = ParcelFileDescriptor.open(file, ParcelFileDescriptor.MODE_READ_ONLY)
    private val renderer = try { PdfRenderer(descriptor) } catch (error: Exception) { descriptor.close(); throw error }
    val pages get() = renderer.pageCount
    @Synchronized fun render(index: Int, width: Int = 1400): Bitmap = renderer.openPage(index).use { page ->
        val height = max(1, (width.toDouble() * page.height / page.width).toInt()).coerceAtMost(5000)
        val actualWidth = min(width, (height.toDouble() * page.width / page.height).toInt()).coerceAtLeast(1)
        Bitmap.createBitmap(actualWidth, height, Bitmap.Config.ARGB_8888).apply {
            eraseColor(android.graphics.Color.WHITE)
            page.render(this, null, null, PdfRenderer.Page.RENDER_MODE_FOR_DISPLAY)
        }
    }
    @Synchronized override fun close() { renderer.close() }
}

object DocumentMarkup {
    suspend fun decodeImage(bytes: ByteArray, maxWidth: Int? = null): Bitmap = withContext(Dispatchers.IO) {
        ImageDecoder.decodeBitmap(ImageDecoder.createSource(ByteBuffer.wrap(bytes))) { decoder, info, _ ->
            decoder.allocator = ImageDecoder.ALLOCATOR_SOFTWARE
            decoder.isMutableRequired = true
            if (maxWidth != null && maxOf(info.size.width, info.size.height) > maxWidth) {
                val scale = maxWidth.toDouble() / maxOf(info.size.width, info.size.height)
                decoder.setTargetSize(max(1, (info.size.width * scale).toInt()), max(1, (info.size.height * scale).toInt()))
            }
        }
    }
    suspend fun image(bytes: ByteArray, mime: String, strokes: List<InkStroke>): ByteArray = withContext(Dispatchers.IO) {
        val bitmap = decodeImage(bytes)
        try {
            val canvas = Canvas(bitmap)
            val paint = Paint(Paint.ANTI_ALIAS_FLAG).apply { style = Paint.Style.STROKE; strokeCap = Paint.Cap.ROUND; strokeJoin = Paint.Join.ROUND }
            for (stroke in strokes) {
                paint.color = stroke.color
                paint.alpha = if (stroke.highlight) 85 else 255
                paint.strokeWidth = stroke.width * bitmap.width
                val path = Path()
                stroke.points.forEachIndexed { index, point -> if (index == 0) path.moveTo(point.x * bitmap.width, point.y * bitmap.height) else path.lineTo(point.x * bitmap.width, point.y * bitmap.height) }
                if (stroke.points.size == 1) canvas.drawCircle(stroke.points[0].x * bitmap.width, stroke.points[0].y * bitmap.height, paint.strokeWidth / 2, Paint(paint).apply { style = Paint.Style.FILL })
                else canvas.drawPath(path, paint)
            }
            val format = when (mime.lowercase()) {
                "image/jpeg", "image/jpg" -> Bitmap.CompressFormat.JPEG
                "image/webp" -> Bitmap.CompressFormat.WEBP
                else -> Bitmap.CompressFormat.PNG
            }
            ByteArrayOutputStream().use { out -> check(bitmap.compress(format, 95, out)); out.toByteArray() }
        } finally { bitmap.recycle() }
    }
    suspend fun pdf(context: Context, file: File, strokes: List<InkStroke>): ByteArray = withContext(Dispatchers.IO) {
        PDFBoxResourceLoader.init(context.applicationContext)
        PDDocument.load(file).use { document ->
            require(document.currentAccessPermission.canModify()) { "This PDF does not allow editing." }
            for ((pageIndex, pageStrokes) in strokes.groupBy { it.page }) {
                require(pageIndex in 0 until document.numberOfPages)
                val page = document.getPage(pageIndex)
                val box = page.cropBox
                val rotation = ((page.rotation % 360) + 360) % 360
                val displayWidth = if (rotation == 90 || rotation == 270) box.height else box.width
                fun point(value: InkPoint): Pair<Float, Float> = when (rotation) {
                    90 -> (box.lowerLeftX + value.y * box.width) to (box.lowerLeftY + value.x * box.height)
                    180 -> (box.lowerLeftX + (1 - value.x) * box.width) to (box.lowerLeftY + value.y * box.height)
                    270 -> (box.lowerLeftX + (1 - value.y) * box.width) to (box.lowerLeftY + (1 - value.x) * box.height)
                    else -> (box.lowerLeftX + value.x * box.width) to (box.upperRightY - value.y * box.height)
                }
                PDPageContentStream(document, page, PDPageContentStream.AppendMode.APPEND, true, true).use { stream ->
                    stream.setLineCapStyle(1); stream.setLineJoinStyle(1)
                    for (stroke in pageStrokes) {
                        if (stroke.points.isEmpty()) continue
                        stream.saveGraphicsState()
                        val color = stroke.color
                        stream.setStrokingColor((color shr 16 and 255) / 255f, (color shr 8 and 255) / 255f, (color and 255) / 255f)
                        stream.setGraphicsStateParameters(PDExtendedGraphicsState().apply { strokingAlphaConstant = if (stroke.highlight) .33f else 1f })
                        stream.setLineWidth(stroke.width * displayWidth)
                        val first = point(stroke.points.first())
                        stream.moveTo(first.first, first.second)
                        if (stroke.points.size == 1) stream.lineTo(first.first + .1f, first.second + .1f)
                        else stroke.points.drop(1).forEach { val next = point(it); stream.lineTo(next.first, next.second) }
                        stream.stroke(); stream.restoreGraphicsState()
                    }
                }
            }
            ByteArrayOutputStream().use { output -> document.save(output); output.toByteArray() }
        }
    }
    fun canKeepMime(mime: String) = mime.lowercase() in setOf("application/pdf", "image/png", "image/jpeg", "image/jpg", "image/webp")
    suspend fun thumbnail(bytes: ByteArray): ByteArray? = runCatching {
        val bitmap = decodeImage(bytes, 480)
        try { ByteArrayOutputStream().use { output -> bitmap.compress(Bitmap.CompressFormat.PNG, 100, output); output.toByteArray() } }
        finally { bitmap.recycle() }
    }.getOrNull()
}
