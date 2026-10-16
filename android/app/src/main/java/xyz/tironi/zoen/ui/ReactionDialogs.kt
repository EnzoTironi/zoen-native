package xyz.tironi.zoen.ui

import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.CancellationException
import xyz.tironi.zoen.ZoenViewModel
import xyz.tironi.zoen.core.TimelineEntry

/** The keyboard can enter any emoji, including skin tones and joined families. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
internal fun ReactionPickerDialog(model: ZoenViewModel, spaceId: String, entry: TimelineEntry, onSaved: (() -> Unit)? = null, close: () -> Unit) {
    var emoji by remember(entry.id) { mutableStateOf("") }
    var busy by remember(entry.id) { mutableStateOf(false) }
    var failed by remember(entry.id) { mutableStateOf(false) }
    val haptics = rememberZoenHaptics()
    val mine = entry.reactions.firstOrNull { it.mine }?.emoji
    fun react(value: String?) {
        if (busy) return
        busy = true
        failed = false
        haptics.perform(ZoenFeedback.Selection)
        model.launch {
            try {
                model.repository.change { it.react(spaceId, entry.id, value) }
                (onSaved ?: close)()
            } catch (error: Exception) {
                if (error is CancellationException) throw error
                failed = true
                haptics.perform(ZoenFeedback.Reject)
            } finally { busy = false }
        }
    }
    ModalBottomSheet(onDismissRequest = close, sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true)) {
        Column(Modifier.fillMaxWidth().padding(horizontal = 24.dp).padding(bottom = 24.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Text("Escolher reação", style = MaterialTheme.typography.titleLarge)
            listOf(listOf("😀", "🥰", "😍", "🥳"), listOf("🤔", "😭", "👏", "🎉"), listOf("🔥", "💯", "✅", "👀")).forEach { row ->
                Row { row.forEach { value ->
                    TextButton(onClick = { react(if (value == mine) null else value) }, enabled = !busy,
                        modifier = Modifier.semantics { contentDescription = if (value == mine) "Remover $value" else "Reagir com $value" }) { Text(value) }
                } }
            }
            OutlinedTextField(emoji, { emoji = it }, enabled = !busy, label = { Text("Emoji do teclado") }, singleLine = true, modifier = Modifier.fillMaxWidth().testTag("reaction-emoji"))
            Text("Use o teclado de emojis para escolher qualquer reação.")
            if (failed) Text("Não foi possível salvar sua reação. Tente de novo.", color = MaterialTheme.colorScheme.error)
            if (busy) LinearProgressIndicator(Modifier.fillMaxWidth().semantics { contentDescription = "Salvando reação" })
            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.End) {
                TextButton(onClick = close) { Text("Cancelar") }
                TextButton(onClick = { react(if (emoji.trim() == mine) null else emoji.trim()) }, enabled = !busy && emoji.isNotBlank() && emoji.trim().toByteArray(Charsets.UTF_8).size <= 32) { Text("Reagir") }
            }
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
internal fun ReactionPeopleDialog(model: ZoenViewModel, spaceId: String, entry: TimelineEntry, close: () -> Unit) {
    var selectedEmoji by remember(entry.id) { mutableStateOf(entry.reactions.firstOrNull()?.emoji) }
    var choosingEmoji by remember(entry.id) { mutableStateOf(false) }
    var busy by remember(entry.id) { mutableStateOf(false) }
    var failed by remember(entry.id) { mutableStateOf(false) }
    val reaction = entry.reactions.firstOrNull { it.emoji == selectedEmoji } ?: entry.reactions.firstOrNull()
    val mine = entry.reactions.firstOrNull { it.mine }
    val haptics = rememberZoenHaptics()
    fun react(value: String?) {
        if (busy) return
        busy = true
        failed = false
        haptics.perform(ZoenFeedback.Selection)
        model.launch {
            try {
                model.repository.change { it.react(spaceId, entry.id, value) }
                close()
            } catch (error: Exception) {
                if (error is CancellationException) throw error
                failed = true
                haptics.perform(ZoenFeedback.Reject)
            } finally { busy = false }
        }
    }
    ModalBottomSheet(onDismissRequest = close, sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true)) {
        LazyColumn(Modifier.fillMaxWidth().padding(horizontal = 24.dp), contentPadding = PaddingValues(bottom = 24.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            item {
                Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.SpaceBetween) {
                    Text("Reações", style = MaterialTheme.typography.titleLarge)
                    TextButton(onClick = close) { Text("Fechar") }
                }
            }
            item {
                Row(Modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    entry.reactions.forEach { item ->
                        FilterChip(selected = item.emoji == reaction?.emoji, enabled = !busy, onClick = {
                            haptics.perform(ZoenFeedback.Selection)
                            selectedEmoji = item.emoji
                        }, label = { Text("${item.emoji} ${item.people.size}") }, modifier = Modifier.semantics { contentDescription = "${item.emoji}, ${item.people.size} participantes" })
                    }
                }
            }
            if (reaction != null) {
                item { Text("Quem reagiu com ${reaction.emoji}", style = MaterialTheme.typography.titleSmall) }
                reaction.people.forEach { person ->
                    item(key = "person:${person.id}") {
                        Row(Modifier.fillMaxWidth().padding(vertical = 4.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                            Avatar(person, size = 36)
                            Text(person.name, modifier = Modifier.weight(1f))
                            if (person.isMe) Text("Você", style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
                        }
                    }
                }
                item {
                    TextButton(onClick = { react(if (reaction.mine) null else reaction.emoji) }, enabled = !busy) {
                        Text(if (reaction.mine) "Remover minha reação" else "Reagir com ${reaction.emoji}")
                    }
                    TextButton(onClick = { haptics.perform(ZoenFeedback.Open); choosingEmoji = true }, enabled = !busy) {
                        Text(if (mine == null) "Escolher outra reação" else "Trocar minha reação")
                    }
                    if (mine != null && !reaction.mine) TextButton(onClick = { react(null) }, enabled = !busy) { Text("Remover minha reação", color = MaterialTheme.colorScheme.error) }
                }
            }
            if (busy) item { LinearProgressIndicator(Modifier.fillMaxWidth().semantics { contentDescription = "Salvando reação" }) }
            if (failed) item { Text("Não foi possível salvar sua reação. Tente de novo.", color = MaterialTheme.colorScheme.error) }
        }
    }
    if (choosingEmoji) ReactionPickerDialog(model, spaceId, entry, onSaved = { choosingEmoji = false; close() }) { choosingEmoji = false }
}
