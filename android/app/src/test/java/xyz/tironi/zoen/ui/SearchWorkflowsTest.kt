package xyz.tironi.zoen.ui

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json
import org.junit.Assert.*
import org.junit.Test
import xyz.tironi.zoen.core.AppSpecDto

class SearchWorkflowsTest {
    private fun app() = AppSpecDto("hike", "Trilha", "Planeje uma ação na floresta com árvores.", "ui://hike", true, emptyList())

    @Test fun scopedSearchTargetsTheSameRealCoreKindsAndExpandsSeeAll() {
        assertEquals(listOf("message"), SearchScope.Messages.kinds)
        assertEquals(listOf("person"), SearchScope.People.kinds)
        assertEquals(listOf("agent"), SearchScope.Agents.kinds)
        assertEquals(listOf("space"), SearchScope.Spaces.kinds)
        assertEquals(listOf("app"), SearchScope.Apps.kinds)
        assertEquals(listOf("item"), SearchScope.Files.kinds)
        assertTrue(SearchScope.All.kinds.isEmpty())
        assertEquals(4u, SearchScope.All.limit)
        assertEquals(40u, SearchScope.Apps.limit)
    }

    @Test fun realCatalogSupportsAccentsCaseAndAllPrefixTerms() {
        assertTrue(catalogSearchMatches("ACAO arv", app()))
        assertTrue(catalogSearchMatches("Tril flo", app()))
        assertFalse(catalogSearchMatches("acao praia", app()))
    }

    @Test fun localizedCatalogCanStillBeFoundByItsStableEnglishAppId() {
        assertTrue(catalogSearchMatches("hike", app()))
        assertTrue(catalogSearchMatches("hik acao", app()))
    }

    @Test fun catalogDoesNotMatchMidWordFragmentsOrPunctuationOnlyQueries() {
        assertFalse(catalogSearchMatches("resta", app()))
        assertFalse(catalogSearchMatches("!!!", app()))
        assertFalse(catalogSearchMatches("", app()))
    }

    @Test fun selectingAnExistingRecentMakesItNewestWithoutCaseDuplicates() {
        val recents = updatedSearchRecents(listOf("Marina", "Paraty", "Hike"), "  paraty  ")
        assertEquals(listOf("paraty", "Marina", "Hike"), recents)
    }

    @Test fun savedRecentHistoryRetainsOnlyTheLatestEightQueries() {
        var recents = emptyList<String>()
        repeat(12) { recents = updatedSearchRecents(recents, "trip " + it) }
        assertEquals(8, recents.size)
        assertEquals("trip 11", recents.first())
        assertEquals("trip 4", recents.last())
        assertEquals(recents, decodeSearchRecents(Json.encodeToString(recents)))
    }

    @Test fun blankAndSingleCodePointQueriesDoNotReplaceUsefulHistory() {
        val history = listOf("Marina")
        assertEquals(history, updatedSearchRecents(history, " \n "))
        assertEquals(history, updatedSearchRecents(history, "M"))
        assertEquals(history, updatedSearchRecents(history, "🧭"))
    }

    @Test fun malformedStoredHistoryCanBeClearedAndDoesNotBreakSearch() {
        assertTrue(decodeSearchRecents("{\"broken\":true}").isEmpty())
        assertTrue(decodeSearchRecents(null).isEmpty())
        assertTrue(decodeSearchRecents("[]").isEmpty())
    }

    @Test fun multipleHighlightedMatchesPreserveUnicodeAndExactVisibleRanges() {
        val result = highlightedSearchText("🧭 [[ação]] com [[árvores]]", Color.Blue)
        assertEquals("🧭 ação com árvores", result.text)
        assertEquals(listOf("ação", "árvores"), result.spanStyles.map { result.text.substring(it.start, it.end) })
        assertTrue(result.spanStyles.all { it.item.fontWeight == FontWeight.Bold && it.item.color == Color.Blue })
    }

    @Test fun unmatchedMarkersStayVisibleAndPlainTextNeedsNoHighlightRanges() {
        assertEquals("An [[unfinished match", highlightedSearchText("An [[unfinished match", Color.Blue).text)
        val plain = highlightedSearchText("Nothing marked here.", Color.Blue)
        assertEquals("Nothing marked here.", plain.text)
        assertTrue(plain.spanStyles.isEmpty())
    }
}
