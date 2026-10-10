package xyz.tironi.zoen.ui

import android.Manifest
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.net.Uri
import android.os.Build
import androidx.activity.compose.BackHandler
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.PickVisualMediaRequest
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.rounded.ArrowBack
import androidx.compose.material.icons.rounded.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import java.util.Locale
import kotlinx.coroutines.CancellationException
import xyz.tironi.zoen.BuildConfig
import xyz.tironi.zoen.R
import xyz.tironi.zoen.ZoenViewModel
import xyz.tironi.zoen.agent.PlanDraft
import xyz.tironi.zoen.agent.ProfilePhotoCrop
import xyz.tironi.zoen.agent.ProfilePhotos
import xyz.tironi.zoen.core.PhotoChange
import xyz.tironi.zoen.core.TrustLevelDto
import xyz.tironi.zoen.data.FileAccess

@OptIn(ExperimentalLayoutApi::class)
@Composable
fun Onboarding(model: ZoenViewModel, modifier: Modifier = Modifier, acquisitionLink: String? = null, finished: (String?) -> Unit = {}) {
    var route by remember { mutableStateOf(OnboardingRoute()) }
    var position by rememberSaveable { mutableIntStateOf(0) }
    var selected by rememberSaveable { mutableStateOf(listOf<Int>()) }
    var trust by rememberSaveable { mutableIntStateOf(2) }
    var name by rememberSaveable { mutableStateOf(model.state.value.me?.name.orEmpty()) }
    var handle by rememberSaveable { mutableStateOf(model.state.value.me?.handle.orEmpty()) }
    var relay by rememberSaveable { mutableStateOf(BuildConfig.RELAY_URL) }
    var connectionSettings by rememberSaveable { mutableStateOf(false) }
    var busy by remember { mutableStateOf(false) }
    var planning by remember { mutableStateOf(false) }
    var savedDraft by rememberSaveable { mutableStateOf<String?>(null) }
    var plan by remember { mutableStateOf(savedDraft?.let(OnboardingDraft::decode)) }
    var photoPath by rememberSaveable { mutableStateOf<String?>(null) }
    var cropUri by rememberSaveable { mutableStateOf<String?>(null) }
    val photo = remember(photoPath) { photoPath?.let { path -> runCatching { java.io.File(path).takeIf { it.isFile && it.length() <= 5 * 1024 * 1024 }?.readBytes() }.getOrNull() } }
    var crop by remember { mutableStateOf<Bitmap?>(null) }
    var cameraUri by rememberSaveable { mutableStateOf<String?>(null) }
    val context = LocalContext.current
    val photoError = stringResource(R.string.something_wrong)
    val pasteError = stringResource(R.string.onboarding_paste_error)
    val language = Locale.forLanguageTag(model.repository.locale).language
    val step = route.steps[position.coerceAtMost(route.steps.lastIndex)]
    fun next() { position = (position + 1).coerceAtMost(route.steps.lastIndex) }
    val notifications = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { granted ->
        model.repository.preferences.edit().putBoolean("notifications", granted).apply(); next()
    }
    val location = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { granted ->
        model.repository.preferences.edit().putBoolean("location", granted).apply(); next()
    }
    fun readPhoto(uri: Uri) { cropUri = uri.toString() }
    LaunchedEffect(cropUri) { cropUri?.let {
        try { crop = ProfilePhotos.read(context, Uri.parse(it)) }
        catch (error: Exception) { if (error is CancellationException) throw error; cropUri = null; model.notify(error.message ?: photoError) }
    } }
    val photoPicker = rememberLauncherForActivityResult(ActivityResultContracts.PickVisualMedia()) { it?.let(::readPhoto) }
    val camera = rememberLauncherForActivityResult(ActivityResultContracts.TakePicture()) { success -> if (success) cameraUri?.let { readPhoto(Uri.parse(it)) } }
    val areaResources = listOf(R.string.life_travel, R.string.life_money, R.string.life_home, R.string.life_food, R.string.life_friends, R.string.life_work, R.string.life_health, R.string.life_family)
    val areaLabels = areaResources.map { stringResource(it) }
    val areas = selected.mapNotNull { areaLabels.getOrNull(it) }.ifEmpty { listOf(areaLabels[0], areaLabels[5]) }
    val prompt = stringResource(R.string.first_prompt) + ": " + areas.joinToString(", ")
    LaunchedEffect(acquisitionLink) {
        route = model.repository.query { core ->
            acquisitionLink?.let { core.growthCaptureLink(it) }
            OnboardingRoute.from(core.growthOnboardingPlan())
        }
    }
    LaunchedEffect(model) {
        try {
            model.repository.network { it.growthSync(relay, false, 0u, 0u) }
            if (position == 0) route = model.repository.query { OnboardingRoute.from(it.growthOnboardingPlan()) }
        } catch (e: Exception) { if (e is CancellationException) throw e }
    }
    LaunchedEffect(step, selected) {
        if (step == OnboardingStep.Plan && !planning && (plan == null || plan!!.plan.sections.map { it.title } != areas)) {
            planning = true
            try { plan = model.planner.makeStarterPlan(areas, model.repository.locale); savedDraft = OnboardingDraft.encode(checkNotNull(plan)) }
            finally { planning = false }
        }
    }
    BackHandler(enabled = position > 0 && !busy) { position-- }
    val pose = when (step) {
        OnboardingStep.Hello -> MascotPose.Wave
        OnboardingStep.Profile, OnboardingStep.Notifications -> MascotPose.Phone
        OnboardingStep.Areas -> MascotPose.Map
        OnboardingStep.Plan -> MascotPose.Run
        OnboardingStep.Agents -> MascotPose.Juggle
        OnboardingStep.Location -> MascotPose.Walk
        OnboardingStep.Done -> MascotPose.Cheer
    }
    Column(modifier.fillMaxSize().background(if (isSystemInDarkTheme()) MaterialTheme.colorScheme.background else Color(0xFFFFFCF5)).safeDrawingPadding().imePadding().testTag("onboarding:${step.id}")) {
        Row(Modifier.fillMaxWidth().padding(horizontal = 12.dp), verticalAlignment = Alignment.CenterVertically) {
            IconButton(onClick = { position-- }, enabled = position > 0 && !busy) { Icon(Icons.AutoMirrored.Rounded.ArrowBack, stringResource(R.string.back)) }
            Row(Modifier.weight(1f).padding(end = 28.dp), horizontalArrangement = Arrangement.spacedBy(5.dp)) {
                repeat(route.steps.size) { index -> LinearProgressIndicator(progress = { if (index <= position) 1f else 0f }, modifier = Modifier.weight(1f).height(4.dp)) }
            }
        }
        Column(Modifier.weight(1f).verticalScroll(rememberScrollState()).widthIn(max = 560.dp).align(Alignment.CenterHorizontally).padding(horizontal = 28.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
            ZoenMascot(Modifier.fillMaxWidth().height(if (step == OnboardingStep.Profile) 130.dp else 230.dp), animated = true, pose = pose)
            Text(route.text("onboarding.${step.id}.title", language) ?: stringResource(step.title), Modifier.fillMaxWidth(), style = MaterialTheme.typography.headlineMedium, fontWeight = FontWeight.ExtraBold, textAlign = TextAlign.Center)
            val detail = route.text("onboarding.${step.id}.body", language) ?: if (step == OnboardingStep.Profile) null else stringResource(step.detail)
            detail?.let { Text(it, Modifier.fillMaxWidth(), style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant, textAlign = TextAlign.Center) }
            when (step) {
                OnboardingStep.Hello -> if (route.flow == "default") {
                    OutlinedButton(onClick = {
                        val clipboard = context.getSystemService(android.content.ClipboardManager::class.java)
                        val invite = clipboard.primaryClip?.takeIf { it.itemCount > 0 }?.getItemAt(0)?.coerceToText(context)?.toString()?.trim()
                        if (invite.isNullOrEmpty() || invite.length > 8192 || Uri.parse(invite).scheme !in listOf("https", "zoen")) model.notify(pasteError)
                        else model.launch {
                            route = model.repository.query { core ->
                                core.growthCaptureLink(invite)
                                OnboardingRoute.from(core.growthOnboardingPlan())
                            }
                        }
                    }, modifier = Modifier.align(Alignment.CenterHorizontally).testTag("onboarding-paste-invite")) {
                        Icon(Icons.Rounded.ContentPaste, null, Modifier.size(18.dp))
                        Spacer(Modifier.width(8.dp))
                        Text(stringResource(R.string.onboarding_paste_invite))
                    }
                }
                OnboardingStep.Profile -> {
                    val preview = remember(photo) { photo?.let { BitmapFactory.decodeByteArray(it, 0, it.size) } }
                    DisposableEffect(preview) { onDispose { preview?.recycle() } }
                    preview?.let { Image(it.asImageBitmap(), stringResource(R.string.agent_photo_crop), Modifier.size(100.dp).align(Alignment.CenterHorizontally)) }
                    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        TextButton(onClick = { photoPicker.launch(PickVisualMediaRequest(ActivityResultContracts.PickVisualMedia.ImageOnly)) }) { Text(stringResource(R.string.agent_photo_choose)) }
                        TextButton(onClick = { cameraUri = FileAccess.cameraUri(context).toString(); camera.launch(Uri.parse(cameraUri)) }) { Text(stringResource(R.string.agent_photo_camera)) }
                    }
                    OutlinedTextField(name, { name = it }, label = { Text(stringResource(R.string.name)) }, singleLine = true, modifier = Modifier.fillMaxWidth().testTag("onboarding-name"), keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.Words))
                    OutlinedTextField(handle, { handle = it.lowercase(Locale.ROOT).filter { c -> c in 'a'..'z' || c in '0'..'9' || c == '_' || c == '.' }.take(24) }, label = { Text(stringResource(R.string.handle)) }, prefix = { Text("@") }, supportingText = { Text(stringResource(R.string.handle_rules)) }, singleLine = true, modifier = Modifier.fillMaxWidth().testTag("onboarding-handle"))
                    TextButton(onClick = { connectionSettings = !connectionSettings }) { Text(stringResource(R.string.connection_settings)) }
                    if (connectionSettings) OutlinedTextField(relay, { relay = it }, label = { Text(stringResource(R.string.relay)) }, singleLine = true, modifier = Modifier.fillMaxWidth().testTag("onboarding-relay"))
                }
                OnboardingStep.Areas -> FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    areaLabels.forEachIndexed { index, label -> FilterChip(selected = index in selected, onClick = { selected = if (index in selected) selected - index else selected + index }, label = { Text(label) }) }
                }
                OnboardingStep.Plan -> if (planning || plan == null) {
                    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) { CircularProgressIndicator(Modifier.size(24.dp)); Text(stringResource(R.string.onboarding_planning)) }
                } else Card(colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.primaryContainer)) {
                    Column(Modifier.padding(20.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                        Text(plan!!.plan.title, style = MaterialTheme.typography.titleLarge)
                        plan!!.plan.sections.forEach { section ->
                            Text(section.title, style = MaterialTheme.typography.titleSmall)
                            section.lines.forEach { line -> Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) { Icon(Icons.Rounded.CheckCircleOutline, null, Modifier.size(18.dp)); Text(line.text) } }
                        }
                        Text(plan!!.engineLabel, style = MaterialTheme.typography.labelSmall)
                    }
                }
                OnboardingStep.Agents -> {
                    val levels = listOf(R.string.listen to R.string.listen_detail, R.string.suggest to R.string.suggest_detail, R.string.act to R.string.act_detail, R.string.autonomous to R.string.autonomous_detail)
                    levels.forEachIndexed { index, labels -> Row(Modifier.fillMaxWidth().selectable(trust == index, role = Role.RadioButton, onClick = { trust = index }).padding(vertical = 5.dp), verticalAlignment = Alignment.CenterVertically) {
                        RadioButton(trust == index, onClick = null)
                        Column(Modifier.padding(start = 12.dp)) { Text(stringResource(labels.first), style = MaterialTheme.typography.titleSmall); Text(stringResource(labels.second), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant) }
                    } }
                }
                else -> Unit
            }
        }
        Column(Modifier.widthIn(max = 560.dp).fillMaxWidth().align(Alignment.CenterHorizontally).padding(24.dp), verticalArrangement = Arrangement.spacedBy(4.dp), horizontalAlignment = Alignment.CenterHorizontally) {
            Button(onClick = {
                when (step) {
                    OnboardingStep.Profile -> {
                        busy = true
                        model.launch {
                            try {
                                if (model.state.value.account == null) model.repository.createAccount(name, handle, relay)
                                val bio = model.state.value.me?.bio.orEmpty()
                                model.repository.change { it.updateProfile(name.trim(), handle, bio); it.updateMyProfile(name.trim(), bio, photo?.let { bytes -> PhotoChange.Set(bytes, "image/jpeg") } ?: PhotoChange.Keep) }
                                next()
                            } finally { busy = false }
                        }
                    }
                    OnboardingStep.Notifications -> if (Build.VERSION.SDK_INT >= 33) notifications.launch(Manifest.permission.POST_NOTIFICATIONS) else { model.repository.preferences.edit().putBoolean("notifications", true).apply(); next() }
                    OnboardingStep.Location -> location.launch(Manifest.permission.ACCESS_COARSE_LOCATION)
                    OnboardingStep.Done -> {
                        busy = true
                        model.launch {
                            try {
                                val draft = plan ?: model.planner.makeStarterPlan(areas, model.repository.locale)
                                model.repository.change { core ->
                                    val agent = core.agents().first { it.persona.handle == "zoen" }
                                    agent.spaces.forEach { core.setTrust(agent.persona.id, it.spaceId, TrustLevelDto.entries[trust]) }
                                    val chat = core.spaces().first { it.counterpart?.handle == "zoen" }
                                    if (trust > 0) core.agentCreatePlan(chat.id, agent.persona.id, prompt, draft.plan, draft.engineLabel, 0)
                                }
                                val landing = try {
                                    when (route.landing) {
                                        "chat_with_inviter" -> route.target?.let { target ->
                                            val friend = model.repository.network { core -> core.findPeople(target).firstOrNull { it.handle.equals(target, true) } }
                                            friend?.let { model.repository.change { core -> core.startDirect(it.id) } }
                                        }
                                        "space" -> route.target?.let { target -> model.repository.network { it.joinInvite(target) } }
                                        else -> null
                                    }
                                } catch (e: Exception) { if (e is CancellationException) throw e; null }
                                photoPath?.let { java.io.File(it).delete() }; photoPath = null
                                finished(landing ?: model.state.value.zoenChat?.id)
                                model.repository.preferences.edit().putBoolean("onboarded", true).putString("areas", selected.joinToString(",")).commit()
                                model.repository.refresh()
                                model.reportGrowthAfterOnboarding()
                            } finally { busy = false }
                        }
                    }
                    else -> next()
                }
            }, enabled = !busy && !planning && (step != OnboardingStep.Profile || (name.isNotBlank() && Regex("[a-z][a-z0-9._]{2,23}").matches(handle) && (relay.startsWith("https://") || BuildConfig.DEBUG && relay.startsWith("http://")))), modifier = Modifier.fillMaxWidth().heightIn(min = 56.dp).testTag("onboarding-next")) {
                if (busy) { CircularProgressIndicator(Modifier.size(20.dp), strokeWidth = 2.dp); Spacer(Modifier.width(12.dp)) }
                Text(stringResource(when { busy -> R.string.creating; step == OnboardingStep.Hello -> R.string.onboarding_hi_zoen; step == OnboardingStep.Plan -> R.string.onboarding_looks_good; step == OnboardingStep.Notifications -> R.string.enable_notifications; step == OnboardingStep.Location -> R.string.onboarding_allow_location; step == OnboardingStep.Done -> R.string.finish; else -> R.string.continue_label }))
            }
            if (step == OnboardingStep.Notifications || step == OnboardingStep.Location) TextButton(onClick = {
                model.repository.preferences.edit().putBoolean(if (step == OnboardingStep.Notifications) "notifications" else "location", false).apply()
                next()
            }) { Text(stringResource(R.string.not_now)) }
            if (step == OnboardingStep.Hello && BuildConfig.DEBUG) TextButton(onClick = { model.launch { model.repository.useDemo() } }) { Text(stringResource(R.string.explore_demo)) }
        }
    }
    crop?.let { source -> ProfilePhotoCrop(source, apply = { zoom, x, y -> model.launch {
        val bytes = ProfilePhotos.jpeg(source, zoom, x, y)
        val directory = java.io.File(context.noBackupFilesDir, "onboarding").apply { mkdirs() }
        val file = java.io.File(directory, "photo-${java.util.UUID.randomUUID()}.jpg").apply { writeBytes(bytes) }
        photoPath?.let { java.io.File(it).delete() }; photoPath = file.absolutePath; cropUri = null; crop = null
    } }, cancel = { cropUri = null; crop = null }) }
    DisposableEffect(crop) { val bitmap = crop; onDispose { bitmap?.recycle() } }
}
