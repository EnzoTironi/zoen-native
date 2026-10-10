package xyz.tironi.zoen.ui

import android.app.Activity
import android.content.Context
import android.content.ContextWrapper
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.VectorConverter
import androidx.compose.animation.core.spring
import androidx.compose.animation.core.tween
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.detectDragGestures
import androidx.compose.foundation.gestures.detectHorizontalDragGestures
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.input.pointer.PointerInputChange
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.CustomAccessibilityAction
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.customActions
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.repeatOnLifecycle
import androidx.navigation3.runtime.NavKey
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlin.math.abs
import kotlin.math.hypot
import xyz.tironi.zoen.R
import xyz.tironi.zoen.ZoenViewModel
import xyz.tironi.zoen.core.*
import xyz.tironi.zoen.data.AppState
import xyz.tironi.zoen.ui.ink.ZoenGlyph

internal enum class ApprovalDirection(val decision: RequestDecision, val label: Int, val glyph: ZoenGlyph, val threshold: Float) {
    Right(RequestDecision.APPROVE, R.string.approve, ZoenGlyph.Check, 110f),
    Left(RequestDecision.DENY, R.string.deny, ZoenGlyph.Close, 110f),
    Up(RequestDecision.ALWAYS_APPROVE, R.string.always_approve, ZoenGlyph.Check, 170f),
    Down(RequestDecision.ALWAYS_DENY, R.string.always_deny, ZoenGlyph.Lock, 170f);

    val standing get() = this == Up || this == Down
    fun distance(offset: Offset): Float = if (standing) abs(offset.y) else abs(offset.x)
    companion object {
        fun from(offset: Offset): ApprovalDirection? = when {
            hypot(offset.x, offset.y) < 8f -> null
            abs(offset.y) > abs(offset.x) * 1.15f -> if (offset.y < 0) Up else Down
            offset.x > 0 -> Right
            else -> Left
        }
    }
}

@Composable
internal fun ApprovalsDeck(model: ZoenViewModel, app: AppState, navigate: (NavKey) -> Unit, onShowList: () -> Unit, modifier: Modifier = Modifier) {
    val review by model.approvals.state.collectAsStateWithLifecycle()
    val queue = app.requests.filter { it.agent.isMine && it.status == RequestStatus.PENDING && it.id !in review.hidden }.sortedBy { it.openedMs }
    val motion = rememberMotionEnabled()
    val scroll = rememberScrollState()
    val haptics = rememberZoenHaptics()
    val lifecycle = LocalLifecycleOwner.current
    val activity = LocalContext.current.activity()
    var help by rememberSaveable { mutableStateOf(false) }
    var seenCards by rememberSaveable(app.me?.id) { mutableStateOf(false) }
    var leaving by remember(app.me?.id) { mutableStateOf<AgentRequestDto?>(null) }
    var returned by remember(app.me?.id) { mutableStateOf<Pair<String, RequestDecision>?>(null) }
    var undoFlight by remember(app.me?.id) { mutableIntStateOf(0) }
    val displayed = leaving?.let { listOf(it) + queue.filter { next -> next.id != it.id } } ?: queue
    LaunchedEffect(queue.isNotEmpty()) { if (queue.isNotEmpty()) seenCards = true }
    LaunchedEffect(model.approvals, lifecycle) {
        lifecycle.lifecycle.repeatOnLifecycle(Lifecycle.State.RESUMED) {
            model.approvals.committed.collect { haptics.perform(ZoenFeedback.Confirm) }
        }
    }
    DisposableEffect(model.approvals, lifecycle, activity) {
        val observer = LifecycleEventObserver { _, event ->
            if (event == Lifecycle.Event.ON_STOP && activity?.isChangingConfigurations != true) model.approvals.flush()
        }
        lifecycle.lifecycle.addObserver(observer)
        onDispose {
            lifecycle.lifecycle.removeObserver(observer)
            if (activity?.isChangingConfigurations != true) model.approvals.flush()
        }
    }
    BoxWithConstraints(modifier.fillMaxSize().testTag("approval-deck")) {
        val compact = maxHeight < 520.dp || LocalDensity.current.fontScale > 1.3f
        val cardViewport = (maxHeight - 76.dp).coerceAtLeast(0.dp)
        val verticalViewportPx = with(LocalDensity.current) { cardViewport.toPx() }
        LaunchedEffect(displayed.firstOrNull()?.id, undoFlight, compact) {
            if (compact) {
                if (motion) scroll.animateScrollTo(0) else scroll.scrollTo(0)
            }
        }
        Column(Modifier.fillMaxSize().then(if (compact) Modifier.verticalScroll(scroll) else Modifier).padding(horizontal = 20.dp, vertical = 8.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                val total = review.decided + queue.size
                LinearProgressIndicator(progress = { if (total == 0) 0f else review.decided.toFloat() / total }, modifier = Modifier.weight(1f).testTag("approval-progress"))
                InkIconButton(ZoenGlyph.Info, stringResource(R.string.approval_gesture_help), { help = true })
                TextButton(onClick = onShowList, modifier = Modifier.testTag("activity-show-list")) { Text(stringResource(R.string.approval_list)) }
            }
            if (displayed.isEmpty()) {
                Column(Modifier.fillMaxWidth().then(if (compact) Modifier.heightIn(min = 280.dp) else Modifier.weight(1f)), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.Center) {
                    ZoenMascot(Modifier.size(150.dp), pose = MascotPose.Zen)
                    Text(stringResource(if (seenCards) R.string.all_clear else R.string.approval_empty), style = MaterialTheme.typography.headlineSmall)
                    Text(stringResource(R.string.approval_empty_detail), modifier = Modifier.padding(top = 12.dp), color = MaterialTheme.colorScheme.onSurfaceVariant)
                }
            } else {
                key(displayed.first().id, undoFlight) {
                    ApprovalStack(displayed, model, app.keyMissing, motion, compact, verticalViewportPx, returned?.takeIf { it.first == displayed.first().id }?.second, navigate, { leaving = it }, if (compact) Modifier.fillMaxWidth() else Modifier.weight(1f).fillMaxWidth())
                }
            }
            review.pending?.let { pending ->
                Surface(color = MaterialTheme.colorScheme.inverseSurface, shape = RoundedCornerShape(20.dp), modifier = Modifier.fillMaxWidth().testTag("approval-undo-toast")) {
                    Row(Modifier.padding(start = 16.dp, end = 8.dp).semantics { liveRegion = LiveRegionMode.Polite }, verticalAlignment = Alignment.CenterVertically) {
                        val label = when (pending.decision) {
                            RequestDecision.APPROVE -> R.string.status_approved
                            RequestDecision.DENY -> R.string.status_denied
                            RequestDecision.ALWAYS_APPROVE -> R.string.always_approve
                            RequestDecision.ALWAYS_DENY -> R.string.always_deny
                        }
                        Text(stringResource(label) + " · " + pending.request.title, modifier = Modifier.weight(1f), maxLines = 2, overflow = TextOverflow.Ellipsis)
                        TextButton(onClick = {
                            if (model.approvals.undo()) {
                                leaving = null
                                returned = pending.request.id to pending.decision
                                undoFlight++
                                haptics.perform(ZoenFeedback.Dismiss)
                            }
                        }, modifier = Modifier.testTag("approval-undo"), colors = ButtonDefaults.textButtonColors(contentColor = MaterialTheme.colorScheme.inversePrimary)) { Text(stringResource(R.string.undo)) }
                    }
                }
            }
            Spacer(Modifier.height(12.dp))
        }
    }
    if (help) AlertDialog(onDismissRequest = { help = false }, title = { Text(stringResource(R.string.approval_gesture_help)) }, text = { Text(stringResource(R.string.approval_gesture_detail)) }, confirmButton = { TextButton(onClick = { help = false }) { Text(stringResource(R.string.done)) } })
}

@Composable
private fun ApprovalStack(queue: List<AgentRequestDto>, model: ZoenViewModel, keyMissing: Boolean, motion: Boolean, compact: Boolean, verticalViewportPx: Float, returned: RequestDecision?, navigate: (NavKey) -> Unit, leaving: (AgentRequestDto?) -> Unit, modifier: Modifier) {
    val top = queue.first()
    val density = LocalDensity.current
    val scope = rememberCoroutineScope()
    val haptics = rememberZoenHaptics()
    val offset = remember(top.id) {
        val origin = if (motion) with(density) {
            when (returned) {
                RequestDecision.APPROVE -> Offset(500.dp.toPx(), 0f)
                RequestDecision.DENY -> Offset(-500.dp.toPx(), 0f)
                RequestDecision.ALWAYS_APPROVE -> Offset(0f, -700.dp.toPx())
                RequestDecision.ALWAYS_DENY -> Offset(0f, 700.dp.toPx())
                else -> Offset.Zero
            }
        } else Offset.Zero
        Animatable(origin, Offset.VectorConverter)
    }
    val opacity = remember(top.id) { Animatable(1f) }
    val entrance = remember(top.id) { Animatable(if (motion) .94f else 1f) }
    var drag by remember(top.id) { mutableStateOf(Offset.Zero) }
    var dragging by remember(top.id) { mutableStateOf(false) }
    var busy by remember(top.id) { mutableStateOf(false) }
    var stamping by remember(top.id) { mutableStateOf<ApprovalDirection?>(null) }
    var crossed by remember(top.id) { mutableStateOf<ApprovalDirection?>(null) }
    var lockedHint by remember(top.id) { mutableStateOf(false) }
    var extent by remember { mutableFloatStateOf(1_300f) }
    var flight by remember(top.id) { mutableStateOf<Job?>(null) }
    LaunchedEffect(top.id, motion) {
        if (motion) {
            launch { entrance.animateTo(1f, spring(dampingRatio = .7f, stiffness = 220f)) }
            offset.animateTo(Offset.Zero, spring(dampingRatio = .7f, stiffness = 360f))
        } else {
            if (busy) {
                flight?.cancel()
                busy = false; stamping = null; crossed = null
                leaving(null)
            }
            entrance.snapTo(1f); offset.snapTo(Offset.Zero)
        }
    }
    val raw = if (dragging) drag else offset.value
    val leading = ApprovalDirection.from(raw / density.density)
    val tint = when (stamping ?: leading) {
        ApprovalDirection.Left -> MaterialTheme.colorScheme.error
        ApprovalDirection.Down -> MaterialTheme.colorScheme.onSurface
        ApprovalDirection.Up -> MaterialTheme.colorScheme.tertiary
        else -> MaterialTheme.colorScheme.primary
    }
    val onTint = when (stamping ?: leading) {
        ApprovalDirection.Left -> MaterialTheme.colorScheme.onError
        ApprovalDirection.Down -> MaterialTheme.colorScheme.surface
        ApprovalDirection.Up -> MaterialTheme.colorScheme.onTertiary
        else -> MaterialTheme.colorScheme.onPrimary
    }
    fun returnCard() {
        scope.launch {
            offset.snapTo(drag); dragging = false
            if (motion) offset.animateTo(Offset.Zero, spring(dampingRatio = .7f, stiffness = 360f)) else offset.snapTo(Offset.Zero)
            drag = Offset.Zero; crossed = null
        }
    }
    fun decide(direction: ApprovalDirection) {
        if (busy || keyMissing) return
        if (direction == ApprovalDirection.Up && !top.canAlwaysApprove) {
            lockedHint = true
            if (crossed != direction) haptics.perform(ZoenFeedback.Reject)
            returnCard(); return
        }
        busy = true
        leaving(top)
        if (!model.approvals.queue(top.id, direction.decision)) { leaving(null); busy = false; returnCard(); return }
        flight = scope.launch {
            try {
                offset.snapTo(drag); dragging = false
                haptics.perform(ZoenFeedback.Drop)
                if (direction.standing) {
                    stamping = direction
                    if (motion) delay(450)
                }
                if (motion) {
                    val target = when (direction) {
                        ApprovalDirection.Right -> Offset(extent, raw.y + extent * .08f)
                        ApprovalDirection.Left -> Offset(-extent, raw.y + extent * .08f)
                        ApprovalDirection.Up -> Offset(raw.x + extent * .04f, -extent)
                        ApprovalDirection.Down -> Offset(raw.x + extent * .04f, extent)
                    }
                    offset.animateTo(target, tween(300))
                } else opacity.snapTo(0f)
                offset.snapTo(Offset.Zero); opacity.snapTo(1f)
            } finally {
                drag = Offset.Zero; crossed = null; stamping = null; busy = false
                leaving(null)
            }
        }
    }
    val labels = ApprovalDirection.entries.associateWith { stringResource(it.label) }
    var cardHeight by remember(top.id) { mutableIntStateOf(0) }
    val verticalDecisions = density.fontScale <= 1.3f && (!compact || cardHeight <= verticalViewportPx)
    val gesture = Modifier.pointerInput(top.id, keyMissing, motion, verticalDecisions) {
        val start: (Offset) -> Unit = { if (!busy && !keyMissing) { scope.launch { offset.stop() }; dragging = true; drag = offset.value; crossed = null } }
        val cancel: () -> Unit = { if (!busy) returnCard() }
        val end: () -> Unit = {
            if (!busy && dragging) {
                val dp = drag / density.density
                val direction = ApprovalDirection.from(dp)
                if (direction != null && direction.distance(dp) >= direction.threshold) decide(direction) else returnCard()
            }
        }
        val move: (PointerInputChange, Offset) -> Unit = { change, amount ->
            if (dragging && !busy) {
                change.consume(); drag += amount
                val dp = drag / density.density
                val direction = ApprovalDirection.from(dp)?.takeIf { it.distance(dp) >= it.threshold }
                if (direction != null && crossed != direction) haptics.perform(if (direction == ApprovalDirection.Up && !top.canAlwaysApprove) ZoenFeedback.Reject else ZoenFeedback.Selection)
                crossed = direction
            }
        }
        if (verticalDecisions) detectDragGestures(onDragStart = start, onDragCancel = cancel, onDragEnd = end, onDrag = move)
        else detectHorizontalDragGestures(onDragStart = start, onDragCancel = cancel, onDragEnd = end, onHorizontalDrag = { change, amount -> move(change, Offset(amount, 0f)) })
    }
    Column(modifier, verticalArrangement = Arrangement.spacedBy(16.dp)) {
        BoxWithConstraints((if (compact) Modifier else Modifier.weight(1f)).fillMaxWidth()) {
            SideEffect { extent = with(density) { maxOf(maxWidth, maxHeight).coerceAtMost(900.dp).toPx() * 1.6f } }
            queue.take(3).withIndex().reversed().forEach { (depth, request) ->
                val isTop = depth == 0
                val cardModifier = if (isTop) (if (compact) Modifier.fillMaxWidth() else Modifier.fillMaxSize()).onSizeChanged { cardHeight = it.height }.testTag("approval-card-top").then(gesture)
                    .clickable(enabled = !busy && !keyMissing) { haptics.perform(ZoenFeedback.Open); model.approvals.flush(); navigate(Request(request.id)) }
                    .semantics { customActions = ApprovalDirection.entries.filter { it != ApprovalDirection.Up || top.canAlwaysApprove }.map { direction -> CustomAccessibilityAction(labels.getValue(direction)) { if (busy || keyMissing) false else { decide(direction); true } } } }
                else Modifier.matchParentSize().clearAndSetSemantics { }
                Surface(cardModifier.graphicsLayer {
                    translationX = if (isTop) raw.x else 0f
                    translationY = if (isTop) raw.y else depth * 8.dp.toPx()
                    rotationZ = if (isTop && motion) (raw.x / density.density / 28f).coerceIn(-14f, 14f) else 0f
                    scaleX = if (isTop) entrance.value else 1f - depth * .035f
                    scaleY = scaleX
                    alpha = if (isTop) opacity.value else 1f
                }, shape = RoundedCornerShape(28.dp), color = MaterialTheme.colorScheme.surfaceContainerLow, shadowElevation = if (isTop) 6.dp else 1.dp) {
                    if (isTop) Column(Modifier.fillMaxWidth().padding(24.dp), verticalArrangement = Arrangement.spacedBy(14.dp)) {
                        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                            Avatar(request.agent, size = 44)
                            Column { Text(request.agent.name, style = MaterialTheme.typography.titleSmall); Text(request.spaceTitle, style = MaterialTheme.typography.bodySmall) }
                        }
                        Text(request.title, style = MaterialTheme.typography.headlineSmall)
                        Text(request.detail, style = MaterialTheme.typography.bodyLarge, maxLines = 4, overflow = TextOverflow.Ellipsis)
                        request.costCents?.let { Text(formatMoney(it, model.repository.locale), style = MaterialTheme.typography.headlineMedium, color = MaterialTheme.colorScheme.primary) }
                        if (!compact) Spacer(Modifier.weight(1f))
                        HorizontalDivider()
                        Text(request.reason, style = MaterialTheme.typography.bodySmall, maxLines = 3, overflow = TextOverflow.Ellipsis)
                        Text(stringResource(R.string.review), style = MaterialTheme.typography.labelLarge, color = MaterialTheme.colorScheme.primary)
                    } else Spacer(Modifier.fillMaxSize())
                }
            }
            if (leading != null || stamping != null) Surface(Modifier.align(Alignment.Center).graphicsLayer { rotationZ = -8f }, color = tint, shape = RoundedCornerShape(16.dp)) {
                Text(labels.getValue(stamping ?: leading!!), Modifier.padding(20.dp), color = onTint, style = MaterialTheme.typography.titleLarge)
            }
        }
        if (lockedHint) Text(stringResource(R.string.approval_always_asks), color = MaterialTheme.colorScheme.error, modifier = Modifier.semantics { liveRegion = LiveRegionMode.Polite })
        val actions = listOf(ApprovalDirection.Left, ApprovalDirection.Right, ApprovalDirection.Down, ApprovalDirection.Up)
        val rows = if (density.fontScale > 1.3f) actions.map { listOf(it) } else actions.chunked(2)
        for (row in rows) {
            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                row.forEach { direction ->
                    OutlinedButton(onClick = { decide(direction) }, enabled = !busy && !keyMissing && (direction != ApprovalDirection.Up || top.canAlwaysApprove), modifier = Modifier.weight(1f).heightIn(min = 48.dp).testTag("approval-${direction.name.lowercase()}"), contentPadding = PaddingValues(horizontal = 12.dp, vertical = 12.dp)) {
                        ZoenIcon(direction.glyph)
                        Spacer(Modifier.width(8.dp))
                        Text(labels.getValue(direction))
                    }
                }
            }
        }
    }
}

private tailrec fun Context.activity(): Activity? = when (this) {
    is Activity -> this
    is ContextWrapper -> baseContext.activity()
    else -> null
}
