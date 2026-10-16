package xyz.tironi.zoen.ui

import android.content.Intent
import android.provider.Settings as AndroidSettings
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.Spring
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.spring
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.slideInVertically
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.rounded.ArrowBack
import androidx.compose.material.icons.automirrored.rounded.Chat
import androidx.compose.material.icons.rounded.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.scale
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.navigation3.runtime.NavKey
import java.text.Normalizer
import java.text.NumberFormat
import java.util.Locale
import xyz.tironi.zoen.R
import xyz.tironi.zoen.ZoenViewModel
import xyz.tironi.zoen.core.*
import xyz.tironi.zoen.data.AppState
import xyz.tironi.zoen.theme.AppAppearance

/** Every Ajustes screen. Back pops one; the X closes Ajustes from anywhere. */
enum class SettingsPage(val title: Int, val icon: ImageVector, val keywords: String) {
    Profile(R.string.st_profile, Icons.Rounded.PersonOutline, "nome name foto photo arroba handle perfil"),
    Preferences(R.string.st_preferences, Icons.Rounded.Tune, "preferências idioma language"),
    Theme(R.string.st_theme, Icons.Rounded.Contrast, "tema escuro claro dark light aparência appearance"),
    Security(R.string.st_security, Icons.Rounded.Lock, "segurança senha password código code"),
    Devices(R.string.st_devices, Icons.Rounded.Devices, "aparelhos celular computador desconectar"),
    TwoStep(R.string.st_two_step, Icons.Rounded.VerifiedUser, "duas etapas 2fa autenticador sms"),
    Data(R.string.st_data, Icons.Rounded.Storage, "dados exportar apagar espaço storage"),
    Backup(R.string.st_backup, Icons.Rounded.CloudQueue, "cópia backup restaurar recuperação senha"),
    Accessibility(R.string.st_accessibility, Icons.Rounded.AccessibilityNew, "acessibilidade movimento texto contraste transparência"),
    Legal(R.string.st_legal, Icons.Rounded.Description, "termos privacidade legal"),
    Feedback(R.string.st_feedback, Icons.AutoMirrored.Rounded.Chat, "feedback sugestão problema bug"),
    About(R.string.st_about, Icons.Rounded.Info, "sobre versão version"),
    Tasks(R.string.st_tasks, Icons.Rounded.EventRepeat, "tarefas agendadas agenda"),
    Disappearing(R.string.st_disappearing, Icons.Rounded.Timer, "temporárias somem apagar sumir timer"),
    Plans(R.string.st_plans, Icons.Rounded.AutoAwesome, "créditos planos pro business assinatura"),
    Discover(R.string.st_discover, Icons.Rounded.Explore, "descobrir comunidades públicas"),
    Moderation(R.string.st_moderation, Icons.Rounded.Flag, "moderação denúncias report"),
    Invites(R.string.st_invites, Icons.Rounded.CardGiftcard, "convites amigos indicar business grátis"),
    Wallet(R.string.st_wallet, Icons.Rounded.CreditCard, "carteira cartão pix pagamento"),
}

/** Tokens: neutral cards, 48dp back/close circles, control outlines ≥ 3:1 on the card. */
private object SettingsTone {
    @Composable fun darkMode() = when (AppAppearance.mode) { "dark" -> true; "light" -> false; else -> isSystemInDarkTheme() }
    @Composable fun card() = if (darkMode()) Color(0xFF151A24) else Color(0xFFF3F4F7)
    @Composable fun edge() = MaterialTheme.colorScheme.outlineVariant
    /** Measured: light #646D80 on the card #F3F4F7 = 4.9:1 (5.4:1 on white); dark #6B7790 on #151A24 = 3.9:1. */
    @Composable fun outline() = if (darkMode()) Color(0xFF6B7790) else Color(0xFF646D80)
    val radius = 16.dp
}

@Composable
fun SettingsScreen(model: ZoenViewModel, state: AppState, page: String?, navigate: (NavKey) -> Unit, back: () -> Unit, close: () -> Unit) {
    when (val p = page?.let { runCatching { SettingsPage.valueOf(it) }.getOrNull() }) {
        null -> SettingsHome(state, navigate, close)
        SettingsPage.Profile -> ProfilePage(state, navigate, back, close)
        SettingsPage.Preferences -> PreferencesPage(state, navigate, back, close)
        SettingsPage.Theme -> ThemePage(back, close)
        SettingsPage.Security -> SecurityPage(model, state, navigate, back, close)
        SettingsPage.Devices -> DevicesPage(model, state, back, close)
        SettingsPage.TwoStep -> TwoStepPage(back, close)
        SettingsPage.Data -> DataPage(model, state, navigate, back, close)
        SettingsPage.Backup -> BackupPage(model, state, back, close)
        SettingsPage.Accessibility -> AccessibilityPage(back, close)
        SettingsPage.Legal -> SimplePage(p, back, close) { SettingsEmpty(Icons.Rounded.Description, R.string.st_legal_title, R.string.st_legal_detail); Caption(R.string.st_legal_caption) }
        SettingsPage.Feedback -> FeedbackPage(back, close)
        SettingsPage.About -> AboutPage(navigate, back, close)
        SettingsPage.Tasks -> SimplePage(p, back, close) { SettingsEmpty(Icons.Rounded.EventRepeat, R.string.st_tasks_empty, R.string.st_tasks_detail) }
        SettingsPage.Disappearing -> DisappearingPage(back, close)
        SettingsPage.Plans -> PlansPage(state, navigate, back, close)
        SettingsPage.Discover -> SimplePage(p, back, close) { SettingsEmpty(Icons.Rounded.Explore, R.string.st_discover_empty, R.string.st_discover_detail) }
        SettingsPage.Moderation -> SimplePage(p, back, close) { SettingsEmpty(Icons.Rounded.Flag, R.string.st_moderation_empty, R.string.st_moderation_detail) }
        SettingsPage.Invites -> InvitesPage(model, state, back, close)
        SettingsPage.Wallet -> SimplePage(p, back, close) { SettingsEmpty(Icons.Rounded.CreditCard, R.string.st_wallet_empty, R.string.st_wallet_detail); Caption(R.string.st_wallet_caption) }
    }
}

// region Kit

@Composable
private fun SettingsScaffold(title: String, back: (() -> Unit)?, close: () -> Unit, bottom: (@Composable () -> Unit)? = null, content: @Composable ColumnScope.() -> Unit) {
    val haptics = rememberZoenHaptics()
    Scaffold(
        topBar = {
            Row(Modifier.fillMaxWidth().statusBarsPadding().padding(horizontal = 12.dp, vertical = 6.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                if (back != null) CircleButton(Icons.AutoMirrored.Rounded.ArrowBack, stringResource(R.string.st_back), "settings:back") { haptics.perform(ZoenFeedback.Dismiss); back() }
                Text(title, Modifier.weight(1f).semantics { heading() }, style = MaterialTheme.typography.titleLarge, maxLines = 1, overflow = TextOverflow.Ellipsis)
                CircleButton(Icons.Rounded.Close, stringResource(R.string.st_close), "settings:close") { haptics.perform(ZoenFeedback.Dismiss); close() }
            }
        },
        bottomBar = { bottom?.let { Box(Modifier.fillMaxWidth().navigationBarsPadding().imePadding().padding(16.dp)) { it() } } },
    ) { padding ->
        Column(
            Modifier.padding(padding).fillMaxSize().verticalScrollCompat().padding(horizontal = 16.dp).padding(top = 8.dp, bottom = 24.dp).widthIn(max = 560.dp),
            verticalArrangement = Arrangement.spacedBy(16.dp),
            content = content,
        )
    }
}

@Composable
private fun Modifier.verticalScrollCompat(): Modifier = this.then(Modifier.verticalScroll(androidx.compose.foundation.rememberScrollState()))

/** The unified 48dp back / close circle. Press scales to 0.96. */
@Composable
private fun CircleButton(icon: ImageVector, description: String, tag: String, onClick: () -> Unit) {
    val interactions = remember { MutableInteractionSource() }
    val pressed by interactions.collectIsPressedAsState()
    val scale by animateFloatAsState(if (pressed) 0.96f else 1f, tween(150), label = "press")
    Box(
        Modifier.size(48.dp).scale(scale).clip(CircleShape).background(SettingsTone.card()).border(0.5.dp, SettingsTone.edge(), CircleShape)
            .clickable(interactions, settingsRipple(), role = Role.Button, onClickLabel = description, onClick = onClick).testTag(tag),
        contentAlignment = Alignment.Center,
    ) { Icon(icon, description, Modifier.size(20.dp)) }
}

@Composable private fun settingsRipple() = androidx.compose.material3.ripple()

@Composable
private fun Group(header: Int? = null, content: @Composable ColumnScope.() -> Unit) {
    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
        header?.let { Text(stringResource(it), Modifier.padding(start = 4.dp).semantics { heading() }, style = MaterialTheme.typography.labelLarge, color = MaterialTheme.colorScheme.onSurfaceVariant) }
        Column(Modifier.fillMaxWidth().clip(RoundedCornerShape(SettingsTone.radius)).background(SettingsTone.card()).border(0.5.dp, SettingsTone.edge(), RoundedCornerShape(SettingsTone.radius)), content = content)
    }
}

@Composable private fun RowDivider() = HorizontalDivider(Modifier.padding(start = 16.dp), thickness = 0.5.dp, color = SettingsTone.edge())

@Composable
private fun Caption(text: Int, vararg args: Any) {
    Text(stringResource(text, *args), Modifier.padding(horizontal = 4.dp), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
}

@Composable
private fun SRow(icon: ImageVector?, title: String, subtitle: String? = null, value: String? = null, tint: Color = Color.Unspecified, chevron: Boolean = true, enabled: Boolean = true, tag: String? = null, onClick: (() -> Unit)? = null) {
    val haptics = rememberZoenHaptics()
    val clickable = onClick != null && enabled
    Row(
        Modifier.fillMaxWidth().heightIn(min = 52.dp)
            .then(if (clickable) Modifier.clickable(role = Role.Button) { haptics.perform(ZoenFeedback.Tap); onClick!!() } else Modifier)
            .then(tag?.let { Modifier.testTag(it) } ?: Modifier)
            .padding(horizontal = 16.dp, vertical = if (subtitle != null) 8.dp else 0.dp),
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        val alpha = if (enabled) 1f else 0.45f
        icon?.let { Icon(it, null, Modifier.size(20.dp), tint = (if (tint == Color.Unspecified) MaterialTheme.colorScheme.onSurfaceVariant else tint).copy(alpha = alpha)) }
        Column(Modifier.weight(1f)) {
            Text(title, style = MaterialTheme.typography.bodyLarge, fontWeight = FontWeight.Medium, color = (if (tint == Color.Unspecified) MaterialTheme.colorScheme.onSurface else tint).copy(alpha = alpha))
            subtitle?.let { Text(it, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = alpha)) }
        }
        value?.let { Text(it, style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = alpha), textAlign = TextAlign.End) }
        if (chevron && clickable) Icon(Icons.Rounded.ChevronRight, null, Modifier.size(18.dp), tint = MaterialTheme.colorScheme.onSurfaceVariant)
    }
}

@Composable
private fun NavRow(page: SettingsPage, navigate: (NavKey) -> Unit, value: String? = null) =
    SRow(page.icon, stringResource(page.title), value = value, tag = "settings:${page.name}") { navigate(SettingsRoute(page.name)) }

/** One of many: ring with a dot. Selection is also in semantics, not only the dot. */
@Composable
private fun RadioRow(title: String, selected: Boolean, subtitle: String? = null, enabled: Boolean = true, tag: String? = null, onClick: () -> Unit) {
    val haptics = rememberZoenHaptics()
    Row(
        Modifier.fillMaxWidth().heightIn(min = 52.dp)
            .selectable(selected, enabled, Role.RadioButton) { if (!selected) haptics.perform(ZoenFeedback.Selection); onClick() }
            .then(tag?.let { Modifier.testTag(it) } ?: Modifier)
            .padding(horizontal = 16.dp, vertical = 6.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        val alpha = if (enabled) 1f else 0.45f
        Column(Modifier.weight(1f)) {
            Text(title, style = MaterialTheme.typography.bodyLarge, fontWeight = FontWeight.Medium, color = MaterialTheme.colorScheme.onSurface.copy(alpha = alpha))
            subtitle?.let { Text(it, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant) }
        }
        val dot by animateFloatAsState(if (selected) 1f else 0f, spring(Spring.DampingRatioNoBouncy, Spring.StiffnessMedium), label = "dot")
        val ring = if (selected) MaterialTheme.colorScheme.onSurface else SettingsTone.outline()
        Box(Modifier.size(20.dp).border(1.5.dp, ring, CircleShape), contentAlignment = Alignment.Center) {
            Box(Modifier.size(10.dp).scale(0.25f + 0.75f * dot).clip(CircleShape).background(MaterialTheme.colorScheme.onSurface.copy(alpha = dot * alpha)))
        }
    }
}

@Composable
private fun PrimaryButton(text: String, enabled: Boolean = true, busy: Boolean = false, tag: String? = null, onClick: () -> Unit) {
    val interactions = remember { MutableInteractionSource() }
    val pressed by interactions.collectIsPressedAsState()
    val scale by animateFloatAsState(if (pressed && enabled) 0.96f else 1f, tween(150), label = "press")
    GlassButton(
        onClick, Modifier.fillMaxWidth().height(50.dp).scale(scale).then(tag?.let { Modifier.testTag(it) } ?: Modifier), enabled = enabled && !busy,
        interactionSource = interactions,
        colors = ButtonDefaults.buttonColors(containerColor = MaterialTheme.colorScheme.onSurface, contentColor = MaterialTheme.colorScheme.surface),
    ) { if (busy) CircularProgressIndicator(Modifier.size(20.dp), strokeWidth = 2.dp, color = MaterialTheme.colorScheme.surface) else Text(text, fontWeight = FontWeight.SemiBold) }
}

/** Honest empty state: what will show up here and how to get it. */
@Composable
private fun SettingsEmpty(icon: ImageVector, title: Int, detail: Int, vararg args: Any) {
    var shown by remember { mutableStateOf(false) }
    LaunchedEffect(Unit) { shown = true }
    AnimatedVisibility(shown, enter = fadeIn(tween(300)) + slideInVertically(tween(300)) { it / 8 }) {
        Column(Modifier.fillMaxWidth().padding(vertical = 40.dp, horizontal = 24.dp), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(10.dp)) {
            Box(Modifier.size(48.dp).clip(CircleShape).background(SettingsTone.card()), contentAlignment = Alignment.Center) { Icon(icon, null, tint = MaterialTheme.colorScheme.onSurfaceVariant) }
            Text(stringResource(title), style = MaterialTheme.typography.titleMedium, textAlign = TextAlign.Center)
            Text(stringResource(detail, *args), style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant, textAlign = TextAlign.Center)
        }
    }
}

@Composable
private fun SimplePage(page: SettingsPage, back: () -> Unit, close: () -> Unit, content: @Composable ColumnScope.() -> Unit) =
    SettingsScaffold(stringResource(page.title), back, close, content = content)

private fun settingsFold(s: String) = Normalizer.normalize(s, Normalizer.Form.NFD).replace(Regex("\\p{M}"), "").lowercase()

// endregion

// region S49 Ajustes

private val sections = listOf(
    R.string.st_account to listOf(SettingsPage.Profile, SettingsPage.Security, SettingsPage.Devices, SettingsPage.Backup, SettingsPage.Data),
    R.string.st_preferences to listOf(SettingsPage.Preferences, SettingsPage.Theme, SettingsPage.Accessibility, SettingsPage.Disappearing),
    R.string.st_zoen to listOf(SettingsPage.Plans, SettingsPage.Invites, SettingsPage.Wallet, SettingsPage.Discover, SettingsPage.Moderation, SettingsPage.Tasks),
    R.string.st_help to listOf(SettingsPage.Feedback, SettingsPage.Legal, SettingsPage.About),
)

@Composable
private fun SettingsHome(state: AppState, navigate: (NavKey) -> Unit, close: () -> Unit) {
    var query by rememberSaveable { mutableStateOf("") }
    val context = LocalContext.current
    val q = settingsFold(query.trim())
    val titles = SettingsPage.entries.associateWith { stringResource(it.title) }
    val results = if (q.isEmpty()) emptyList() else SettingsPage.entries.filter { settingsFold(titles.getValue(it) + " " + it.keywords).contains(q) }
    SettingsScaffold(stringResource(R.string.st_settings), null, close) {
        Row(
            Modifier.fillMaxWidth().height(48.dp).clip(CircleShape).background(SettingsTone.card()).border(1.dp, SettingsTone.outline(), CircleShape).padding(start = 16.dp),
            verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            Icon(Icons.Rounded.Search, null, tint = MaterialTheme.colorScheme.onSurfaceVariant)
            Box(Modifier.weight(1f)) {
                if (query.isEmpty()) Text(stringResource(R.string.st_search), color = MaterialTheme.colorScheme.onSurfaceVariant)
                BasicTextField(query, { query = it }, Modifier.fillMaxWidth().testTag("settings:search"), singleLine = true,
                    textStyle = MaterialTheme.typography.bodyLarge.copy(color = MaterialTheme.colorScheme.onSurface), cursorBrush = SolidColor(MaterialTheme.colorScheme.onSurface))
            }
            if (query.isNotEmpty()) IconButton({ query = "" }, Modifier.size(48.dp)) { Icon(Icons.Rounded.Cancel, stringResource(R.string.st_clear), tint = MaterialTheme.colorScheme.onSurfaceVariant) }
        }
        when {
            q.isEmpty() -> {
                // Identity only from the account, so nothing contradicts "no account".
                state.me?.takeIf { state.account != null }?.let { me ->
                    Row(Modifier.fillMaxWidth().clip(RoundedCornerShape(SettingsTone.radius)).background(SettingsTone.card()).clickable { navigate(SettingsRoute(SettingsPage.Profile.name)) }.padding(16.dp),
                        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(14.dp)) {
                        Avatar(me, size = 52)
                        Column(Modifier.weight(1f)) { Text(me.name, style = MaterialTheme.typography.titleMedium); Text("@${me.handle}", color = MaterialTheme.colorScheme.onSurfaceVariant) }
                        Icon(Icons.Rounded.ChevronRight, null, tint = MaterialTheme.colorScheme.onSurfaceVariant)
                    }
                }
                sections.forEach { (header, pages) -> Group(header) { pages.forEachIndexed { i, p -> if (i > 0) RowDivider(); NavRow(p, navigate) } } }
            }
            results.isEmpty() -> Box(Modifier.testTag("settings:noResults")) { SettingsEmpty(Icons.Rounded.Search, R.string.st_no_results, R.string.st_no_results_detail, query) }
            else -> Group(R.string.st_results) { results.forEachIndexed { i, p -> if (i > 0) RowDivider(); NavRow(p, navigate) } }
        }
    }
}

// endregion

// region Account

@Composable
private fun ProfilePage(state: AppState, navigate: (NavKey) -> Unit, back: () -> Unit, close: () -> Unit) = SettingsScaffold(stringResource(R.string.st_profile), back, close) {
    val acct = state.account
    state.me?.takeIf { acct != null }?.let { me ->
        Column(Modifier.fillMaxWidth(), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Avatar(me, size = 72); Text(me.name, style = MaterialTheme.typography.titleLarge)
        }
    }
    if (acct != null) {
        // Editing reuses the person screen's editor (same fields, same save path).
        val edit = { navigate(Person(acct.identityId)) }
        Group { SRow(Icons.Rounded.PersonOutline, stringResource(R.string.st_name), value = acct.name, onClick = edit); RowDivider(); SRow(Icons.Rounded.AlternateEmail, stringResource(R.string.st_username), value = "@${acct.handle}", onClick = edit) }
        Caption(R.string.st_profile_caption)
    } else SettingsEmpty(Icons.Rounded.AccountCircle, R.string.st_no_account, R.string.st_no_account_detail)
}

@Composable
private fun PreferencesPage(state: AppState, navigate: (NavKey) -> Unit, back: () -> Unit, close: () -> Unit) = SettingsScaffold(stringResource(R.string.st_preferences), back, close) {
    val context = LocalContext.current
    Group {
        NavRow(SettingsPage.Theme, navigate, stringResource(AppAppearance.label()))
        RowDivider()
        SRow(Icons.Rounded.Translate, stringResource(R.string.st_language), value = Locale.getDefault().getDisplayLanguage(Locale.getDefault()).replaceFirstChar { it.titlecase() }, tag = "settings:language") {
            runCatching { context.startActivity(Intent(AndroidSettings.ACTION_APP_LOCALE_SETTINGS, android.net.Uri.fromParts("package", context.packageName, null))) }
                .onFailure { context.startActivity(Intent(AndroidSettings.ACTION_LOCALE_SETTINGS)) }
        }
        RowDivider(); NavRow(SettingsPage.Data, navigate); RowDivider(); NavRow(SettingsPage.Accessibility, navigate)
    }
    Caption(R.string.st_language_caption)
}

@Composable
private fun ThemePage(back: () -> Unit, close: () -> Unit) = SettingsScaffold(stringResource(R.string.st_theme), back, close) {
    val context = LocalContext.current
    Caption(R.string.st_theme_caption)
    Group {
        AppAppearance.modes.forEachIndexed { i, (mode, label) ->
            if (i > 0) RowDivider()
            RadioRow(stringResource(label), AppAppearance.mode == mode, if (mode == "auto") stringResource(R.string.st_follows_device) else null, tag = "theme:$mode") { AppAppearance.set(context, mode) }
        }
    }
}

@Composable
private fun SecurityPage(model: ZoenViewModel, state: AppState, navigate: (NavKey) -> Unit, back: () -> Unit, close: () -> Unit) = SettingsScaffold(stringResource(R.string.st_security), back, close) {
    val backup by produceState<BackupStatusDto?>(null, state.revision) { value = runCatching { model.repository.query { it.backupStatus() } }.getOrNull() }
    val devices by produceState<List<DeviceDto>?>(null) { value = if (state.account == null) null else runCatching { model.repository.network { it.devices() } }.getOrNull() }
    Group {
        NavRow(SettingsPage.TwoStep, navigate, stringResource(R.string.st_off)); RowDivider()
        NavRow(SettingsPage.Backup, navigate, stringResource(if (backup?.enabled == true) R.string.st_on else R.string.st_off)); RowDivider()
        NavRow(SettingsPage.Devices, navigate, devices?.count { !it.revoked }?.toString())
    }
    Caption(R.string.st_security_caption)
}

@Composable
private fun DevicesPage(model: ZoenViewModel, state: AppState, back: () -> Unit, close: () -> Unit) = SettingsScaffold(stringResource(R.string.st_devices), back, close) {
    val result by produceState<Result<List<DeviceDto>>?>(null) { value = if (state.account == null) null else runCatching { model.repository.network { it.devices() } } }
    when {
        state.account == null -> SettingsEmpty(Icons.Rounded.Devices, R.string.st_no_account, R.string.st_devices_empty)
        result == null -> Box(Modifier.fillMaxWidth().padding(40.dp), contentAlignment = Alignment.Center) { CircularProgressIndicator() }
        result!!.isFailure -> SettingsEmpty(Icons.Rounded.WifiOff, R.string.st_devices_failed, R.string.st_devices_empty)
        else -> {
            Caption(R.string.st_devices_caption)
            Group {
                result!!.getOrThrow().filter { !it.revoked }.forEachIndexed { i, d ->
                    if (i > 0) RowDivider()
                    SRow(if (d.thisDevice) Icons.Rounded.PhoneAndroid else Icons.Rounded.Computer, stringResource(if (d.thisDevice) R.string.st_this_device else R.string.st_another_device),
                        subtitle = stringResource(R.string.st_code, d.deviceId.take(8)), value = if (d.thisDevice) stringResource(R.string.st_now) else null)
                }
            }
            Caption(R.string.st_devices_add)
        }
    }
}

@Composable
private fun TwoStepPage(back: () -> Unit, close: () -> Unit) = SettingsScaffold(stringResource(R.string.st_two_step), back, close) {
    Caption(R.string.st_two_step_caption)
    Group {
        SRow(Icons.Rounded.Key, stringResource(R.string.st_authenticator), stringResource(R.string.st_authenticator_sub), stringResource(R.string.st_soon), enabled = false); RowDivider()
        SRow(Icons.Rounded.Sms, stringResource(R.string.st_sms), stringResource(R.string.st_sms_sub), stringResource(R.string.st_soon), enabled = false)
    }
    Caption(R.string.st_two_step_unavailable)
}

@Composable
private fun DataPage(model: ZoenViewModel, state: AppState, navigate: (NavKey) -> Unit, back: () -> Unit, close: () -> Unit) {
    var erase by remember { mutableStateOf(false) }
    val haptics = rememberZoenHaptics()
    SettingsScaffold(stringResource(R.string.st_data), back, close) {
        val stats by produceState<CoreStats?>(null, state.revision) { value = runCatching { model.repository.query { it.stats() } }.getOrNull() }
        val backup by produceState<BackupStatusDto?>(null, state.revision) { value = runCatching { model.repository.query { it.backupStatus() } }.getOrNull() }
        stats?.let { s ->
            Group {
                SRow(Icons.Rounded.Forum, stringResource(R.string.st_chats), value = s.spaces.toString()); RowDivider()
                SRow(Icons.Rounded.Layers, stringResource(R.string.st_items_label), value = s.items.toString()); RowDivider()
                SRow(Icons.Rounded.Groups, stringResource(R.string.st_people_agents), value = s.identities.toString())
            }
        }
        Group { NavRow(SettingsPage.Backup, navigate, stringResource(if (backup?.enabled == true) R.string.st_on else R.string.st_off)) }
        if (state.account != null) {
            Group { SRow(Icons.Rounded.DeleteOutline, stringResource(R.string.st_erase), tint = MaterialTheme.colorScheme.error, chevron = false, tag = "settings:erase") { haptics.perform(ZoenFeedback.Reject); erase = true } }
            Caption(R.string.st_erase_caption)
        }
    }
    if (erase) AlertDialog(onDismissRequest = { erase = false }, title = { Text(stringResource(R.string.st_erase_q)) }, text = { Text(stringResource(R.string.st_erase_detail)) },
        confirmButton = { TextButton({ erase = false; model.launch { model.repository.signOut() } }) { Text(stringResource(R.string.st_erase_confirm), color = MaterialTheme.colorScheme.error) } },
        dismissButton = { TextButton({ erase = false }) { Text(stringResource(R.string.st_cancel)) } })
}

@Composable
private fun BackupPage(model: ZoenViewModel, state: AppState, back: () -> Unit, close: () -> Unit) {
    var status by remember { mutableStateOf<BackupStatusDto?>(null) }
    var ask by remember { mutableStateOf(false) }
    var busy by remember { mutableStateOf(false) }
    val haptics = rememberZoenHaptics()
    LaunchedEffect(state.revision, ask) { status = model.repository.query { it.backupStatus() } }
    SettingsScaffold("Cópia e recuperação", back, close) {
        val s = status
        Group {
            if (s?.enabled == true) {
                SRow(Icons.Rounded.CloudDone, "Cópia ativada", if (s.lastBackupMs > 0) "Sua última cópia foi enviada." else "O envio ainda não foi concluído."); RowDivider()
                SRow(Icons.Rounded.CloudSync, "Fazer cópia agora", enabled = !busy, tag = "settings:backupNow") {
                    busy = true
                    model.launch { try { status = model.repository.network { it.backupNow(model.repository.vault) }; haptics.perform(ZoenFeedback.Confirm) } finally { busy = false } }
                }
            } else {
                SRow(Icons.Rounded.CloudOff, "Cópia desativada", "Suas conversas estão apenas nos seus aparelhos.")
                if (state.account != null && s != null) { RowDivider(); SRow(Icons.Rounded.Key, "Ativar com chave de recuperação", tag = "settings:backupTurnOn") { ask = true } }
            }
        }
        Text("Para recuperar em um aparelho novo, escolha Recuperar minha conta na tela inicial e use sua chave de 64 números. Guarde a chave fora deste aparelho.", style = MaterialTheme.typography.bodySmall)
        if (s?.enabled == true && s.mode == "password") Text("Esta cópia usa senha. Ela foi mantida e você pode continuar fazendo cópias. A recuperação por senha não está disponível nesta tela; mantenha um aparelho conectado.", style = MaterialTheme.typography.bodySmall)
    }
    if (ask) RecoveryKeySetup(model, { ask = false }) { status = it; haptics.perform(ZoenFeedback.Confirm) }
}

// endregion

// region Preferences and help

@Composable
private fun AccessibilityPage(back: () -> Unit, close: () -> Unit) = SettingsScaffold(stringResource(R.string.st_accessibility), back, close) {
    val context = LocalContext.current
    val resolver = context.contentResolver
    val animationsOff = AndroidSettings.Global.getFloat(resolver, AndroidSettings.Global.ANIMATOR_DURATION_SCALE, 1f) == 0f
    val fontScale = LocalDensity.current.fontScale
    val highContrast = runCatching { AndroidSettings.Secure.getInt(resolver, "high_text_contrast_enabled", 0) == 1 }.getOrDefault(false)
    val onText = stringResource(R.string.st_on)
    val offText = stringResource(R.string.st_off)
    val onOff = { on: Boolean -> if (on) onText else offText }
    Group {
        SRow(Icons.Rounded.Animation, stringResource(R.string.st_reduce_motion), value = onOff(animationsOff)); RowDivider()
        SRow(Icons.Rounded.Contrast, stringResource(R.string.st_contrast), value = onOff(highContrast)); RowDivider()
        SRow(Icons.Rounded.FormatSize, stringResource(R.string.st_text_size), value = stringResource(when { fontScale > 1.05f -> R.string.st_larger; fontScale < 0.95f -> R.string.st_smaller; else -> R.string.st_default }))
    }
    Group { SRow(Icons.Rounded.Settings, stringResource(R.string.st_device_settings), tag = "settings:a11yOpen") { context.startActivity(Intent(AndroidSettings.ACTION_ACCESSIBILITY_SETTINGS)) } }
    Caption(R.string.st_a11y_caption)
}

@Composable
private fun FeedbackPage(back: () -> Unit, close: () -> Unit) {
    var text by rememberSaveable { mutableStateOf("") }
    val context = LocalContext.current
    SettingsScaffold(stringResource(R.string.st_feedback), back, close, bottom = {
        PrimaryButton(stringResource(R.string.st_send), enabled = text.isNotBlank(), tag = "settings:feedbackSend") {
            context.startActivity(Intent.createChooser(Intent(Intent.ACTION_SEND).setType("text/plain").putExtra(Intent.EXTRA_TEXT, text), null))
        }
    }) {
        Caption(R.string.st_feedback_caption)
        OutlinedTextField(text, { text = it }, Modifier.fillMaxWidth().heightIn(min = 160.dp).testTag("settings:feedback"), placeholder = { Text(stringResource(R.string.st_feedback_hint)) },
            shape = RoundedCornerShape(SettingsTone.radius), colors = OutlinedTextFieldDefaults.colors(unfocusedBorderColor = SettingsTone.outline(), unfocusedContainerColor = SettingsTone.card(), focusedContainerColor = SettingsTone.card()))
    }
}

@Composable
private fun AboutPage(navigate: (NavKey) -> Unit, back: () -> Unit, close: () -> Unit) = SettingsScaffold(stringResource(R.string.st_about), back, close) {
    Group {
        SRow(Icons.Rounded.Info, stringResource(R.string.st_version), value = "${xyz.tironi.zoen.BuildConfig.VERSION_NAME} (${xyz.tironi.zoen.BuildConfig.VERSION_CODE})"); RowDivider()
        NavRow(SettingsPage.Legal, navigate)
    }
    Caption(R.string.st_about_caption)
}

@Composable
private fun DisappearingPage(back: () -> Unit, close: () -> Unit) = SettingsScaffold(stringResource(R.string.st_disappearing), back, close) {
    Caption(R.string.st_disappearing_caption)
    Group {
        val soon = stringResource(R.string.st_soon)
        RadioRow(stringResource(R.string.st_off), false, soon, enabled = false, tag = "disappear:off") {}; RowDivider()
        RadioRow(stringResource(R.string.st_24h), false, soon, enabled = false, tag = "disappear:24h") {}; RowDivider()
        RadioRow(stringResource(R.string.st_7d), false, soon, enabled = false, tag = "disappear:7d") {}; RowDivider()
        RadioRow(stringResource(R.string.st_90d), false, soon, enabled = false, tag = "disappear:90d") {}
    }
    Caption(R.string.st_disappearing_unavailable)
}

@Composable
private fun PlansPage(state: AppState, navigate: (NavKey) -> Unit, back: () -> Unit, close: () -> Unit) = SettingsScaffold(stringResource(R.string.st_plans), back, close) {
    val mine = if (state.account == null) emptyList() else state.agents.filter { it.persona.isMine }
    val limit = mine.mapNotNull { it.budgetLimitCents }.sum()
    val spent = mine.mapNotNull { it.budgetSpentCents }.sum()
    val money = { cents: Long -> NumberFormat.getCurrencyInstance().format(cents / 100.0) }
    if (limit > 0) {
        val progress by animateFloatAsState((spent.toFloat() / limit).coerceIn(0f, 1f), spring(Spring.DampingRatioNoBouncy, Spring.StiffnessLow), label = "meter")
        Column(Modifier.fillMaxWidth().clip(RoundedCornerShape(SettingsTone.radius)).background(SettingsTone.card()).padding(20.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
            Text(stringResource(R.string.st_agents_month), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
            Text(money(spent), fontSize = 32.sp, fontWeight = FontWeight.Medium)
            LinearProgressIndicator(progress = { progress }, Modifier.fillMaxWidth().height(6.dp).clip(CircleShape), color = MaterialTheme.colorScheme.onSurface, trackColor = MaterialTheme.colorScheme.onSurface.copy(alpha = .1f), drawStopIndicator = {})
            Text(stringResource(R.string.st_budget_of, money(limit)), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
    } else SettingsEmpty(Icons.Rounded.AutoAwesome, R.string.st_no_spend, R.string.st_no_spend_detail)
    Group(R.string.st_your_plan) { SRow(Icons.Rounded.PersonOutline, stringResource(R.string.st_free), stringResource(R.string.st_current_plan), stringResource(R.string.st_active)) }
    Group { NavRow(SettingsPage.Invites, navigate, stringResource(R.string.st_business_free)) }
    Caption(R.string.st_plans_caption)
}

@Composable
private fun InvitesPage(model: ZoenViewModel, state: AppState, back: () -> Unit, close: () -> Unit) {
    val context = LocalContext.current
    val inviteMessage = stringResource(R.string.st_invite_message)
    val link by produceState<String?>(null, state.account) { value = runCatching { model.repository.query { it.friendInviteLink() } }.getOrNull() }
    SettingsScaffold(stringResource(R.string.st_invites), back, close, bottom = {
        PrimaryButton(stringResource(R.string.st_invite_someone), busy = link == null, tag = "settings:inviteShare") {
            val l = link ?: return@PrimaryButton
            context.startActivity(Intent.createChooser(Intent(Intent.ACTION_SEND).setType("text/plain").putExtra(Intent.EXTRA_TEXT, "$inviteMessage $l"), null))
        }
    }) {
        Column(Modifier.fillMaxWidth().clip(RoundedCornerShape(SettingsTone.radius)).background(SettingsTone.card()).padding(20.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Icon(Icons.Rounded.CardGiftcard, null)
            Text(stringResource(R.string.st_invites_title), style = MaterialTheme.typography.titleLarge)
            Text(stringResource(R.string.st_invites_rule), style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
        Caption(R.string.st_invites_caption)
    }
}

// endregion
