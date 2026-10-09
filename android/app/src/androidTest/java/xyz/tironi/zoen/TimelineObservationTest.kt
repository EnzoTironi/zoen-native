package xyz.tironi.zoen

import android.content.Context
import android.content.ContextWrapper
import android.content.res.Configuration
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import java.io.File
import java.util.Locale
import java.util.UUID
import kotlinx.coroutines.runBlocking
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import xyz.tironi.zoen.data.ZoenRepository

@RunWith(AndroidJUnit4::class)
class TimelineObservationTest {
    @Test fun staleChatDisposalKeepsTheReplacementTimelineSubscribed() = runBlocking {
        val context = ApplicationProvider.getApplicationContext<Context>()
        val namespace = "timeline-observation-${UUID.randomUUID()}"
        val directory = File(context.noBackupFilesDir, namespace).apply { mkdirs() }
        val localized = context.createConfigurationContext(Configuration(context.resources.configuration).apply { setLocale(Locale.ENGLISH) })
        val scoped = object : ContextWrapper(localized) {
            override fun getNoBackupFilesDir(): File = directory
            override fun getSharedPreferences(name: String, mode: Int) = context.getSharedPreferences("$namespace-$name", mode)
        }
        val repository = ZoenRepository(scoped)
        try {
            repository.boot(true)
            val space = checkNotNull(repository.state.value.zoenChat).id
            val otherSpace = repository.state.value.spaces.first { it.id != space }.id
            val oldScreen = Any()
            val replacement = Any()
            repository.observe(space, oldScreen)
            repository.observe(space, oldScreen) // Recomposition must not acquire another subscription.
            repository.observe(otherSpace, oldScreen)
            val before = repository.change { it.sendMessage(space, "Before recreation $namespace") }
            repository.observe(space, replacement)

            // Reproduce the old screen's cleanup running after its replacement observes.
            repository.unobserve(space, oldScreen)
            assertNotNull("The replacement screen must retain its timeline", repository.state.value.timelines[space])
            val replacementRevision = repository.state.value.revision
            repository.unobserve(space, oldScreen)
            repository.unobserve(space, Any())
            assertEquals(replacementRevision, repository.state.value.revision)
            val after = repository.change { it.sendMessage(space, "After recreation $namespace") }
            val timeline = checkNotNull(repository.state.value.timelines[space])
            assertTrue(timeline.any { it.id == before.id })
            assertTrue(timeline.any { it.id == after.id })
            assertEquals(repository.query { it.timeline(space).map { entry -> entry.id } }, timeline.map { it.id })
            assertTrue(repository.state.value.timelines.containsKey(otherSpace))

            repository.unobserve(space, replacement)
            repository.refresh()
            assertFalse(repository.state.value.timelines.containsKey(space))
            assertTrue(repository.state.value.timelines.containsKey(otherSpace))
            repository.unobserve(otherSpace, oldScreen)
            assertTrue(repository.state.value.timelines.isEmpty())
            assertTrue(repository.query { it.verifyAll().all { report -> report.valid } })
        } finally {
            repository.close()
            directory.deleteRecursively()
            context.deleteSharedPreferences("$namespace-zoen")
        }
    }
}
