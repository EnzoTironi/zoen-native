package xyz.tironi.zoen.ui

import androidx.compose.foundation.layout.*
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.Check
import androidx.compose.material.icons.rounded.FilterList
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.unit.dp
import xyz.tironi.zoen.R
import xyz.tironi.zoen.core.SpaceKindDto
import xyz.tironi.zoen.core.SpaceSummary

internal enum class ChatInboxFilter(val title: Int) {
    All(R.string.chat_filter_all), Direct(R.string.chat_filter_direct), Groups(R.string.chat_filter_groups), Communities(R.string.chat_filter_communities);
    fun includes(space: SpaceSummary) = when (this) {
        All -> true; Direct -> space.kind == SpaceKindDto.DIRECT; Groups -> space.kind == SpaceKindDto.GROUP; Communities -> space.kind == SpaceKindDto.COMMUNITY
    }
}

@Composable
internal fun ChatFilterMenu(filter: ChatInboxFilter, select: (ChatInboxFilter) -> Unit) {
    var open by remember { mutableStateOf(false) }
    val title = stringResource(filter.title)
    Box(Modifier.padding(horizontal = 12.dp)) {
        TextButton({ open = true }, Modifier.heightIn(min = 48.dp).testTag("chat-filter").semantics { stateDescription = title }) {
            Icon(Icons.Rounded.FilterList, stringResource(R.string.chat_filter)); Spacer(Modifier.width(6.dp)); Text(title)
        }
        DropdownMenu(open, { open = false }) {
            ChatInboxFilter.entries.forEach { candidate ->
                DropdownMenuItem(text = { Text(stringResource(candidate.title)) }, onClick = { open = false; if (candidate != filter) select(candidate) },
                    leadingIcon = { if (candidate == filter) Icon(Icons.Rounded.Check, null) }, modifier = Modifier.testTag("chat-filter-${candidate.name.lowercase()}").semantics { stateDescription = if (candidate == filter) title else "" })
            }
        }
    }
}
