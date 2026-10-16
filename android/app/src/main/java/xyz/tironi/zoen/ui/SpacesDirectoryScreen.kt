package xyz.tironi.zoen.ui

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.rounded.KeyboardArrowRight
import androidx.compose.material.icons.rounded.Group
import androidx.compose.material3.*
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import androidx.navigation3.runtime.NavKey
import xyz.tironi.zoen.data.AppState

/** Mirrors SpacesDirectoryScreen: every authorized core space, not a fixture subset. */
@Composable
internal fun SpacesDirectoryScreen(state: AppState, navigate: (NavKey) -> Unit) {
    Scaffold(topBar = { HomeBar("Spaces", state, navigate) }, contentWindowInsets = WindowInsets(0, 0, 0, 0)) { padding ->
        LazyColumn(Modifier.fillMaxSize().padding(padding).testTag("spaces-directory"), contentPadding = PaddingValues(start = 20.dp, end = 20.dp, bottom = 112.dp)) {
            items(state.spaces, key = { it.id }) { space ->
                ListItem(modifier = Modifier.clickable { navigate(Chat(space.id)) }.testTag("space:${space.id}"),
                    headlineContent = { Text(space.title) },
                    leadingContent = { Icon(Icons.Rounded.Group, null, Modifier.size(36.dp), tint = MaterialTheme.colorScheme.primary) },
                    trailingContent = { Icon(Icons.AutoMirrored.Rounded.KeyboardArrowRight, null) })
            }
        }
    }
}
