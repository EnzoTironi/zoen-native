package xyz.tironi.zoen.ui

import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.FastOutSlowInEasing
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.blur
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import org.json.JSONObject
import xyz.tironi.zoen.R
import xyz.tironi.zoen.core.Persona

/** A note your own agent sends you. The core decides whether a message is one
 *  (`RodaEngine.ownerAgentNote`: the author is your agent with a verified ownership proof);
 *  the app only renders the JSON it hands back. */
sealed interface OwnerAgentNote {
    val space: String
    data class Ask(override val space: String, val from: String, val draft: String) : OwnerAgentNote
    data class Proposal(override val space: String, val name: String, val handle: String) : OwnerAgentNote
    /** A client booked, cancelled, or asked to cancel ([kind] new / cancelled / cancel_request). */
    data class Booking(override val space: String, val kind: String, val client: String, val service: String, val time: String) : OwnerAgentNote

    companion object {
        fun parse(json: String): OwnerAgentNote? = runCatching {
            val root = JSONObject(json)
            root.optJSONObject("ask_owner")?.let {
                return@runCatching Ask(it.optString("space"), it.optString("from"), it.optString("draft"))
            }
            root.optJSONObject("agent_proposal")?.let {
                val spec = it.optJSONObject("spec") ?: JSONObject()
                return@runCatching Proposal(it.optString("space"), spec.optString("name"), spec.optString("handle"))
            }
            root.optJSONObject("booking")?.let {
                return@runCatching Booking(it.optString("space"), it.optString("kind"), it.optString("client"), it.optString("service"), it.optString("when"))
            }
            null
        }.getOrNull()
    }
}

/** The card. [fresh]: the note arrived just now, so it enters once (older ones are just there). */
@Composable
fun OwnerAgentNoteCard(agent: Persona, note: OwnerAgentNote, personName: (String) -> String?, fresh: Boolean, open: (String) -> Unit, modifier: Modifier = Modifier) {
    val motion = rememberMotionEnabled()
    val haptics = rememberZoenHaptics()
    val enter = remember { Animatable(if (fresh) 0f else 1f) }
    LaunchedEffect(Unit) {
        if (enter.value < 1f) {
            haptics.perform(ZoenFeedback.Open)
            enter.animateTo(1f, tween(if (motion) 300 else 200, easing = FastOutSlowInEasing))
        }
    }
    val p = enter.value
    Surface(
        shape = RoundedCornerShape(28.dp),
        color = MaterialTheme.colorScheme.surface,
        tonalElevation = 1.dp,
        shadowElevation = 3.dp,
        modifier = modifier
            .widthIn(max = 340.dp)
            .testTag("owner-note")
            .graphicsLayer { alpha = p; if (motion) translationY = (1f - p) * 12.dp.toPx() }
            .then(if (motion && p < 1f) Modifier.blur(((1f - p) * 4).dp) else Modifier),
    ) {
        Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                Avatar(agent, size = 28, contact = true)
                Column {
                    Text(stringResource(when (note) {
                        is OwnerAgentNote.Ask -> R.string.owner_note_ask_title
                        is OwnerAgentNote.Proposal -> R.string.owner_note_proposal_title
                        is OwnerAgentNote.Booking -> when (note.kind) {
                            "cancelled" -> R.string.owner_note_booking_cancelled_title
                            "cancel_request" -> R.string.owner_note_booking_cancel_request_title
                            else -> R.string.owner_note_booking_title
                        }
                    }),
                        style = MaterialTheme.typography.titleSmall, fontWeight = FontWeight.SemiBold)
                    Text(agent.name, style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
                }
            }
            when (note) {
                is OwnerAgentNote.Ask -> {
                    val who = personName(note.from) ?: stringResource(R.string.owner_note_someone)
                    Text(stringResource(R.string.owner_note_ask_body, who), style = MaterialTheme.typography.bodyMedium)
                    if (note.draft.isNotBlank()) Column(
                        Modifier.fillMaxWidth().background(MaterialTheme.colorScheme.surfaceVariant, RoundedCornerShape(12.dp)).padding(12.dp),
                        verticalArrangement = Arrangement.spacedBy(4.dp),
                    ) {
                        Text(stringResource(R.string.owner_note_draft), style = MaterialTheme.typography.labelMedium, fontWeight = FontWeight.SemiBold, color = MaterialTheme.colorScheme.onSurfaceVariant)
                        Text(note.draft, style = MaterialTheme.typography.bodyMedium, maxLines = 6, overflow = TextOverflow.Ellipsis)
                    }
                }
                is OwnerAgentNote.Proposal -> {
                    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        Text(note.name.ifBlank { note.handle }, style = MaterialTheme.typography.bodyMedium, fontWeight = FontWeight.SemiBold)
                        if (note.handle.isNotBlank()) Text("@${note.handle}", style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
                    }
                    Text(stringResource(R.string.owner_note_proposal_body), style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
                }
                is OwnerAgentNote.Booking -> {
                    val who = note.client.ifBlank { stringResource(R.string.owner_note_someone_cap) }
                    Text(stringResource(when (note.kind) {
                        "cancelled" -> R.string.owner_note_booking_cancelled_body
                        "cancel_request" -> R.string.owner_note_booking_cancel_request_body
                        else -> R.string.owner_note_booking_body
                    }, who, note.service, note.time),
                        style = MaterialTheme.typography.bodyMedium)
                }
            }
            val press = remember { MutableInteractionSource() }
            val pressed by press.collectIsPressedAsState()
            val enabled = note.space.isNotBlank()
            val scale by animateFloatAsState(if (pressed && enabled && motion) .96f else 1f, tween(150), label = "press")
            GlassButton(
                onClick = { haptics.perform(ZoenFeedback.Tap); open(note.space) },
                enabled = enabled,
                interactionSource = press,
                shape = RoundedCornerShape(12.dp),
                modifier = Modifier.fillMaxWidth().graphicsLayer { scaleX = scale; scaleY = scale }.testTag("owner-note-open"),
            ) { Text(stringResource(R.string.owner_note_open)) }
        }
    }
}
