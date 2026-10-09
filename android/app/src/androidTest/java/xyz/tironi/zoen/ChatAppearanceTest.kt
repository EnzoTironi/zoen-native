package xyz.tironi.zoen

import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import kotlinx.coroutines.runBlocking
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import xyz.tironi.zoen.ui.ChatAppearanceStore

@RunWith(AndroidJUnit4::class)
class ChatAppearanceTest {
    @Test fun localAppearanceStaysPrivateAndSharedAppearanceCreatesASignedEvent() = runBlocking {
        val repository = ApplicationProvider.getApplicationContext<ZoenApplication>().repository
        repository.useDemo()
        val space = repository.state.value.zoenChat!!.id
        val before = repository.query { it.timeline(space).size }
        val local = ChatAppearanceStore.default().copy(style = "color:mint", appearance = "dark")
        ChatAppearanceStore.save(repository, space, local, true)
        assertEquals(before, repository.query { it.timeline(space).size })
        assertEquals(local, ChatAppearanceStore.load(repository, space))
        val shared = local.copy(style = "gradient:forest")
        ChatAppearanceStore.save(repository, space, shared, false)
        assertEquals(shared, repository.query { it.background(space) })
        assertFalse(repository.preferences.contains("background:$space"))
        assertEquals(before + 1, repository.query { it.timeline(space).size })
        assertTrue(repository.query { it.verifyLog(space).valid })
        ChatAppearanceStore.save(repository, space, ChatAppearanceStore.default(), true)
    }
}
