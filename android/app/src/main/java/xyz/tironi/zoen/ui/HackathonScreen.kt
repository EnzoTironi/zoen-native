package xyz.tironi.zoen.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.navigation3.runtime.NavKey
import kotlinx.coroutines.delay
import org.json.JSONArray
import org.json.JSONObject
import xyz.tironi.zoen.ZoenViewModel
import xyz.tironi.zoen.core.PersonaKind
import xyz.tironi.zoen.data.AppState

private fun JSONArray?.objects(): List<JSONObject> = if (this == null) emptyList() else (0 until length()).mapNotNull { optJSONObject(it) }
private fun JSONArray?.strings(): List<String> = if (this == null) emptyList() else (0 until length()).map { optString(it) }
private fun JSONObject.projectUrl(): String? = optString("project_url").takeIf { runCatching { android.net.Uri.parse(it).scheme?.lowercase() in listOf("https", "http") }.getOrDefault(false) }

@Composable
private fun LivingFormSection(title: String, content: @Composable ColumnScope.() -> Unit) {
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Text(title, style = MaterialTheme.typography.titleMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
        Surface(shape = RoundedCornerShape(12.dp), color = MaterialTheme.colorScheme.surface) {
            Column(Modifier.fillMaxWidth().padding(16.dp), verticalArrangement = Arrangement.spacedBy(12.dp), content = content)
        }
    }
}

/** Native forms project authoritative signed state; admission and success never happen optimistically. */
@Composable
fun HackathonScreen(model: ZoenViewModel, state: AppState, spaceId: String, navigate: (NavKey) -> Unit, back: () -> Unit) {
    var home by remember(spaceId) { mutableStateOf<JSONObject?>(null) }
    var revision by remember { mutableIntStateOf(0) }
    var busy by remember { mutableStateOf(false) }
    var success by remember { mutableStateOf<String?>(null) }
    var failure by remember { mutableStateOf<String?>(null) }
    var title by rememberSaveable { mutableStateOf("") }
    var deadline by rememberSaveable { mutableLongStateOf(System.currentTimeMillis() + 86400000L) }
    var teamName by rememberSaveable { mutableStateOf("") }
    var skills by rememberSaveable { mutableStateOf("") }
    var projectUrl by rememberSaveable { mutableStateOf("") }
    var eventTitle by rememberSaveable { mutableStateOf("") }
    var eventAt by rememberSaveable { mutableLongStateOf(System.currentTimeMillis() + 3600000L) }
    var selectedFiles by remember { mutableStateOf<Set<String>>(emptySet()) }
    var scores by remember { mutableStateOf<Map<String, Int>>(emptyMap()) }
    var now by remember { mutableLongStateOf(System.currentTimeMillis()) }
    var judgeMenu by remember { mutableStateOf(false) }
    val uri = LocalUriHandler.current
    val me = state.me?.id.orEmpty()
    val members = state.spaces.firstOrNull { it.id == spaceId }?.members.orEmpty().filter { it.kind == PersonaKind.PERSON }
    LaunchedEffect(Unit) { while (true) { now = System.currentTimeMillis(); delay(1000) } }
    LaunchedEffect(spaceId, state.revision, revision) {
        try { home = model.repository.query { JSONObject(it.livingSpaceCommand(spaceId, "{\"action\":\"home\"}")) }; failure = null }
        catch (error: Exception) { failure = error.message }
    }
    fun command(value: JSONObject) {
        if (busy) return
        val registeredBefore = me in home?.optJSONObject("hackathon")?.optJSONArray("registered").strings()
        busy = true; success = null
        model.launch {
            try {
                model.repository.change { it.livingSpaceCommand(spaceId, value.toString()) }
                revision++; failure = null
                success = when (value.optString("action")) { "register" -> if (registeredBefore) "Interests updated" else "Registered"; "team_create" -> "Team created"; "project_submit" -> "Project submitted"; "vote" -> "Vote recorded"; "finalize" -> "Winners announced"; else -> "Updated" }
            } catch (error: Exception) { failure = error.message }
            finally { busy = false }
        }
    }
    val hack = home?.optJSONObject("hackathon")
    val admin = home?.optBoolean("can_manage") == true
    val teams = hack?.optJSONArray("teams").objects()
    val mine = teams.firstOrNull { me in it.optJSONArray("members").strings() }
    LaunchedEffect(hack?.optString("id"), me) { if (hack != null) skills = hack.optJSONObject("profiles")?.optString(me).orEmpty() }
    LaunchedEffect(mine?.optString("id")) { if (mine != null) { projectUrl = mine.projectUrl().orEmpty(); selectedFiles = mine.optJSONArray("files").strings().toSet() } }
    Scaffold(topBar = { ScreenBar("Hackathon", back) }) { padding ->
        Column(Modifier.padding(padding).fillMaxSize().background(MaterialTheme.colorScheme.surfaceContainerLow).verticalScroll(rememberScrollState()).padding(20.dp).testTag("living-space-hackathon"), verticalArrangement = Arrangement.spacedBy(24.dp)) {
            failure?.let { Text(it, color = MaterialTheme.colorScheme.error) }
            success?.let { LivingSpaceFeedback(it, revision) }
            if (hack == null) {
                LivingFormSection("Overview") {
                    Text("Build something together", style = MaterialTheme.typography.titleLarge)
                    if (admin) {
                        OutlinedTextField(title, { title = it }, label = { Text("Hackathon name") }, modifier = Modifier.fillMaxWidth())
                        LivingDateTimeField("Submission deadline", deadline) { deadline = it }
                        Button(enabled = !busy && title.isNotBlank() && deadline > now, onClick = { command(JSONObject().put("action", "hackathon_create").put("title", title.trim()).put("deadline_ms", deadline)) }) { Text("Create hackathon") }
                    } else Text("No hackathon yet.")
                }
            } else {
                val ended = hack.optBoolean("submissions_closed")
                val remaining = ((hack.optLong("deadline_ms") - now + 999) / 1000).coerceAtLeast(0)
                val finalized = hack.optBoolean("finalized")
                val registered = me in hack.optJSONArray("registered").strings()
                LivingFormSection("Overview") {
                    Text(hack.optString("title"), style = MaterialTheme.typography.titleLarge, fontWeight = FontWeight.SemiBold)
                    Text(if (ended) "Submissions closed" else if (remaining == 0L) "Awaiting administrator to close submissions" else "${remaining / 86400}d ${remaining / 3600 % 24}h ${remaining / 60 % 60}m ${remaining % 60}s remaining", style = MaterialTheme.typography.bodyLarge.copy(fontFeatureSettings = "tnum"))
                    Text(java.text.DateFormat.getDateTimeInstance().format(java.util.Date(hack.optLong("deadline_ms"))), color = MaterialTheme.colorScheme.onSurfaceVariant)
                    OutlinedTextField(skills, { skills = it.take(512) }, label = { Text("What would you like to work on?") }, modifier = Modifier.fillMaxWidth(), minLines = 2, maxLines = 4)
                    Button(enabled = !busy && !ended && !finalized, onClick = { command(JSONObject().put("action", "register").put("skills", skills.trim())) }) { Text(if (registered) "Update interests" else "Register") }
                }
                LivingFormSection("Find teammates") {
                    hack.optJSONArray("registered").strings().filter { who -> teams.none { who in it.optJSONArray("members").strings() } }.forEach { who ->
                        Text(members.firstOrNull { it.id == who }?.name ?: "Community member", fontWeight = FontWeight.SemiBold)
                        hack.optJSONObject("profiles")?.optString(who)?.takeIf { it.isNotBlank() }?.let { Text(it, style = MaterialTheme.typography.bodySmall) }
                    }
                    if (mine == null) {
                        OutlinedTextField(teamName, { teamName = it }, label = { Text("Team name") }, modifier = Modifier.fillMaxWidth())
                        Button(enabled = !busy && registered && !ended && !finalized && teamName.isNotBlank(), onClick = { command(JSONObject().put("action", "team_create").put("name", teamName.trim())) }) { Text("Create team") }
                    }
                    teams.forEach { team ->
                        Text(team.optString("name"), fontWeight = FontWeight.SemiBold)
                        Text("${team.optJSONArray("members")?.length() ?: 0} teammates", color = MaterialTheme.colorScheme.onSurfaceVariant)
                        val own = me in team.optJSONArray("members").strings()
                        val chat = team.optString("space").takeUnless { it.isBlank() || it == "null" }
                        if (own) TextButton(enabled = chat != null, onClick = { chat?.let { navigate(Chat(it)) } }) { Text("Open team conversation") }
                        else if (mine == null) TextButton(enabled = !busy && registered && !ended && !finalized, onClick = { command(JSONObject().put("action", "team_join").put("team_id", team.getString("id"))) }) { Text("Request to join") }
                        hack.optJSONArray("pending_team_requests").objects().filter { it.optString("team_id") == team.optString("id") }.forEach { request ->
                            val who = request.optString("identity_id")
                            Text(members.firstOrNull { it.id == who }?.name ?: "Community member")
                            TextButton(enabled = !busy && !ended && !finalized, onClick = { command(JSONObject().put("action", "team_join").put("team_id", team.getString("id")).put("identity_id", who)) }) { Text("Accept teammate") }
                        }
                    }
                }
                LivingFormSection("Schedule") {
                    val events = hack.optJSONArray("schedule").objects()
                    if (events.isEmpty()) Text("No milestones yet.", color = MaterialTheme.colorScheme.onSurfaceVariant)
                    events.forEach { event -> Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(12.dp)) { Text(event.optString("title"), Modifier.weight(1f)); Text(java.text.DateFormat.getDateTimeInstance().format(java.util.Date(event.optLong("at_ms"))), color = MaterialTheme.colorScheme.onSurfaceVariant) } }
                    if (admin) {
                        OutlinedTextField(eventTitle, { eventTitle = it }, label = { Text("Milestone") }, modifier = Modifier.fillMaxWidth())
                        LivingDateTimeField("When", eventAt) { eventAt = it }
                        TextButton(enabled = !busy && !finalized && eventTitle.isNotBlank(), onClick = { command(JSONObject().put("action", "schedule_event").put("title", eventTitle.trim()).put("at_ms", eventAt)) }) { Text("Add milestone") }
                    }
                }
                LivingFormSection("Project") {
                    if (mine != null) {
                        mine.projectUrl()?.let { link -> TextButton({ uri.openUri(link) }) { Text("Submitted project") } }
                        OutlinedTextField(projectUrl, { projectUrl = it }, label = { Text("Project URL") }, modifier = Modifier.fillMaxWidth())
                        state.items.filter { it.spaceId == spaceId && it.file != null }.forEach { file -> Row(verticalAlignment = Alignment.CenterVertically) { Checkbox(file.id in selectedFiles, { checked -> selectedFiles = if (checked) selectedFiles + file.id else selectedFiles - file.id }); Text(file.title) } }
                        Button(enabled = !busy && !ended && !finalized && (projectUrl.isNotBlank() || selectedFiles.isNotEmpty()), onClick = { command(JSONObject().put("action", "project_submit").put("team_id", mine.getString("id")).put("url", projectUrl.trim()).put("file_ids", JSONArray(selectedFiles.toList()))) }) { Text("Submit project") }
                        mine.optJSONArray("files").strings().forEach { id -> TextButton({ navigate(Item(id)) }) { Text("Open submitted file") } }
                    } else Text("Join a team to submit a project.", color = MaterialTheme.colorScheme.onSurfaceVariant)
                    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) { Icon(Icons.Rounded.Lock, null); Text("Demo mini-app · Coming later", style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant) }
                }
                LivingFormSection("Judging & voting") {
                    teams.forEach { team ->
                        val id = team.getString("id")
                        val own = me in team.optJSONArray("members").strings()
                        val submitted = team.projectUrl() != null || team.optJSONArray("files").strings().isNotEmpty()
                        val mayJudge = !busy && ended && !finalized && !own && submitted
                        Text(team.optString("name"), fontWeight = FontWeight.SemiBold)
                        Text("${team.optInt("votes")} votes · ${team.optInt("score")} points", color = MaterialTheme.colorScheme.onSurfaceVariant)
                        TextButton(enabled = mayJudge && registered, onClick = { command(JSONObject().put("action", "vote").put("team_id", id)) }) { Text("Vote") }
                        team.projectUrl()?.let { link -> TextButton({ uri.openUri(link) }) { Text("View project") } }
                        team.optJSONArray("files").strings().forEach { file -> TextButton({ navigate(Item(file)) }) { Text("Open project file") } }
                        if (admin || hack.optBoolean("can_judge")) {
                            val score = scores[id] ?: 50
                            Row(verticalAlignment = Alignment.CenterVertically) {
                                Text("Score: $score", Modifier.weight(1f))
                                IconButton(enabled = mayJudge && score > 0, onClick = { scores = scores + (id to score - 1) }) { Icon(Icons.Rounded.Remove, "Decrease score") }
                                IconButton(enabled = mayJudge && score < 100, onClick = { scores = scores + (id to score + 1) }) { Icon(Icons.Rounded.Add, "Increase score") }
                            }
                            TextButton(enabled = mayJudge, onClick = { command(JSONObject().put("action", "judge").put("team_id", id).put("score", score)) }) { Text("Submit judge score") }
                        }
                    }
                    if (admin) Box {
                        TextButton({ judgeMenu = true }) { Text("Assign judge") }
                        DropdownMenu(judgeMenu, { judgeMenu = false }) { members.forEach { person -> DropdownMenuItem(text = { Text(person.name) }, onClick = { judgeMenu = false; command(JSONObject().put("action", "set_judge").put("identity_id", person.id)) }, enabled = !busy && !finalized) } }
                    }
                }
                LivingFormSection("Winners") {
                    val winners = hack.optJSONArray("winners").strings()
                    if (winners.isEmpty()) Text("Results have not been published.", color = MaterialTheme.colorScheme.onSurfaceVariant)
                    teams.filter { it.optString("id") in winners }.forEach { team -> Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) { Icon(Icons.Rounded.EmojiEvents, null); Text(team.optString("name"), fontWeight = FontWeight.SemiBold) } }
                    if (admin && !ended && !finalized) Button(enabled = !busy, onClick = { command(JSONObject().put("action", "submissions_close")) }) { Text("Close submissions and open voting") }
                    if (admin && !finalized) Button(enabled = !busy && ended, onClick = { command(JSONObject().put("action", "finalize")) }) { Text("Publish winners announcement") }
                    if (finalized) Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) { Icon(Icons.Rounded.CheckCircle, null); Text("Results published to announcements", style = MaterialTheme.typography.bodySmall) }
                }
            }
        }
    }
}
