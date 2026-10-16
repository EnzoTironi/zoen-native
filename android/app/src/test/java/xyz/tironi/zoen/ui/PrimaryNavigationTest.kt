package xyz.tironi.zoen.ui

import org.junit.Assert.assertEquals
import org.junit.Test

class PrimaryNavigationTest {
    @Test fun communitiesShareChatsAndSecondaryToolsAreNotTabs() {
        assertEquals(listOf(Tab.Chats, Tab.Store), Tab.main)
        assertEquals(Tab.entries.toList(), Tab.main)
    }
}
