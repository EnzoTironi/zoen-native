package xyz.tironi.zoen.media

import android.content.Context
import android.content.ContextWrapper
import android.graphics.Bitmap
import android.graphics.Color
import android.media.MediaMetadataRetriever
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import com.tom_roush.pdfbox.android.PDFBoxResourceLoader
import com.tom_roush.pdfbox.pdmodel.PDDocument
import com.tom_roush.pdfbox.pdmodel.PDPage
import com.tom_roush.pdfbox.pdmodel.PDPageContentStream
import com.tom_roush.pdfbox.pdmodel.common.PDRectangle
import com.tom_roush.pdfbox.pdmodel.font.PDType1Font
import com.tom_roush.pdfbox.text.PDFTextStripper
import java.io.ByteArrayOutputStream
import java.io.File
import java.util.UUID
import kotlin.math.PI
import kotlin.math.abs
import kotlin.math.cos
import kotlin.math.sin
import kotlinx.coroutines.runBlocking
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import xyz.tironi.zoen.core.RodaEngine
import xyz.tironi.zoen.data.ZoenRepository

@RunWith(AndroidJUnit4::class)
class MediaEditingTest {
    private val context get() = ApplicationProvider.getApplicationContext<Context>()
    private fun folder() = File(context.cacheDir, "media-test-${UUID.randomUUID()}").apply { mkdirs() }
    private fun tone(seconds: Double, frequency: Double, rate: Int = 44_100) = PcmAudio(ShortArray((seconds * rate).toInt()) { (sin(it * 2 * PI * frequency / rate) * 12_000).toInt().toShort() }, rate, 1)

    @Test fun realAacRenderExcludesRemovedToneAndHasTheEditedDuration() = runBlocking {
        val dir = folder()
        try {
            val first = tone(1.0, 220.0); val removed = tone(1.0, 880.0); val last = tone(1.0, 440.0)
            val source = PcmAudio(first.samples + removed.samples + last.samples, first.sampleRate, 1)
            val original = AudioFiles.encode(source, File(dir, "original.m4a"))
            val editor = VoiceEditor(source.duration, VoiceTranscript("first secret last", listOf(TranscriptWord("first", 0.0, 1.0), TranscriptWord("secret", 1.0, 1.0), TranscriptWord("last", 2.0, 1.0))))
            editor.removeWords(setOf(1))
            val exported = AudioFiles.encode(source.edit(editor.keptRanges), File(dir, "edited.m4a"))
            assertEquals(2.0, MediaExport.duration(exported), .15)
            assertEquals("first last", editor.keptTranscript)
            val decoded = AudioFiles.decode(exported)
            fun energy(frequency: Double): Double {
                var real = 0.0; var imaginary = 0.0
                decoded.samples.forEachIndexed { i, sample -> val angle = i * 2 * PI * frequency / decoded.sampleRate; real += sample * cos(angle); imaginary += sample * sin(angle) }
                return kotlin.math.hypot(real, imaginary) / decoded.samples.size
            }
            assertTrue("Removed 880 Hz tone survived export", energy(880.0) < energy(220.0) * .03)
            assertTrue(original.exists()) // The original remains available for retry until send succeeds.
        } finally { dir.deleteRecursively() }
    }

    @Test fun voiceAttachmentMarkerAndTypedFileVersionsSurviveReopening() = runBlocking {
        val dir = folder()
        val localized = context.createConfigurationContext(android.content.res.Configuration(context.resources.configuration).apply { setLocale(java.util.Locale.ENGLISH) })
        val wrapped = object : ContextWrapper(localized) {
            override fun getNoBackupFilesDir(): File = File(dir, "no-backup").apply { mkdirs() }
            override fun getSharedPreferences(name: String, mode: Int) = context.getSharedPreferences("${dir.name}-$name", mode)
        }
        val repository = ZoenRepository(wrapped)
        repository.boot(true)
        try {
            val space = checkNotNull(repository.state.value.zoenChat).id
            val audio = tone(1.2, 320.0)
            val id = UUID.randomUUID().toString()
            val clip = VoiceClip(id, AudioFiles.encode(audio, File(dir, "$id.m4a")), audio)
            val entry = VoiceTransport.send(repository, clip, "A real encoded voice note.", space)
            val reference = checkNotNull(VoiceNoteRef.parse((entry.kind as xyz.tironi.zoen.core.EntryKind.Message).text))
            assertFalse(clip.file.exists())
            val attachment = repository.query { it.item(reference.id) }
            assertEquals("audio/mp4", attachment.file!!.mime)
            assertArrayEquals(repository.query { it.fileBytes(reference.id, null) }, VoiceTransport.localFile(wrapped, repository, reference)!!.readBytes())
            assertTrue(repository.query { it.verifyAll().all { report -> report.valid } })
            // Reopen the on-disk log through a separate FFI object, without using the UI cache.
            val database = File(wrapped.noBackupFilesDir, "core/demo-en.sqlite")
            repository.query { it.destroy() }
            val reopened = RodaEngine.open(database.absolutePath, "en")
            try {
                val old = reopened.fileBytes(reference.id, 1u)!!
                val new = AudioFiles.encode(audio.edit(listOf(TimeRange(.2, .9))), File(dir, "trim.m4a")).readBytes()
                val version = reopened.fileNewVersionTyped(reference.id, new, null, "Trimmed", "short.m4a", "audio/mp4")
                assertEquals(2u, version.version); assertEquals("short.m4a", version.file!!.name)
                assertArrayEquals(old, reopened.fileBytes(reference.id, 1u)); assertArrayEquals(new, reopened.fileBytes(reference.id, null))
                assertTrue(reopened.verifyAll().all { it.valid })
            } finally { reopened.destroy() }
        } finally { repository.query { it.destroy() }; dir.deleteRecursively() }
    }

    @Test fun pdfInkPreservesTextEveryPageAndTheOriginalVersionBytes() = runBlocking {
        val dir = folder()
        PDFBoxResourceLoader.init(context)
        try {
            val input = File(dir, "original.pdf")
            PDDocument().use { document ->
                repeat(2) { index ->
                    val page = PDPage(PDRectangle(300f, 400f)); document.addPage(page)
                    PDPageContentStream(document, page).use { content ->
                        content.beginText(); content.setFont(PDType1Font.HELVETICA, 18f); content.newLineAtOffset(20f, 350f); content.showText("Preserved page ${index + 1}"); content.endText()
                    }
                }
                document.save(input)
            }
            val original = input.readBytes()
            val strokes = listOf(InkStroke(1, Color.RED, .015f, listOf(InkPoint(.2f, .5f), InkPoint(.8f, .5f))))
            val edited = DocumentMarkup.pdf(context, input, strokes)
            assertArrayEquals(original, input.readBytes())
            PDDocument.load(edited).use { document ->
                assertEquals(2, document.numberOfPages)
                assertTrue(PDFTextStripper().getText(document).contains("Preserved page 1"))
                assertTrue(PDFTextStripper().getText(document).contains("Preserved page 2"))
            }
            val exported = File(dir, "edited.pdf").apply { writeBytes(edited) }
            NativePdf(exported).use { pdf ->
                val page = pdf.render(1, 300)
                try { val color = page.getPixel(page.width / 2, page.height / 2); assertTrue(Color.red(color) > 180); assertTrue(Color.green(color) < 100) }
                finally { page.recycle() }
            }
        } finally { dir.deleteRecursively() }
    }

    @Test fun imageInkIsWrittenIntoActualEncodedPixels() = runBlocking {
        val bitmap = Bitmap.createBitmap(100, 80, Bitmap.Config.ARGB_8888).apply { eraseColor(Color.WHITE) }
        val bytes = try { ByteArrayOutputStream().use { output -> bitmap.compress(Bitmap.CompressFormat.PNG, 100, output); output.toByteArray() } } finally { bitmap.recycle() }
        val output = DocumentMarkup.image(bytes, "image/png", listOf(InkStroke(0, Color.RED, .05f, listOf(InkPoint(.1f, .5f), InkPoint(.9f, .5f)))))
        val decoded = DocumentMarkup.decodeImage(output)
        try { assertEquals(Color.RED, decoded.getPixel(50, 40)); assertEquals(Color.WHITE, decoded.getPixel(50, 10)) } finally { decoded.recycle() }
    }

    @Test fun videoExportContainsOnlySelectedGreenFramesAndNoHiddenPreRoll() = runBlocking {
        val dir = folder()
        try {
            val input = File(dir, "colors.mp4")
            InstrumentationRegistry.getInstrumentation().context.assets.open("trim-colors.mp4").use { stream -> input.outputStream().use(stream::copyTo) }
            val output = MediaExport.trimVideo(context, input, TimeRange(1.7, 2.7), File(dir, "selected.mp4"))
            assertEquals(1.0, MediaExport.duration(output), .15)
            val retriever = MediaMetadataRetriever()
            try {
                retriever.setDataSource(output.absolutePath)
                for (time in listOf(0L, 200_000L, 800_000L)) {
                    val frame = checkNotNull(retriever.getFrameAtTime(time, MediaMetadataRetriever.OPTION_CLOSEST))
                    try { val pixel = frame.getPixel(frame.width / 2, frame.height / 2); assertTrue("Frame has excluded red/blue content", Color.green(pixel) > Color.red(pixel) + 50 && Color.green(pixel) > Color.blue(pixel) + 50) }
                    finally { frame.recycle() }
                }
            }
            finally { retriever.release() }
            // A decoded export starts at the first selected frame, with no negative media samples.
            val extractor = android.media.MediaExtractor()
            try { extractor.setDataSource(output.absolutePath); extractor.selectTrack(0); assertTrue(extractor.sampleTime >= 0) } finally { extractor.release() }
            assertTrue(input.exists())
        } finally { dir.deleteRecursively() }
    }
}
