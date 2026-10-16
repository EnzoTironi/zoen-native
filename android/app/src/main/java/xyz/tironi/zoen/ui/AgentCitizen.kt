package xyz.tironi.zoen.ui

import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.background
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.rounded.Chat
import androidx.compose.material.icons.rounded.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import xyz.tironi.zoen.R
import xyz.tironi.zoen.core.AgentProfile
import xyz.tironi.zoen.core.AgentStatus

/* Agents are citizens: a status only their owner reads on the relay, notes waiting for
 * the owner, and a profile like a person's. These composables take plain values so the
 * screens (real core data) and the tests render the same thing. */

private val Active = Color(0xFF22C55E)
private val Amber = Color(0xFFFFB020)

@Composable
fun AgentStatusPill(status: AgentStatus, modifier: Modifier = Modifier) {
    val (label, color) = when (status) {
        AgentStatus.ACTIVE -> R.string.agent_status_active to Active
        AgentStatus.PAUSED -> R.string.agent_status_paused to Amber
        AgentStatus.REVOKED -> R.string.agent_status_revoked to MaterialTheme.colorScheme.outline
    }
    // Word and color change together: the state never rests on color or motion alone.
    Row(
        modifier.clip(CircleShape).background(color.copy(alpha = 0.14f)).padding(horizontal = 10.dp, vertical = 4.dp).testTag("agent-status"),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        Box(Modifier.size(7.dp).clip(CircleShape).background(color))
        AnimatedContent(stringResource(label), transitionSpec = { fadeIn(tween(150)) togetherWith fadeOut(tween(150)) }, label = "status") {
            Text(it, style = MaterialTheme.typography.labelMedium)
        }
    }
}

@Composable
fun AgentPendingBadge(count: Int, modifier: Modifier = Modifier) {
    Text(
        pluralStringResource(R.plurals.agent_waiting_for_you, count, count),
        modifier.clip(CircleShape).background(Amber.copy(alpha = 0.16f)).padding(horizontal = 10.dp, vertical = 4.dp).testTag("agent-pending"),
        style = MaterialTheme.typography.labelMedium,
        color = MaterialTheme.colorScheme.onSurface,
    )
}

@Composable
fun AgentCitizenRow(profile: AgentProfile, status: AgentStatus?, onClick: () -> Unit) {
    val pending = profile.ownerNotesPending.toInt()
    val chats = pluralStringResource(R.plurals.agent_in_chats, profile.spaces.size, profile.spaces.size)
    val subtitle = if (profile.persona.isMine) chats else stringResource(R.string.agent_someones, profile.persona.ownerName ?: stringResource(R.string.agent_someone), chats)
    val press = remember { MutableInteractionSource() }
    GlassCard(
        onClick = onClick,
        interactionSource = press,
        modifier = Modifier.fillMaxWidth().pressScale(press).testTag("agent-row-${profile.persona.handle}"),
        shape = RoundedCornerShape(20.dp),
        colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surfaceContainerLow),
    ) {
        Row(Modifier.padding(14.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(14.dp)) {
            Avatar(profile.persona, size = 52, working = pending > 0)
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                Text(profile.persona.name, style = MaterialTheme.typography.titleMedium)
                Text(subtitle, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant, maxLines = 1)
                if (status != null || pending > 0) Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                    status?.let { AgentStatusPill(it) }
                    if (pending > 0) AgentPendingBadge(pending)
                }
            }
            Icon(Icons.Rounded.ChevronRight, null, tint = MaterialTheme.colorScheme.outline)
        }
    }
}

/** The profile's actions, like a person's: talk, share the stable link, pause or resume,
 *  and turn off for good behind a plain confirmation. */
@Composable
fun AgentCitizenActions(
    profile: AgentProfile,
    status: AgentStatus?,
    link: String?,
    busy: Boolean,
    onChat: () -> Unit,
    onShare: (String) -> Unit,
    onPauseToggle: (pause: Boolean) -> Unit,
    onRevoke: () -> Unit,
) {
    val haptics = rememberZoenHaptics()
    var confirm by remember { mutableStateOf(false) }
    val mine = profile.persona.isMine
    Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
        if (mine && (status != null || profile.ownerNotesPending > 0u)) Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            status?.let { AgentStatusPill(it) }
            if (profile.ownerNotesPending > 0u) AgentPendingBadge(profile.ownerNotesPending.toInt())
        }
        Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
            ActionTile(Icons.AutoMirrored.Rounded.Chat, stringResource(R.string.agent_talk), "agent-chat", enabled = true) { haptics.perform(ZoenFeedback.Tap); onChat() }
            if (mine && status != AgentStatus.REVOKED) {
                ActionTile(Icons.Rounded.IosShare, stringResource(R.string.agent_share_link), "agent-share", enabled = link != null) { link?.let { haptics.perform(ZoenFeedback.Tap); onShare(it) } }
                val paused = status == AgentStatus.PAUSED
                ActionTile(
                    if (paused) Icons.Rounded.PlayArrow else Icons.Rounded.Pause,
                    stringResource(if (paused) R.string.agent_resume else R.string.agent_pause),
                    "agent-pause", enabled = status != null && !busy,
                ) { haptics.perform(ZoenFeedback.Confirm); onPauseToggle(!paused) }
            }
        }
        if (mine && status != null && status != AgentStatus.REVOKED) {
            // Final, so it asks right where you tapped, in plain words (like iOS).
            AnimatedContent(confirm, transitionSpec = { fadeIn(tween(150)) togetherWith fadeOut(tween(150)) }, label = "revoke") { asking ->
                if (!asking) TextButton(onClick = { haptics.perform(ZoenFeedback.Open); confirm = true }, enabled = !busy, modifier = Modifier.testTag("agent-revoke")) {
                    Icon(Icons.Rounded.PowerSettingsNew, null, tint = MaterialTheme.colorScheme.error)
                    Spacer(Modifier.width(8.dp))
                    Text(stringResource(R.string.agent_turn_off), color = MaterialTheme.colorScheme.error)
                } else Surface(color = MaterialTheme.colorScheme.errorContainer, shape = RoundedCornerShape(16.dp), modifier = Modifier.fillMaxWidth().testTag("agent-revoke-ask")) {
                    Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                        Text(stringResource(R.string.agent_turn_off_question, profile.persona.name), style = MaterialTheme.typography.titleMedium, color = MaterialTheme.colorScheme.onErrorContainer)
                        Text(stringResource(R.string.agent_turn_off_detail), style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onErrorContainer)
                        Row(Modifier.align(Alignment.End), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                            TextButton(onClick = { confirm = false; haptics.perform(ZoenFeedback.Dismiss) }) { Text(stringResource(R.string.cancel)) }
                            GlassButton(
                                onClick = { confirm = false; haptics.perform(ZoenFeedback.Reject); onRevoke() },
                                colors = ButtonDefaults.buttonColors(containerColor = MaterialTheme.colorScheme.error, contentColor = MaterialTheme.colorScheme.onError),
                                modifier = Modifier.testTag("agent-revoke-confirm"),
                            ) { Text(stringResource(R.string.agent_turn_off)) }
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun RowScope.ActionTile(icon: ImageVector, label: String, tag: String, enabled: Boolean, onClick: () -> Unit) {
    val press = remember { MutableInteractionSource() }
    FilledTonalButton(
        onClick = onClick, enabled = enabled, interactionSource = press, shape = RoundedCornerShape(16.dp),
        contentPadding = PaddingValues(vertical = 12.dp),
        modifier = Modifier.weight(1f).pressScale(press).testTag(tag).semantics { contentDescription = label },
    ) {
        Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(4.dp)) {
            AnimatedContent(icon, transitionSpec = { fadeIn(tween(150)) togetherWith fadeOut(tween(150)) }, label = "icon") { Icon(it, null) }
            Text(label, style = MaterialTheme.typography.labelMedium, maxLines = 1)
        }
    }
}

/** Press scales to 0.96 over 150 ms; nothing when the system says remove animations. */
@Composable
private fun Modifier.pressScale(source: MutableInteractionSource): Modifier {
    val pressed by source.collectIsPressedAsState()
    val still = reduceMotion(LocalView.current)
    val scale by animateFloatAsState(if (pressed && !still) 0.96f else 1f, tween(150), label = "press")
    return graphicsLayer { scaleX = scale; scaleY = scale }
}
