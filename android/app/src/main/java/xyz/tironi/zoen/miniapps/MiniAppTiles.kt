package xyz.tironi.zoen.miniapps

import android.animation.ValueAnimator
import android.view.HapticFeedbackConstants
import androidx.activity.compose.BackHandler
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.tween
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.gestures.detectDragGesturesAfterLongPress
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.rounded.ArrowBack
import androidx.compose.material.icons.automirrored.rounded.ArrowForward
import androidx.compose.material.icons.rounded.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.boundsInWindow
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.CustomAccessibilityAction
import androidx.compose.ui.semantics.customActions
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import kotlinx.coroutines.launch
import xyz.tironi.zoen.R
import xyz.tironi.zoen.ZoenViewModel
import xyz.tironi.zoen.core.ItemDetail
import xyz.tironi.zoen.data.AppState
import xyz.tironi.zoen.ui.ItemTile
import xyz.tironi.zoen.ui.MiniAppScreen
import xyz.tironi.zoen.ui.ScreenBar

data class MiniAppTile(val key: String, val item: ItemDetail, val snapshot: WidgetSnapshot?)
private data class AppFlip(val item: ItemDetail, val from: Rect, val snapshot: WidgetSnapshot?)

/** Shared by Chats and the chat header. Long press, drag or accessibility actions edit local pins. */
@OptIn(ExperimentalFoundationApi::class)
@Composable
fun MiniAppTileStrip(
    model: ZoenViewModel,
    state: AppState,
    items: List<ItemDetail>,
    modifier: Modifier = Modifier,
    scope: String = MiniAppPins.HOME,
    compact: Boolean = false,
    onOpenItem: (String) -> Unit = {},
) {
    val preferences = model.repository.preferences
    val pins = remember(preferences) { MiniAppPins(preferences) }
    var revision by remember { mutableIntStateOf(0) }
    var editing by rememberSaveable(scope) { mutableStateOf(false) }
    var remove by remember { mutableStateOf<MiniAppTile?>(null) }
    var flip by remember { mutableStateOf<AppFlip?>(null) }
    val view = LocalView.current
    val density = LocalDensity.current
    DisposableEffect(preferences, scope) {
        val listener = android.content.SharedPreferences.OnSharedPreferenceChangeListener { _, key -> if (key?.startsWith("miniapps.") == true) revision++ }
        preferences.registerOnSharedPreferenceChangeListener(listener)
        onDispose { preferences.unregisterOnSharedPreferenceChangeListener(listener) }
    }
    val isChat = scope.startsWith("chat:")
    val raw = items.flatMap { item ->
        val snapshots = if (isChat) MiniAppSnapshots.tiles(item, model.repository.locale) else listOfNotNull(WidgetSnapshot.from(item))
        if (snapshots.isEmpty()) listOf(MiniAppTile(item.id, item, null))
        else snapshots.mapIndexed { index, snapshot -> MiniAppTile(if (isChat) "${item.id}#$index" else item.id, item, snapshot) }
    }
    val tiles = remember(raw, revision, state.revision) { pins.visible(raw, MiniAppTile::key, scope) }
    val moveLeft = stringResource(R.string.miniapp_move_left)
    val moveRight = stringResource(R.string.miniapp_move_right)
    val unpin = stringResource(R.string.unpin)
    val open = stringResource(R.string.miniapp_open)
    fun move(tile: MiniAppTile, distance: Int) {
        val ids = tiles.map { it.key }.toMutableList()
        val from = ids.indexOf(tile.key)
        val to = (from + distance).coerceIn(0, ids.lastIndex)
        if (from >= 0 && from != to) {
            ids.add(to, ids.removeAt(from)); pins.reorder(ids, scope); revision++
            view.performHapticFeedback(HapticFeedbackConstants.CLOCK_TICK)
        }
    }
    Column(modifier) {
        if (editing) Row(Modifier.fillMaxWidth().padding(horizontal = 24.dp), horizontalArrangement = Arrangement.End) {
            TextButton(onClick = { editing = false }) { Text(stringResource(R.string.done)) }
        }
        LazyRow(contentPadding = PaddingValues(horizontal = if (isChat) 16.dp else 24.dp, vertical = 8.dp), horizontalArrangement = Arrangement.spacedBy(12.dp)) {
            itemsIndexed(tiles, key = { _, tile -> tile.key }) { index, tile ->
                var bounds by remember(tile.key) { mutableStateOf(Rect.Zero) }
                var drag by remember(tile.key) { mutableFloatStateOf(0f) }
                var dragging by remember(tile.key) { mutableStateOf(false) }
                val shared = Modifier.onGloballyPositioned { bounds = it.boundsInWindow() }
                    .testTag("miniapp-tile:${tile.key}")
                    .alpha(if (flip?.item?.id == tile.item.id) 0f else 1f)
                    .graphicsLayer { translationX = drag; scaleX = if (dragging) 1.05f else 1f; scaleY = scaleX }
                    .combinedClickable(onClick = {
                        if (!editing) {
                            view.performHapticFeedback(HapticFeedbackConstants.CONTEXT_CLICK)
                            if (compact || bounds == Rect.Zero) onOpenItem(tile.item.id) else flip = AppFlip(tile.item, bounds, tile.snapshot)
                        }
                    }, onLongClick = { editing = true; view.performHapticFeedback(HapticFeedbackConstants.LONG_PRESS) })
                    .pointerInput(editing, tile.key, index, tiles.map { it.key }) {
                        if (editing) detectDragGesturesAfterLongPress(onDragStart = { dragging = true }, onDragCancel = { drag = 0f; dragging = false }, onDragEnd = { drag = 0f; dragging = false }) { change, delta ->
                            change.consume(); drag += delta.x
                            val threshold = with(density) { 92.dp.toPx() }
                            if (drag > threshold && index < tiles.lastIndex) { move(tile, 1); drag = 0f }
                            else if (drag < -threshold && index > 0) { move(tile, -1); drag = 0f }
                        }
                    }.semantics {
                        customActions = listOf(CustomAccessibilityAction(moveLeft) { move(tile, -1); true }, CustomAccessibilityAction(moveRight) { move(tile, 1); true }, CustomAccessibilityAction(unpin) { remove = tile; true })
                    }
                Column(shared.width(if (compact) 170.dp else 180.dp)) {
                    if (compact) AssistChip(onClick = { onOpenItem(tile.item.id) }, label = { Text(tile.snapshot?.title ?: tile.item.title) })
                    else if (tile.snapshot != null) SnapshotCard(tile.snapshot, Modifier.fillMaxWidth().height((180f + (density.fontScale - 1).coerceAtLeast(0f) * 120f).dp))
                    else ItemTile(tile.item, { if (!editing) flip = AppFlip(tile.item, bounds, null) })
                    if (editing) Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) {
                        IconButton(onClick = { move(tile, -1) }, enabled = index > 0) { Icon(Icons.AutoMirrored.Rounded.ArrowBack, moveLeft) }
                        IconButton(onClick = { remove = tile }) { Icon(Icons.Rounded.RemoveCircleOutline, unpin) }
                        IconButton(onClick = { move(tile, 1) }, enabled = index < tiles.lastIndex) { Icon(Icons.AutoMirrored.Rounded.ArrowForward, moveRight) }
                    }
                }
            }
        }
    }
    remove?.let { tile -> AlertDialog(onDismissRequest = { remove = null }, title = { Text(stringResource(R.string.miniapp_unpin_question, tile.snapshot?.title ?: tile.item.title)) },
        text = { Text(stringResource(R.string.miniapp_unpin_detail)) }, confirmButton = { TextButton(onClick = { pins.hide(tile.item.id, scope); revision++; remove = null }) { Text(unpin) } },
        dismissButton = { TextButton(onClick = { remove = null }) { Text(stringResource(R.string.cancel)) } }) }
    flip?.let { selected -> MiniAppFlipHost(model, state, selected.item.id, selected.from, selected.snapshot, onClosed = { flip = null }) }
}

@Composable
fun MiniAppFlipHost(model: ZoenViewModel, state: AppState, itemId: String, source: Rect, front: WidgetSnapshot?, onClosed: () -> Unit) {
    val item = state.items.firstOrNull { it.id == itemId }
    val progress = remember(itemId) { Animatable(0f) }
    val coroutine = rememberCoroutineScope()
    var closing by remember { mutableStateOf(false) }
    val animations = ValueAnimator.areAnimatorsEnabled()
    val density = LocalDensity.current
    val close = {
        if (!closing) {
            closing = true
            coroutine.launch { progress.animateTo(0f, tween(if (animations) 320 else 140)); onClosed() }
        }
    }
    LaunchedEffect(itemId) { progress.animateTo(1f, tween(if (animations) 440 else 180)) }
    Dialog(onDismissRequest = close, properties = DialogProperties(usePlatformDefaultWidth = false, decorFitsSystemWindows = false)) {
        BackHandler { close() }
        BoxWithConstraints(Modifier.fillMaxSize()) {
            val targetWidth = with(density) { maxWidth.toPx() }
            val targetHeight = with(density) { maxHeight.toPx() }
            val p = progress.value
            val width = source.width + (targetWidth - source.width) * p
            val height = source.height + (targetHeight - source.height) * p
            val x = source.left * (1 - p)
            val y = source.top * (1 - p)
            val transform = if (animations) Modifier.graphicsLayer {
                translationX = x; translationY = y
                transformOrigin = androidx.compose.ui.graphics.TransformOrigin(0f, 0f)
                scaleX = (width / targetWidth).coerceAtLeast(.001f); scaleY = (height / targetHeight).coerceAtLeast(.001f)
                rotationY = if (p < .5f) p * 180 else (p - 1) * 180
                cameraDistance = 18 * density.density
                clip = true; shape = androidx.compose.foundation.shape.RoundedCornerShape((24 * (1 - p)).dp)
            } else Modifier.alpha(p)
            Box(Modifier.fillMaxSize().then(transform)) {
                if (animations && p < .5f && front != null) SnapshotCard(front, Modifier.fillMaxSize(), live = false)
                else Scaffold(topBar = { ScreenBar(item?.title ?: stringResource(R.string.live_apps), close, actions = {
                    if (item != null) MiniAppDetailsButton(model, state, item)
                }) }) { padding ->
                    if (item != null) MiniAppScreen(model, state, item, Modifier.fillMaxSize().padding(padding), onClose = close)
                }
            }
        }
    }
}
