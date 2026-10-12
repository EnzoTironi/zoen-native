package xyz.tironi.zoen.media

import android.content.Context
import android.content.ContextWrapper
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import java.io.File
import java.util.UUID
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.Job
import kotlinx.coroutines.awaitCancellation
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class VoiceSourceLifetimeTest {
    @Test fun cancellationDeletesOnlyItsTranscriptionSource() = runBlocking {
        val context = ApplicationProvider.getApplicationContext<Context>()
        val directory = File(context.cacheDir, "voice-source-${UUID.randomUUID()}").apply { mkdirs() }
        val scoped = object : ContextWrapper(context) { override fun getCacheDir() = directory }
        val audio = PcmAudio(ShortArray(2_000) { (it % 1_000).toShort() }, 8_000, 2)
        val original = File(VoiceRecorder.directory(scoped), "recording.m4a").apply { writeBytes(byteArrayOf(1, 2, 3)) }
        val sources = List(2) { CompletableDeferred<File>() }
        val jobs = mutableListOf<Job>()
        try {
            sources.forEach { ready ->
                jobs += launch {
                    VoiceTranscriber.withPcmSource(scoped, audio) { source ->
                        ready.complete(source)
                        awaitCancellation()
                    }
                }
            }
            val first = withTimeout(5_000) { sources[0].await() }
            val replacement = withTimeout(5_000) { sources[1].await() }
            assertArrayEquals(audio.monoAt(16_000).bytes(), first.readBytes())
            assertNotEquals(first, replacement)
            jobs[0].cancelAndJoin()
            assertFalse("Cancelled transcription must remove its PCM source", first.exists())
            assertTrue("A concurrent transcription retains its own source", replacement.exists())
            assertArrayEquals(byteArrayOf(1, 2, 3), original.readBytes())
            jobs[1].cancelAndJoin()
            assertFalse(replacement.exists())
            assertEquals(listOf(original.name), original.parentFile!!.listFiles()!!.map { it.name })
        } finally {
            jobs.forEach { it.cancelAndJoin() }
            directory.deleteRecursively()
        }
    }
}
