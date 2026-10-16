package xyz.tironi.zoen.ui

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import xyz.tironi.zoen.ZoenViewModel
import xyz.tironi.zoen.core.TimelineEntry

/** The keyboard can enter any emoji, including skin tones and joined families. */
@Composable
internal fun ReactionPickerDialog(model: ZoenViewModel, spaceId: String, entry: TimelineEntry, close: () -> Unit) {
    var emoji by remember(entry.id) { mutableStateOf("") }
    val mine = entry.reactions.firstOrNull { it.mine }?.emoji
    fun react(value: String?) {
        model.launch { model.repository.change { it.react(spaceId, entry.id, value) }; close() }
    }
    AlertDialog(onDismissRequest = close, title = { Text("Escolher reação") }, text = {
        Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
            listOf(listOf("😀", "🥰", "😍", "🥳"), listOf("🤔", "😭", "👏", "🎉"), listOf("🔥", "💯", "✅", "👀")).forEach { row ->
                Row { row.forEach { value -> TextButton(onClick = { react(if (value == mine) null else value) }) { Text(value) } } }
            }
            OutlinedTextField(emoji, { emoji = it }, label = { Text("Emoji do teclado") }, singleLine = true, modifier = Modifier.fillMaxWidth().testTag("reaction-emoji"))
            Text("Use o teclado de emojis para escolher qualquer reação.")
        }
    }, confirmButton = { TextButton(onClick = { react(if (emoji.trim() == mine) null else emoji.trim()) }, enabled = emoji.isNotBlank() && emoji.trim().toByteArray(Charsets.UTF_8).size <= 32) { Text("Reagir") } },
        dismissButton = { TextButton(onClick = close) { Text("Cancelar") } })
}

@Composable
internal fun ReactionPeopleDialog(model: ZoenViewModel, spaceId: String, entry: TimelineEntry, close: () -> Unit) {
    AlertDialog(onDismissRequest = close, title = { Text("Reações") }, text = {
        LazyColumn {
            entry.reactions.forEach { reaction ->
                item(key = reaction.emoji) {
                    Column(Modifier.fillMaxWidth().padding(vertical = 8.dp)) {
                        Text(reaction.emoji)
                        reaction.people.forEach { person -> Text(person.name) }
                        if (reaction.mine) TextButton(onClick = {
                            model.launch { model.repository.change { it.react(spaceId, entry.id, null) }; close() }
                        }) { Text("Remover minha reação") }
                    }
                }
            }
        }
    }, confirmButton = { TextButton(onClick = close) { Text("Fechar") } })
}
