package xyz.tironi.zoen.ui

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import androidx.navigation3.runtime.NavKey
import kotlinx.coroutines.delay
import xyz.tironi.zoen.R
import xyz.tironi.zoen.ZoenViewModel
import xyz.tironi.zoen.core.*
import xyz.tironi.zoen.data.AppState

@Composable
fun SearchScreen(model: ZoenViewModel, state: AppState, navigate: (NavKey) -> Unit, back: () -> Unit) {
    var query by rememberSaveable { mutableStateOf("") }
    val focus = remember { FocusRequester() }
    val results by produceState<List<UniversalHit>>(emptyList(), query, state.revision) {
        if (query.isBlank()) value = emptyList()
        else { delay(220); value = model.repository.query { it.universalSearch(query, emptyList(), 25u).hits } }
    }
    LaunchedEffect(Unit) { focus.requestFocus() }
    Scaffold(topBar = { ScreenBar(stringResource(R.string.search), back) }) { padding ->
        Column(Modifier.padding(padding).imePadding()) {
            OutlinedTextField(query, { query = it }, Modifier.fillMaxWidth().padding(horizontal = 24.dp, vertical = 8.dp).focusRequester(focus).testTag("search"),
                placeholder = { Text(stringResource(R.string.search_hint)) }, leadingIcon = { Icon(Icons.Rounded.Search, null) }, singleLine = true,
                trailingIcon = { if (query.isNotEmpty()) IconButton(onClick = { query = "" }) { Icon(Icons.Rounded.Close, stringResource(R.string.close)) } },
            )
            LazyColumn(contentPadding = PaddingValues(16.dp)) {
                if (query.isBlank()) item { EmptyState(stringResource(R.string.search_empty), stringResource(R.string.search_empty_detail)) }
                else if (results.isEmpty()) item { EmptyState(stringResource(R.string.no_results), stringResource(R.string.no_results_detail)) }
                items(results, key = { "${it.kind}:${it.refId}" }) { hit ->
                    val route: NavKey? = when (hit.kind) {
                        "app", "item" -> Item(hit.refId)
                        "space" -> Chat(hit.refId)
                        "agent" -> Agent(hit.refId)
                        "person" -> Person(hit.refId)
                        "message" -> hit.spaceId?.let { Chat(it, hit.refId) }
                        else -> null
                    }
                    ListItem(headlineContent = { Text(hit.title.replace("[[", "").replace("]]", "")) }, supportingContent = { Text(hit.snippet.replace("[[", "").replace("]]", ""), maxLines = 3) },
                        leadingContent = { if (hit.persona != null) Avatar(hit.persona, size = 40) else Icon(appIcon(hit.kind), null) },
                        modifier = Modifier.testTag("search-result:${hit.refId}").clickable(enabled = route != null) { route?.let(navigate) },
                    )
                }
            }
        }
    }
}
