#!/usr/bin/env python3
"""Source contracts only; integrated Compose/runtime checks remain required."""
from pathlib import Path
import unittest
ROOT = Path(__file__).resolve().parents[1]
UI = ROOT / 'android/app/src/main/java/xyz/tironi/zoen/ui'

class HomeSearchContract(unittest.TestCase):
    def test_three_tabs_and_actual_marketplace(self):
        nav = (UI / 'Navigation.kt').read_text()
        self.assertIn('enum class Tab { Chats, Marketplace, Profile;', nav)
        app = (UI / 'ZoenApp.kt').read_text()
        self.assertIn('Tab.Marketplace -> MiniAppStoreScreen(model, state,', app)
        self.assertNotIn('Tab.Spaces', app)
        self.assertNotIn('Tab.Notifications', app)

    def test_notifications_first_even_zero_and_no_apps(self):
        home = (UI / 'HomeScreens.kt').read_text()
        self.assertIn('onOpenNotifications = { navigate(Activity) }', home)
        self.assertNotIn('apps.isNotEmpty() && !showArchived', home)
        tiles = (ROOT / 'android/app/src/main/java/xyz/tironi/zoen/miniapps/MiniAppTiles.kt').read_text()
        self.assertLess(tiles.index('item(key = "home-notifications")'), tiles.index('itemsIndexed(tiles'))
        self.assertIn('Modifier.size(170.dp).testTag("home-notifications")', tiles)
        self.assertIn('Text(state.pending.toString()', tiles)

    def test_person_starts_real_dm_not_arbitrary_shared_group(self):
        search = (UI / 'SearchScreen.kt').read_text()
        start = search[search.index('fun openPerson('):search.index('fun openHit(')]
        self.assertIn('it.counterpart?.id == person.id', start)
        self.assertIn('model.repository.change { it.startDirect(person.id) }', start)
        self.assertNotIn('space.members.any', start)
        self.assertIn('it.findPeople(trimmedQuery)', search)
        self.assertIn('it.universalSearch(trimmedQuery', search)
        self.assertIn('items(matchingChats', search)
        self.assertIn('items(matchingPeople', search)

if __name__ == '__main__':
    unittest.main()
