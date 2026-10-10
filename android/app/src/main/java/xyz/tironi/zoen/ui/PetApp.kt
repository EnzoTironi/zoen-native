package xyz.tironi.zoen.ui

import androidx.compose.animation.*
import androidx.compose.animation.core.*
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.delay
import org.json.JSONObject
import xyz.tironi.zoen.R

@Composable
internal fun PetApp(preferences: android.content.SharedPreferences, itemId: String, data: JSONObject, action: (String, JSONObject) -> Unit, modifier: Modifier) {
    var rename by remember { mutableStateOf(false) }
    var name by rememberSaveable { mutableStateOf("") }
    var tab by rememberSaveable(itemId) { mutableIntStateOf(0) }
    var showEmotes by rememberSaveable(itemId) { mutableStateOf(false) }
    val motion = rememberMotionEnabled()
    val floating = remember(itemId) { mutableStateListOf<Pair<Long, String>>() }
    var nextEmote by remember(itemId) { mutableLongStateOf(0) }
    val haptics = rememberZoenHaptics()
    val fullness = data.optDouble("fullness", 0.0)
    var previousFullness by remember(itemId) { mutableDoubleStateOf(fullness) }
    var eatingSince by remember(itemId) { mutableStateOf<Long?>(null) }
    LaunchedEffect(itemId, fullness) {
        if (fullness > previousFullness + .01) {
            eatingSince = android.os.SystemClock.uptimeMillis()
            floating.add(nextEmote++ to "🥕")
        }
        previousFullness = fullness
    }
    val emoteLabel = stringResource(R.string.miniapp_emotes)
    val asleep = data.optBoolean("asleep")
    val released = data.optBoolean("released")
    val backdrop = listOf(0xFF2C3054 to 0xFFC4D8E2, 0xFF6D5A80 to 0xFFDCDCD6, 0xFFC9A3A0 to 0xFFF2DFCD).map { (night, day) ->
        animateColorAsState(Color(if (asleep) night else day), tween(if (motion) 800 else 0), label = "Pet backdrop").value
    }
    var unboxing by rememberSaveable(itemId) { mutableStateOf(!preferences.getBoolean("RodaUnboxed.$itemId", false) && !released) }
    LaunchedEffect(itemId) { preferences.edit().putBoolean("RodaUnboxed.$itemId", true).apply() }
    val petText = if (asleep) Color.White else Color(0xFF1C1C1E)
    val outlineColors = ButtonDefaults.outlinedButtonColors(containerColor = MaterialTheme.colorScheme.surface,
        contentColor = MaterialTheme.colorScheme.onSurface)
    Column(modifier.fillMaxSize().background(Brush.verticalGradient(backdrop))) {
        Box(Modifier.weight(1f).fillMaxWidth()) {
            when (tab) {
                1 -> DonkeyDash(onClose = { tab = 0 }, onScore = { meters, carrots ->
                    action("pet_dash_score", JSONObject().put("meters", meters).put("carrots", carrots))
                })
                2 -> PetLeaderboard(data, petText)
                else -> LazyColumn(Modifier.fillMaxSize().testTag("pet-app-content"), contentPadding = PaddingValues(24.dp), verticalArrangement = Arrangement.spacedBy(18.dp)) {
                    item {
                        Column(Modifier.fillMaxWidth(), horizontalAlignment = Alignment.CenterHorizontally) {
                            Box(Modifier.fillMaxWidth().height(210.dp), contentAlignment = Alignment.Center) {
                                PixelDonkey(Modifier.size(210.dp), asleep, released, eatingSince = eatingSince)
                                floating.forEach { emote -> key(emote.first) { FloatingPetEmote(emote.second) { floating.remove(emote) } } }
                            }
                            Text(data.optString("name"), style = MaterialTheme.typography.displaySmall, color = petText)
                            Text(data.optString("mood"), color = petText.copy(alpha = .8f))
                            OutlinedButton(onClick = { showEmotes = !showEmotes }, Modifier.testTag("pet-emotes-toggle"), colors = outlineColors) {
                                Icon(Icons.Rounded.EmojiEmotions, null); Spacer(Modifier.width(8.dp)); Text(emoteLabel)
                            }
                            AnimatedVisibility(showEmotes, enter = if (motion) scaleIn() + fadeIn() else EnterTransition.None,
                                exit = if (motion) scaleOut() + fadeOut() else ExitTransition.None) {
                                FlowRow(Modifier.fillMaxWidth().semantics { contentDescription = emoteLabel }, horizontalArrangement = Arrangement.Center) {
                                    listOf("❤️", "🥕", "😂", "🫶", "😴").forEach { emoji -> TextButton(onClick = {
                                        haptics.perform(ZoenFeedback.Selection); floating.add(nextEmote++ to emoji)
                                    }) { Text(emoji, style = MaterialTheme.typography.headlineSmall) } }
                                }
                            }
                        }
                    }
                    item { Card(colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.tertiaryContainer)) {
                        Column(Modifier.fillMaxWidth().padding(20.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
                            listOf("fullness" to R.string.food, "joy" to R.string.mood, "energy" to R.string.rest).forEach { (key, label) ->
                                val progress by animateFloatAsState((data.optDouble(key, 0.0) / 100).toFloat().coerceIn(0f, 1f),
                                    if (motion) spring(dampingRatio = .7f, stiffness = 100f) else tween(0), label = "Pet $key")
                                Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) { Text(stringResource(label)); Text("${(progress * 100).toInt()}%", style = MaterialTheme.typography.labelMedium) }
                                LinearProgressIndicator(progress = { progress }, modifier = Modifier.fillMaxWidth())
                            }
                        }
                    } }
                    if (!released) {
                        item { FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                            FilledTonalButton(onClick = { action("pet_feed", JSONObject()) }, enabled = !asleep) { Text(stringResource(R.string.feed)) }
                            FilledTonalButton(onClick = { action(if (asleep) "pet_wake" else "pet_nap", JSONObject()) }) { Text(stringResource(if (asleep) R.string.wake else R.string.nap)) }
                            FilledTonalButton(onClick = { action("pet_play", JSONObject()) }, enabled = !asleep) { Text(stringResource(R.string.play)) }
                        } }
                        item { OutlinedButton(onClick = { tab = 1 }, Modifier.fillMaxWidth(), enabled = !asleep, colors = outlineColors) { Icon(Icons.Rounded.SportsEsports, null); Spacer(Modifier.width(8.dp)); Text(stringResource(R.string.dash)) } }
                        item { Surface(shape = MaterialTheme.shapes.medium) {
                            Row { TextButton(onClick = { name = data.optString("name"); rename = true }) { Text(stringResource(R.string.rename)) }; Spacer(Modifier.weight(1f)); TextButton(onClick = { action("pet_release", JSONObject()) }) { Text(stringResource(R.string.release_pet), color = MaterialTheme.colorScheme.error) } }
                        } }
                    }
                    item { AppLog(data, petText.copy(alpha = .8f)) }
                }
            }
        }
        NavigationBar(modifier = Modifier.keepSnackbarAbove(), containerColor = if (asleep || tab == 1) Color(0xFF2C3054) else Color(0xFFF2DFCD)) {
            listOf(R.string.pet_care, R.string.play, R.string.miniapp_leaderboard).forEachIndexed { index, label ->
                NavigationBarItem(selected = tab == index, onClick = {
                    if (tab != index) haptics.perform(ZoenFeedback.Selection)
                    tab = index
                }, icon = { Icon(when (index) { 0 -> Icons.Rounded.Favorite; 1 -> Icons.Rounded.SportsEsports; else -> Icons.Rounded.EmojiEvents }, null) },
                    label = { Text(stringResource(label)) }, modifier = Modifier.testTag("pet-tab-$index"),
                    enabled = index != 1 || (!asleep && !released),
                    colors = NavigationBarItemDefaults.colors(selectedTextColor = if (asleep || tab == 1) Color.White else petText,
                        unselectedIconColor = if (asleep || tab == 1) Color.White else petText, unselectedTextColor = if (asleep || tab == 1) Color.White else petText))
            }
        }
    }
    if (unboxing) NativePetReveal(data.optString("name")) { unboxing = false }
    if (rename) AlertDialog(onDismissRequest = { rename = false }, title = { Text(stringResource(R.string.rename)) }, text = { OutlinedTextField(name, { name = it.take(18) }, label = { Text(stringResource(R.string.name)) }) }, confirmButton = { TextButton(onClick = { rename = false; action("pet_rename", JSONObject().put("name", name)) }, enabled = name.isNotBlank()) { Text(stringResource(R.string.save)) } }, dismissButton = { TextButton(onClick = { rename = false }) { Text(stringResource(R.string.cancel)) } })
}

@Composable
private fun PetLeaderboard(data: JSONObject, textColor: Color) {
    val dash = data.optJSONObject("dash") ?: JSONObject()
    val best = dash.optJSONObject("best") ?: JSONObject()
    val people = best.keys().asSequence().toList().sortedByDescending { best.optJSONObject(it)?.optInt("meters") ?: 0 }
    LazyColumn(Modifier.fillMaxSize().testTag("pet-leaderboard"), contentPadding = PaddingValues(24.dp), verticalArrangement = Arrangement.spacedBy(14.dp)) {
        item { Text(stringResource(R.string.pet_group_leaderboard), style = MaterialTheme.typography.headlineMedium, color = textColor) }
        item { Text(pluralStringResource(R.plurals.pet_dash_run_count, dash.optInt("runs"), dash.optInt("runs")), color = textColor.copy(alpha = .8f)) }
        if (people.isEmpty()) item {
            Column(Modifier.fillMaxWidth(), horizontalAlignment = Alignment.CenterHorizontally) {
                ZoenMascot(Modifier.size(110.dp), pose = MascotPose.Zen)
                Text(stringResource(R.string.pet_no_runs), color = textColor,
                    textAlign = androidx.compose.ui.text.style.TextAlign.Center)
            }
        }
        items(people, key = { it }) { person ->
            val rank = people.indexOf(person)
            val score = best.getJSONObject(person)
            Card(colors = CardDefaults.cardColors(containerColor = Color.White.copy(alpha = .75f), contentColor = Color(0xFF1C1C1E))) {
                Row(Modifier.fillMaxWidth().padding(12.dp), horizontalArrangement = Arrangement.spacedBy(12.dp), verticalAlignment = Alignment.CenterVertically) {
                    Text(listOf("🥇", "🥈", "🥉").getOrElse(rank) { "${rank + 1}" }, style = MaterialTheme.typography.titleLarge)
                    Text(person, Modifier.weight(1f), style = MaterialTheme.typography.titleMedium)
                    Column(horizontalAlignment = Alignment.End) {
                        Text(pluralStringResource(R.plurals.dash_distance, score.optInt("meters"), score.optInt("meters")))
                        Text(pluralStringResource(R.plurals.dash_carrot_count, score.optInt("carrots"), score.optInt("carrots")), style = MaterialTheme.typography.bodySmall)
                    }
                }
            }
        }
    }
}

@Composable
fun PixelDonkey(modifier: Modifier = Modifier, asleep: Boolean = false, faded: Boolean = false, live: Boolean = true, eatingSince: Long? = null) {
    ZoenPetSprite(modifier, asleep, faded, live, eatingSince)
}

@Composable
private fun FloatingPetEmote(emoji: String, finished: () -> Unit) {
    val motion = rememberMotionEnabled()
    val progress = remember { Animatable(0f) }
    val drift = remember { kotlin.random.Random.nextFloat() * 100 - 50 }
    val close by rememberUpdatedState(finished)
    val density = androidx.compose.ui.platform.LocalDensity.current.density
    LaunchedEffect(motion) {
        if (motion) { progress.animateTo(1f, tween(1_400)); delay(100) } else delay(1_500)
        close()
    }
    Text(emoji, Modifier.graphicsLayer {
        translationX = drift * density * progress.value
        translationY = -110 * density * progress.value
        alpha = 1 - progress.value
        scaleX = if (motion) .6f + .7f * progress.value else 1f
        scaleY = scaleX
    },
        style = MaterialTheme.typography.displayMedium)
}

