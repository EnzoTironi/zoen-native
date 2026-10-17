package xyz.tironi.zoen.ui

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import xyz.tironi.zoen.ZoenViewModel
import xyz.tironi.zoen.core.*
import xyz.tironi.zoen.data.AppState

/** Human-only member information; no legacy agent, mini-app, or key-management controls. */
@Composable
fun LivingParticipantsScreen(model: ZoenViewModel, state: AppState, spaceId: String, back: () -> Unit) {
    val space by produceState<SpaceSummary?>(null, spaceId, state.revision) { value = try { model.repository.query { it.space(spaceId) } } catch (_: Exception) { null } }
    Scaffold(topBar = { ScreenBar("Members", back) }) { padding ->
        LazyColumn(Modifier.padding(padding).fillMaxSize(), contentPadding = PaddingValues(20.dp)) {
            items(space?.members.orEmpty().filter { it.kind == PersonaKind.PERSON }, key = { it.id }) { person ->
                ListItem(headlineContent = { Text(person.name) }, leadingContent = { Avatar(person, size = 44) })
            }
        }
    }
}

@Composable
fun LivingPersonSummary(state: AppState, personId: String, back: () -> Unit) {
    val person = state.spaces.asSequence().flatMap { it.members }.firstOrNull { it.id == personId && it.kind == PersonaKind.PERSON }
    Scaffold(topBar = { ScreenBar(person?.name ?: "Person", back) }) { padding ->
        Column(Modifier.padding(padding).padding(24.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            if (person != null) { Avatar(person, size = 72); Text(person.name, style = MaterialTheme.typography.titleLarge); Text("@${person.handle}", color = MaterialTheme.colorScheme.onSurfaceVariant) }
            else Text("This profile is not available.")
        }
    }
}
