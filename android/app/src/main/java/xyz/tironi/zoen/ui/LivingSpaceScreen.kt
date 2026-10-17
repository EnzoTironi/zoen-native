package xyz.tironi.zoen.ui

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.luminance
import androidx.compose.ui.unit.sp
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.draw.clip
import androidx.compose.ui.layout.ContentScale
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.navigation3.runtime.NavKey
import androidx.navigation3.runtime.entryProvider
import androidx.navigation3.runtime.rememberNavBackStack
import androidx.navigation3.ui.NavDisplay
import androidx.activity.compose.BackHandler
import xyz.tironi.zoen.ZoenViewModel
import xyz.tironi.zoen.core.*
import xyz.tironi.zoen.data.AppState
import org.json.JSONObject

/** Experiment stays opt-in. The existing chat and rich page editor remain the detail surfaces. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun LivingSpaceScreen(model: ZoenViewModel, state: AppState, space: SpaceSummary,
                      navigate: (NavKey) -> Unit, back: () -> Unit) {
    var projection by remember(space.id) { mutableStateOf<JSONObject?>(null) }
    var failure by remember(space.id) { mutableStateOf<String?>(null) }
    var reload by remember { mutableIntStateOf(0) }
    var choosePage by remember { mutableStateOf(false) }
    var showingAnnouncements by rememberSaveable { mutableStateOf(false) }
    var pinAnnouncement by remember { mutableStateOf(false) }
    var chooseCover by remember { mutableStateOf(false) }
    var hackathon by rememberSaveable { mutableStateOf(false) }
    var tiers by rememberSaveable { mutableStateOf(false) }
    val coverId = projection?.optString("cover_file")?.takeUnless { it == "null" || it.isEmpty() }
    val cover by produceState<android.graphics.Bitmap?>(null, coverId, state.revision) {
        value = try { coverId?.let { id -> model.repository.query { core ->
            val item = state.items.firstOrNull { it.id == id }
            val bytes = core.fileThumbnail(id) ?: item?.let { core.fileBytesBounded(id, it.version, 20_971_520uL) }
            bytes?.let {
                val bounds = android.graphics.BitmapFactory.Options().apply { inJustDecodeBounds = true }
                android.graphics.BitmapFactory.decodeByteArray(it, 0, it.size, bounds)
                val options = android.graphics.BitmapFactory.Options().apply { inSampleSize = 1 }
                while (bounds.outWidth / options.inSampleSize > 1600 || bounds.outHeight / options.inSampleSize > 1600) options.inSampleSize *= 2
                android.graphics.BitmapFactory.decodeByteArray(it, 0, it.size, options)
            }
        } } } catch (error: Exception) { failure = error.message; null }
    }
    val pages = state.items.filter { it.spaceId == space.id && it.kindId == "page" }
    fun command(action: JSONObject) { model.launch {
        try { model.repository.change { it.livingSpaceCommand(space.id, action.toString()) }; reload++; failure = null }
        catch (error: Exception) { failure = error.message }
    } }
    LaunchedEffect(space.id, state.revision, reload) {
        try {
            projection = model.repository.query { core ->
                core.setLivingSpaceEnabled(true)
                JSONObject(core.livingSpaceCommand(space.id, "{\"action\":\"home\"}"))
            }
            failure = null
        } catch (error: Exception) { failure = error.message }
    }
    BackHandler(enabled = tiers || hackathon) { if (tiers) tiers = false else hackathon = false }
    if (tiers) {
        SpaceTiersScreen(model, state, space.id, navigate = navigate) { tiers = false }
        return
    }
    if (hackathon) {
        HackathonScreen(model, state, space.id, navigate) { hackathon = false }
        return
    }
    var creatingPage by remember { mutableStateOf(false) }
    fun createPage() {
        if (creatingPage) return
        creatingPage = true
        model.launch {
            try {
                val page = model.repository.change { it.pageCreate(space.id, "Untitled page") }
                model.repository.change { it.livingSpaceCommand(space.id, JSONObject().put("action", "set_home").put("page_id", page.id).toString()) }
                reload++; navigate(Item(page.id))
            } catch (_: Exception) { failure = "The page could not be opened. Check your access and try again." }
            finally { creatingPage = false }
        }
    }
    Scaffold(topBar = { LivingScreenBar("Home", back) }) { padding ->
        Box(Modifier.padding(padding).fillMaxSize(), contentAlignment = Alignment.TopCenter) {
            Column(Modifier.widthIn(max = 720.dp).fillMaxWidth().verticalScroll(rememberScrollState())
                .padding(horizontal = 20.dp, vertical = 20.dp).testTag("living-space-home"),
                verticalArrangement = Arrangement.spacedBy(24.dp)) {
                LivingGlassCard(shape = RoundedCornerShape(40.dp), modifier = Modifier.fillMaxWidth()) {
                    Column(Modifier.padding(24.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                        if (cover != null) Image(cover!!.asImageBitmap(), null, Modifier.fillMaxWidth().height(160.dp).clip(RoundedCornerShape(16.dp)).border(1.dp, if (MaterialTheme.colorScheme.background.luminance() < 0.5f) Color.White.copy(alpha = 0.1f) else Color.Black.copy(alpha = 0.1f), RoundedCornerShape(16.dp)), contentScale = ContentScale.Crop)
                        else Box(Modifier.fillMaxWidth().height(160.dp).clip(RoundedCornerShape(16.dp)).background(MaterialTheme.colorScheme.surfaceContainerHigh), contentAlignment = Alignment.Center) { Icon(Icons.Rounded.Image, null, Modifier.size(32.dp)) }
                        Text(space.title, style = MaterialTheme.typography.headlineLarge.copy(fontSize = 34.sp, lineHeight = 41.sp), fontWeight = FontWeight.Bold)
                        val memberCount = space.members.count { it.kind == PersonaKind.PERSON }
                        Text("$memberCount ${if (memberCount == 1) "member" else "members"}", style = MaterialTheme.typography.bodyMedium.copy(fontSize = 15.sp, lineHeight = 20.sp), color = MaterialTheme.colorScheme.onSurfaceVariant)
                        if (projection?.optBoolean("can_manage") == true) LivingLink("Set cover", { chooseCover = true }, icon = Icons.Rounded.AddPhotoAlternate)
                        val labels = projection?.optJSONArray("tiers")
                        if (labels != null) Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                            (0 until labels.length()).mapNotNull { labels.optJSONObject(it) }.filter { tier ->
                                val members = tier.optJSONArray("members")
                                members != null && (0 until members.length()).any { members.optString(it) == state.me?.id }
                            }.forEach { tier -> Surface(shape = RoundedCornerShape(50), color = MaterialTheme.colorScheme.surfaceContainerHigh) {
                                Text(tier.optString("name"), Modifier.padding(horizontal = 8.dp, vertical = 4.dp), style = MaterialTheme.typography.labelMedium)
                            } }
                        }
                    }
                }
                Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                    LivingSectionLabel("Pinned announcements", Icons.Rounded.PushPin)
                    val ids = projection?.optJSONArray("announcement_items")
                    val announcements = state.items.filter { item -> ids != null && (0 until ids.length()).any { ids.optString(it) == item.id } }
                    LivingLink("View announcements", { showingAnnouncements = true }, Modifier.testTag("living-space-open-announcements"), Icons.Rounded.ChevronRight)
                    if (announcements.isEmpty()) Text("No pinned announcements.", color = MaterialTheme.colorScheme.onSurfaceVariant)
                    announcements.forEach { page -> LivingLink(page.title, { navigate(Item(page.id)) }, Modifier.fillMaxWidth(), verticalPadding = 8.dp) }
                    if (projection?.optBoolean("can_manage") == true) LivingLink("Pin announcement", { pinAnnouncement = true }, icon = Icons.Rounded.PushPin, enabled = pages.isNotEmpty())
                }
                Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                    LivingSectionLabel("Live page", Icons.Rounded.Description)
                    val selected = pages.filter { it.id == projection?.optString("page") }
                    if (selected.isEmpty()) Text("A shared page, edited together.", color = MaterialTheme.colorScheme.onSurfaceVariant)
                    selected.forEach { page -> LivingLink(page.title, { navigate(Item(page.id)) }, Modifier.fillMaxWidth(), Icons.Rounded.Description, verticalPadding = 8.dp) }
                    if (projection?.optBoolean("can_manage") == true) {
                        LivingLink("Choose page", { choosePage = true }, icon = Icons.Rounded.InsertDriveFile, enabled = pages.isNotEmpty())
                        LivingGlassCard(onClick = { if (!creatingPage) createPage() }, shape = RoundedCornerShape(50), elevation = 8.dp, modifier = Modifier.testTag("living-space-new-page")) {
                            Row(Modifier.padding(horizontal = 12.dp, vertical = 8.dp), horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
                                Icon(Icons.Rounded.Add, null, Modifier.size(17.dp), tint = MaterialTheme.colorScheme.primary)
                                Text(if (creatingPage) "Creating page…" else "Create page", style = MaterialTheme.typography.bodyLarge.copy(fontSize = 17.sp, lineHeight = 20.333f.sp), color = MaterialTheme.colorScheme.primary)
                            }
                        }
                    }
                }
                listOf("Events" to Icons.Rounded.CalendarMonth, "Files" to Icons.Rounded.Folder).forEach { (label, icon) ->
                    Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                        LivingSectionLabel(label, icon)
                        val ids = projection?.optJSONArray(if (label == "Files") "file_items" else "event_items")
                        val items = state.items.filter { item -> ids != null && (0 until ids.length()).any { ids.optString(it) == item.id } }
                        if (items.isEmpty()) Text(if (label == "Files") "No files yet." else "No events yet.", color = MaterialTheme.colorScheme.onSurfaceVariant)
                        items.forEach { item -> TextButton(onClick = { navigate(Item(item.id)) }, modifier = Modifier.fillMaxWidth()) {
                            Icon(icon, null); Spacer(Modifier.width(8.dp)); Text(item.title)
                        } }
                        if (label == "Files") TextButton({ navigate(Folder(space.id)) }) { Text("Open folder") }
                    }
                }
                LivingGlassCard(shape = RoundedCornerShape(20.dp), modifier = Modifier.fillMaxWidth()) {
                    Row(Modifier.padding(16.dp).padding(vertical = 12.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                        Icon(Icons.Rounded.GridView, null, Modifier.width(24.dp))
                        Text("Mini-apps", Modifier.weight(1f))
                        Text("Coming later", style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                        Icon(Icons.Rounded.Lock, null, Modifier.size(13.dp), tint = MaterialTheme.colorScheme.onSurfaceVariant)
                    }
                }
                Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                    LivingSectionLabel("Hackathon", Icons.Rounded.EmojiEvents)
                    TextButton({ hackathon = true }, Modifier.testTag("living-space-open-hackathon")) { Text(projection?.optJSONObject("hackathon")?.optString("title") ?: "Hackathon") }
                }
                Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                    LivingSectionLabel("Memberships", Icons.Rounded.VerifiedUser)
                    TextButton({ tiers = true }, Modifier.testTag("living-space-open-tiers")) { Text(if (projection?.optBoolean("can_manage") == true) "Manage memberships" else "View memberships") }
                }
                if (failure != null) Text(failure.orEmpty(), color = MaterialTheme.colorScheme.error)
                Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                    LivingSectionLabel("Conversations", Icons.Rounded.Forum)
                    val conversationSpace = projection?.optString("conversation_space")?.takeUnless { it.isBlank() || it == "null" }
                    TextButton(enabled = conversationSpace != null, onClick = { conversationSpace?.let { navigate(Chat(it)) } }, modifier = Modifier.testTag("living-space-conversations")) { Text("General conversation") }
                    if (conversationSpace == null) Text("Connect your account to join the conversation.", color = MaterialTheme.colorScheme.onSurfaceVariant)
                }
                Text("Agents are paused in this Space.", style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
            }
        }
    }
    if (showingAnnouncements) ModalBottomSheet(onDismissRequest = { showingAnnouncements = false }, sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true)) {
        Column(Modifier.fillMaxWidth().padding(20.dp).testTag("living-space-announcements"), verticalArrangement = Arrangement.spacedBy(16.dp)) {
            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) { Text("Announcements", style = MaterialTheme.typography.titleLarge); TextButton({ showingAnnouncements = false }) { Text("Done") } }
            val ids = projection?.optJSONArray("announcement_items")
            val announcements = state.items.filter { item -> ids != null && (0 until ids.length()).any { ids.optString(it) == item.id } }
            if (announcements.isEmpty()) Text("No pinned announcements.", color = MaterialTheme.colorScheme.onSurfaceVariant)
            announcements.forEach { item -> TextButton({ showingAnnouncements = false; navigate(Item(item.id)) }) { Text(item.title) } }
        }
    }
    if (chooseCover) AlertDialog(onDismissRequest = { chooseCover = false }, title = { Text("Choose cover") }, text = {
        Column { val images = state.items.filter { it.spaceId == space.id && it.file?.mime?.startsWith("image/") == true }
            if (images.isEmpty()) Text("Add an image to this Space’s files first.")
            images.forEach { image -> TextButton({ command(JSONObject().put("action", "set_cover").put("file_id", image.id)); chooseCover = false }) { Text(image.title) } }
        }
    }, confirmButton = { TextButton({ chooseCover = false }) { Text("Cancel") } })
    if (choosePage || pinAnnouncement) AlertDialog(onDismissRequest = { choosePage = false; pinAnnouncement = false }, title = { Text(if (pinAnnouncement) "Pin announcement" else "Choose page") }, text = {
        Column { pages.forEach { page -> TextButton({
            command(JSONObject().put("action", if (pinAnnouncement) "announce" else "set_home").put(if (pinAnnouncement) "item_id" else "page_id", page.id))
            choosePage = false; pinAnnouncement = false
        }) { Text(page.title) } } }
    }, confirmButton = { TextButton({ choosePage = false; pinAnnouncement = false }) { Text("Cancel") } })

}

/** Opt-in native shell uses the existing native navigation stack and durable entry state. */
@Composable
fun LivingSpaceRoot(model: ZoenViewModel, state: AppState, deepLink: String?, consumed: () -> Unit, snackbar: SnackbarHostState) {
    var tierInvite by remember { mutableStateOf<Pair<String, String>?>(null) }
    LaunchedEffect(deepLink) {
        val uri = deepLink?.let(android.net.Uri::parse)
        if (uri?.scheme == "zoen" && uri.host == "tier") {
            val space = uri.getQueryParameter("space"); val token = uri.getQueryParameter("token")
            if (space != null && token != null) tierInvite = space to uri.toString()
            consumed()
        }
    }
    var notifications by rememberSaveable { mutableStateOf(false) }
    val stack = rememberNavBackStack(Home)
    val navigate: (NavKey) -> Unit = { target ->
        when (target) {
            is Chat, is Item, is Folder, is Participants, is Thread, is VersionPreview, is Person -> if (stack.lastOrNull() != target) stack.add(target)
            else -> model.notify("This action is paused in this Space.")
        }
        tierInvite = null
    }
    val back: () -> Unit = { if (stack.size > 1) stack.removeLastOrNull() }
    BackHandler(enabled = tierInvite != null || notifications) { if (tierInvite != null) tierInvite = null else notifications = false }
    val pending by produceState(0, state.revision) {
        value = try { model.repository.query { core -> state.spaces.filter { it.kind == SpaceKindDto.COMMUNITY }.sumOf { space ->
            val home = JSONObject(core.livingSpaceCommand(space.id, "{\"action\":\"home\"}"))
            if (home.optBoolean("can_manage")) (home.optJSONArray("pending_requests")?.length() ?: 0) + (home.optJSONArray("tier_claims")?.length() ?: 0) else 0
        } } } catch (_: Exception) { 0 }
    }
    Scaffold(contentWindowInsets = WindowInsets(0, 0, 0, 0), snackbarHost = { SnackbarHost(snackbar) }, bottomBar = {
        Box(Modifier.fillMaxWidth().navigationBarsPadding().padding(horizontal = 20.dp, vertical = 8.dp), contentAlignment = Alignment.Center) {
        LivingGlassCard(modifier = Modifier.widthIn(max = 320.dp), shape = RoundedCornerShape(50)) {
        NavigationBar(modifier = Modifier.height(64.dp), containerColor = Color.Transparent, tonalElevation = 0.dp, windowInsets = WindowInsets(0, 0, 0, 0)) {
            NavigationBarItem(selected = !notifications, onClick = { notifications = false; while (stack.size > 1) stack.removeLastOrNull() }, icon = { Icon(Icons.Rounded.Layers, null) }, label = { Text("Spaces") })
            NavigationBarItem(selected = notifications, onClick = { notifications = true }, icon = { BadgedBox(badge = { if (pending > 0) Badge { Text(pending.toString()) } }) { Icon(Icons.Rounded.Notifications, null) } }, label = { Text("Notifications") })
        }
        }
        }
    }) { padding -> Box(Modifier.padding(padding)) {
        val invite = tierInvite
        if (invite != null) {
            if (state.spaces.any { it.id == invite.first }) SpaceTiersScreen(model, state, invite.first, invite.second, navigate) { tierInvite = null }
            else Column(Modifier.padding(24.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) { Text("Join this Space before using this invitation."); TextButton({ tierInvite = null }) { Text("Close") } }
        } else if (notifications) LivingSpaceNotifications(model, state)
        else NavDisplay(backStack = stack, onBack = back, entryProvider = entryProvider {
            entry<Home> {
                Scaffold(topBar = { ScreenBar("Spaces", null) }) { insets ->
                    Column(Modifier.padding(insets).fillMaxSize().verticalScroll(rememberScrollState()).padding(20.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                        state.spaces.filter { it.kind == SpaceKindDto.COMMUNITY }.forEach { space ->
                            LivingGlassCard(onClick = { navigate(Chat(space.id)) }, modifier = Modifier.fillMaxWidth(), shape = RoundedCornerShape(20.dp)) {
                                ListItem(colors = ListItemDefaults.colors(containerColor = Color.Transparent), headlineContent = { Text(space.title, fontWeight = FontWeight.SemiBold) }, leadingContent = { SpaceAvatar(space, size = 44) }, supportingContent = { Text("${space.members.count { it.kind == PersonaKind.PERSON }} members") })
                            }
                        }
                    }
                }
            }
            entry<Chat> { ChatScreen(model, state, it.id, navigate, back, it.message) }
            entry<Item> { ItemScreen(model, state, it.id, navigate, back) }
            entry<Folder> { FilesScreen(model, state, navigate, back, it.space) }
            entry<Participants> { LivingParticipantsScreen(model, state, it.id, back) }
            entry<Person> { LivingPersonSummary(state, it.id, back) }
            entry<Thread> { ThreadScreen(model, state, it.space, it.root, navigate, back) }
            entry<VersionPreview> { VersionPreviewScreen(model, it.id, it.number, back, snackbar) }
        })
    } }
}
