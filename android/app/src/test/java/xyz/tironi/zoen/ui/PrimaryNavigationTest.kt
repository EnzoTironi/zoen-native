package xyz.tironi.zoen.ui

import org.junit.Assert.assertEquals
import org.junit.Test

class PrimaryNavigationTest {
    @Test fun languageHasItsOwnDirectSettingsDestination() {
        assertEquals(xyz.tironi.zoen.R.string.st_language, SettingsPage.Language.title)
        assertEquals(false, SettingsPage.entries.any { it.name == "Preferences" })
    }

    @Test fun communitiesShareChatsAndSecondaryToolsAreNotTabs() {
        assertEquals(listOf(Tab.Chats, Tab.Store), Tab.main)
        assertEquals(Tab.entries.toList(), Tab.main)
    }
}
