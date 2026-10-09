package xyz.tironi.zoen

import android.app.Notification
import android.app.NotificationManager
import android.os.Build
import android.view.View
import android.widget.FrameLayout
import android.widget.ImageView
import android.widget.TextView
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import java.io.File
import java.util.UUID
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import xyz.tironi.zoen.background.ActivityNotifications
import xyz.tironi.zoen.core.*
import xyz.tironi.zoen.data.AndroidSecretVault
import xyz.tironi.zoen.data.AppState
import xyz.tironi.zoen.data.ZoenRepository
import xyz.tironi.zoen.data.FileAccess
import xyz.tironi.zoen.miniapps.WidgetSnapshot
import xyz.tironi.zoen.pages.PageEditing
import xyz.tironi.zoen.widgets.ZoenWidgetProvider

@RunWith(AndroidJUnit4::class)
class NativeParityTest {
    @Test fun signOutErasesPrivateDraftsAndPreferencesBeforeAnotherAccountOpens() = kotlinx.coroutines.runBlocking {
        val context = ApplicationProvider.getApplicationContext<ZoenApplication>()
        val namespace = "ownership-${UUID.randomUUID()}"
        val folder = File(context.noBackupFilesDir, namespace).apply { mkdirs() }
        val scoped = object : android.content.ContextWrapper(context) {
            override fun getNoBackupFilesDir() = folder
            override fun getSharedPreferences(name: String, mode: Int) = context.getSharedPreferences("$namespace-$name", mode)
        }
        val repository = ZoenRepository(scoped)
        try {
            repository.boot()
            val firstActivity = Any()
            val secondActivity = Any()
            repository.setAppVisible(true, firstActivity)
            repository.setAppVisible(true, secondActivity)
            repository.setAppVisible(false, firstActivity)
            assertTrue(repository.appVisible)
            repository.setAppVisible(false, secondActivity)
            assertFalse(repository.appVisible)
            repository.createAccount("First owner", "owner_a", "http://10.0.2.2:18787")
            val oldOwner = repository.state.value.me!!.id
            val oldKey = repository.localKey("pageDraft", "shared-page")
            repository.preferences.edit().putString(oldKey, "Private draft").putString("search.recents", "Private query")
                .putString(repository.localKey("background", "shared-chat"), "Private photo")
                .putBoolean(repository.localKey("muted.person", "shared-person"), true).commit()
            repository.signOut()
            assertNull(repository.state.value.me)
            assertFalse(repository.preferences.contains(oldKey))
            assertFalse(repository.preferences.contains("search.recents"))
            assertFalse(repository.preferences.contains(repository.localKey("background", "shared-chat", oldOwner)))
            repository.createAccount("Second owner", "owner_b", "http://10.0.2.2:18787")
            assertNotEquals(oldOwner, repository.state.value.me!!.id)
            assertNotEquals(oldKey, repository.localKey("pageDraft", "shared-page"))
            assertNull(repository.preferences.getString(repository.localKey("pageDraft", "shared-page"), null))
            assertFalse(repository.preferences.getBoolean(repository.localKey("muted.person", "shared-person"), false))
            assertEquals("offline", repository.state.value.connection.state)
        } finally {
            repository.signOut(); repository.close(); folder.deleteRecursively()
            context.deleteSharedPreferences("$namespace-zoen")
        }
    }

    @Test fun nativeDocumentUrisBatchMarkdownAndKeepImageThumbnailsInTheChosenFolder() = kotlinx.coroutines.runBlocking {
        val context = ApplicationProvider.getApplicationContext<ZoenApplication>()
        val namespace = "files-${UUID.randomUUID()}"
        val folder = File(context.noBackupFilesDir, namespace).apply { mkdirs() }
        val scoped = object : android.content.ContextWrapper(context) {
            override fun getNoBackupFilesDir() = folder
            override fun getSharedPreferences(name: String, mode: Int) = context.getSharedPreferences("$namespace-$name", mode)
        }
        val exports = File(context.cacheDir, "exports/$namespace").apply { mkdirs() }
        val repository = ZoenRepository(scoped)
        try {
            repository.boot(true)
            val space = repository.state.value.spaces.first { it.members.count { person -> person.kind == PersonaKind.PERSON } > 1 }
            val first = File(exports, "Café.md").apply { writeText("# Café\n\nA **shared** note") }
            val second = File(exports, "More.markdown").apply { writeText("# More\n\n- Bring water") }
            val image = File(exports, "Forest.png")
            val bitmap = android.graphics.Bitmap.createBitmap(24, 24, android.graphics.Bitmap.Config.ARGB_8888).apply { eraseColor(android.graphics.Color.GREEN) }
            image.outputStream().use { bitmap.compress(android.graphics.Bitmap.CompressFormat.PNG, 100, it) }; bitmap.recycle()
            val uris = listOf(first, second, image).map { androidx.core.content.FileProvider.getUriForFile(context, "${context.packageName}.files", it) }
            val imported = FileAccess.importMany(scoped, repository, space.id, uris)
            assertEquals(3, imported.size)
            assertTrue(imported.all { it.spaceId == space.id })
            assertEquals(2, imported.count { it.kindId == "page" })
            val photo = imported.single { it.kindId == "file" }
            assertTrue(photo.file!!.hasThumbnail)
            assertArrayEquals(image.readBytes(), repository.query { it.fileBytes(photo.id, null) })
            val state = repository.state.value
            val found = xyz.tironi.zoen.ui.filterFiles(state, xyz.tironi.zoen.ui.FileScope.SHARED, "cafe", space.id)
            assertTrue(found.any { it.title == "Café" })
            assertTrue(xyz.tironi.zoen.ui.filterFiles(state, xyz.tironi.zoen.ui.FileScope.PERSONAL, "", space.id).isEmpty())
            assertTrue(repository.query { it.pageMarkdown(found.first { item -> item.title == "Café" }.id) }.contains("**shared**"))
            assertTrue(repository.query { it.verifyAll().all { report -> report.valid } })
        } finally {
            repository.close(); exports.deleteRecursively(); folder.deleteRecursively()
            context.deleteSharedPreferences("$namespace-zoen")
        }
    }

    @Test fun richPageFormattingAndHistoricalContentSurviveKeystoreReopening() {
        val context = ApplicationProvider.getApplicationContext<ZoenApplication>()
        val namespace = "rich-${UUID.randomUUID()}"
        val folder = File(context.noBackupFilesDir, namespace).apply { mkdirs() }
        val database = File(folder, "account.sqlite").absolutePath
        val vault = AndroidSecretVault(context, "$namespace-vault")
        var core = RodaEngine.open(database, "en")
        try {
            core.createAccount("Rich Page Test", "rich_page_test", "http://10.0.2.2:18787", vault)
            val space = core.spaces().first { it.counterpart?.handle == "zoen" }
            val markdown = "# Rich page\n\nA **styled 🌿** word and *italic* with [link](https://example.com).\n\n7. A numbered item\n   - Nested item\n\n```kotlin\nval x = 1\n```\n\n![Forest](https://example.com/forest.png)\n"
            val item = core.pageImportMarkdown(space.id, "notes/rich.md", markdown)
            val original = core.page(item.id)
            assertTrue(original.blocks.any { it.spans.isNotEmpty() })
            val body = original.blocks.first { it.kind == "paragraph" && it.text.contains("styled") }
            val changed = PageEditing.replaceText(body, "Updated " + body.text)
            val blocks = PageEditing.decode(PageEditing.encode(original.blocks.map { if (it.id == body.id) changed else it }))!!
            core.pageApply(item.id, blocks.map { it.id }, listOf(changed))
            core.pageCommit(item.id, "Rich text edited")
            assertEquals(2u, core.item(item.id).version)
            assertEquals(original.blocks, core.pageAt(item.id, 1u).blocks)
            assertTrue(core.pageMarkdown(item.id).contains("**styled 🌿**"))
            core.destroy()
            core = RodaEngine.open(database, "en")
            assertTrue(core.unlock(vault))
            val reopened = core.page(item.id)
            assertEquals(changed.spans, reopened.blocks.first { it.id == body.id }.spans)
            listOf("image", "code", "numbered", "bullet").forEach { kind ->
                assertEquals(original.blocks.filter { it.kind == kind }, reopened.blocks.filter { it.kind == kind })
            }
            assertEquals("Rich page", core.itemAt(item.id, 1u).title)
            core.restoreVersion(item.id, 1u)
            assertEquals(3u, core.item(item.id).version)
            assertEquals(original.blocks, core.page(item.id).blocks)
            assertTrue(core.verifyAll().all { it.valid })
        } finally {
            core.eraseDevice(vault); core.destroy(); folder.deleteRecursively()
            File(context.noBackupFilesDir, "$namespace-vault").deleteRecursively()
        }
    }

    @Test fun everyWidgetTemplateInflatesAsRealRemoteViewsAndHidesSensitiveContent() {
        val context = ApplicationProvider.getApplicationContext<ZoenApplication>()
        val core = RodaEngine.open(":memory:", "en")
        try {
            core.seedDemoIfEmpty()
            val space = core.spaces().first { it.counterpart?.handle == "zoen" }
            val item = core.installApp(space.id, "hike", "{}")
            val sample = checkNotNull(WidgetSnapshot.from(item))
            InstrumentationRegistry.getInstrumentation().runOnMainSync {
                WidgetSnapshot.Template.entries.forEachIndexed { index, template ->
                    val snapshot = sample.copy(template = template, title = "Native $template", value = "42", detail = "Shared state", rows = listOf(WidgetSnapshot.Row("Bring water", true)), bars = listOf(WidgetSnapshot.Bar("Rest", .5)), targetMs = System.currentTimeMillis() + 86_400_000, codes = listOf("SF", "TML"), places = listOf("San Francisco", "Tomales"), times = listOf("08:00", "09:00"), photo = if (template == WidgetSnapshot.Template.PHOTO) "hike-tomales" else null)
                    val view = ZoenWidgetProvider.views(context, index, snapshot, false, false).apply(context, FrameLayout(context))
                    assertEquals(snapshot.title, view.findViewById<TextView>(R.id.widget_title).text.toString())
                    if (template == WidgetSnapshot.Template.PHOTO) assertEquals(View.VISIBLE, view.findViewById<ImageView>(R.id.widget_photo).visibility)
                    val private = snapshot.copy(sensitive = true, eyebrow = "Private eyebrow")
                    val locked = ZoenWidgetProvider.views(context, index, private, true, false).apply(context, FrameLayout(context))
                    assertEquals(context.getString(R.string.app_name), locked.findViewById<TextView>(R.id.widget_title).text.toString())
                    assertEquals(context.getString(R.string.app_name), locked.findViewById<TextView>(R.id.widget_eyebrow).text.toString())
                    assertEquals("", locked.findViewById<TextView>(R.id.widget_detail).text.toString())
                    assertEquals(View.GONE, locked.findViewById<ImageView>(R.id.widget_photo).visibility)
                }
            }
        } finally { core.destroy() }
    }

    @Test fun AndroidNotificationsUsePrivatePreviewsAndHonorReadMuteAndVisibleChat() {
        val context = ApplicationProvider.getApplicationContext<ZoenApplication>()
        if (Build.VERSION.SDK_INT >= 33) InstrumentationRegistry.getInstrumentation().uiAutomation.executeShellCommand("pm grant ${context.packageName} android.permission.POST_NOTIFICATIONS").use { android.os.ParcelFileDescriptor.AutoCloseInputStream(it).readBytes() }
        val manager = context.getSystemService(NotificationManager::class.java)
        val core = RodaEngine.open(":memory:", "en")
        val tag = "chat:notification-${UUID.randomUUID()}"
        val preferences = context.getSharedPreferences("zoen", android.content.Context.MODE_PRIVATE)
        val hadChoice = preferences.contains("notifications"); val oldChoice = preferences.getBoolean("notifications", false)
        preferences.edit().putBoolean("notifications", true).commit()
        try {
            core.seedDemoIfEmpty()
            val me = core.me()
            val source = core.spaces().first { it.members.any { p -> !p.isMe && p.kind == PersonaKind.PERSON } }
            val author = source.members.first { !it.isMe && it.kind == PersonaKind.PERSON }
            val initial = source.copy(id = tag.removePrefix("chat:"), unread = 0u)
            val notifications = ActivityNotifications(context)
            notifications.update(AppState(ready = true, me = me, spaces = listOf(initial)), false, null)
            val incoming = initial.copy(unread = 1u, eventCount = initial.eventCount + 1uL, lastAuthor = author, lastPreview = "Native encrypted message")
            notifications.update(AppState(ready = true, me = me, spaces = listOf(incoming)), false, null)
            waitUntil { manager.activeNotifications.any { it.tag == tag } }
            val posted = manager.activeNotifications.first { it.tag == tag }.notification
            assertEquals(Notification.VISIBILITY_PRIVATE, posted.visibility)
            assertEquals("Native encrypted message", posted.extras.getCharSequence(Notification.EXTRA_TEXT).toString())
            assertEquals(context.getString(R.string.new_activity), posted.publicVersion.extras.getCharSequence(Notification.EXTRA_TEXT).toString())
            notifications.update(AppState(ready = true, me = me, spaces = listOf(incoming.copy(unread = 0u))), true, initial.id)
            waitUntil { manager.activeNotifications.none { it.tag == tag } }
            notifications.update(AppState(ready = true, me = me, spaces = listOf(incoming.copy(eventCount = incoming.eventCount + 1uL))), true, initial.id)
            assertTrue(manager.activeNotifications.none { it.tag == tag })
            val key = "muted.person:${me.id}:${author.id}"
            val existed = preferences.contains(key); val old = preferences.getBoolean(key, false)
            try {
                preferences.edit().putBoolean(key, true).commit()
                notifications.update(AppState(ready = true, me = me, spaces = listOf(incoming.copy(eventCount = incoming.eventCount + 2uL))), false, null)
                assertTrue(manager.activeNotifications.none { it.tag == tag })
            } finally { preferences.edit().apply { if (existed) putBoolean(key, old) else remove(key) }.commit() }
            notifications.update(AppState(ready = true, me = me, spaces = listOf(incoming.copy(eventCount = incoming.eventCount + 3uL))), false, null)
            waitUntil { manager.activeNotifications.any { it.tag == tag } }
            notifications.update(AppState(ready = true, me = me), false, null)
            waitUntil { manager.activeNotifications.none { it.tag == tag } }
            preferences.edit().putBoolean("notifications", false).commit()
            notifications.update(AppState(ready = true, me = me, spaces = listOf(incoming.copy(eventCount = incoming.eventCount + 4uL))), false, null)
            assertTrue(manager.activeNotifications.none { it.tag == tag })
            preferences.edit().putBoolean("notifications", true).commit()
            val foreign = core.requests().first().let { it.copy(id = "foreign-${UUID.randomUUID()}", status = RequestStatus.PENDING, agent = it.agent.copy(isMine = false)) }
            notifications.update(AppState(ready = true, me = me, requests = listOf(foreign)), false, null)
            assertTrue(manager.activeNotifications.none { it.tag == "request:${foreign.id}" })
            assertEquals(0, AppState(requests = listOf(foreign)).pending)
        } finally {
            manager.cancel(tag, 1); core.destroy()
            preferences.edit().apply { if (hadChoice) putBoolean("notifications", oldChoice) else remove("notifications") }.commit()
        }
    }

    private fun waitUntil(test: () -> Boolean) {
        val deadline = android.os.SystemClock.elapsedRealtime() + 5000
        while (!test() && android.os.SystemClock.elapsedRealtime() < deadline) Thread.sleep(25)
        assertTrue(test())
    }
}
