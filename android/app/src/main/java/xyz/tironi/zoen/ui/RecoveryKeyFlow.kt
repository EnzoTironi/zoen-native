package xyz.tironi.zoen.ui

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import androidx.compose.ui.window.SecureFlagPolicy
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.launch
import xyz.tironi.zoen.BuildConfig
import xyz.tironi.zoen.ZoenViewModel
import xyz.tironi.zoen.core.BackupStatusDto

@Composable
private fun RecoveryDialog(title: String, busy: Boolean, dismiss: () -> Unit, content: @Composable ColumnScope.() -> Unit) {
    Dialog(onDismissRequest = { if (!busy) dismiss() }, properties = DialogProperties(
        dismissOnBackPress = !busy, dismissOnClickOutside = false, securePolicy = SecureFlagPolicy.SecureOn,
    )) {
        Surface(shape = MaterialTheme.shapes.large) {
            Column(Modifier.fillMaxWidth().verticalScroll(rememberScrollState()).padding(24.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
                Text(title, style = MaterialTheme.typography.titleLarge)
                content()
                TextButton(onClick = dismiss, enabled = !busy) { Text("Fechar") }
            }
        }
    }
}

@Composable
internal fun RecoveryKeySetup(model: ZoenViewModel, dismiss: () -> Unit, done: (BackupStatusDto) -> Unit) {
    // Never rememberSaveable: the key must not enter Android's saved-instance bundle.
    var key by remember { mutableStateOf("") }
    var confirmation by remember { mutableStateOf("") }
    var confirming by remember { mutableStateOf(false) }
    var busy by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<String?>(null) }
    val scope = rememberCoroutineScope()
    LaunchedEffect(Unit) { key = model.repository.query { it.backupCreateRecoveryKey() } }
    RecoveryDialog("Chave de recuperação", busy, dismiss) {
        Text("Guarde esta chave fora deste aparelho. Ela abre suas conversas se você perder todos os aparelhos. O Zoen não pode recuperá-la por você.")
        if (!confirming) {
            Text(key, fontFamily = FontFamily.Monospace, modifier = Modifier.testTag("recovery-key"))
            Button(onClick = { confirming = true }, enabled = key.isNotEmpty()) { Text("Já guardei a chave") }
        } else {
            Text("Digite a chave que você guardou para conferir antes de ativar a cópia.")
            OutlinedTextField(confirmation, { confirmation = it }, label = { Text("Chave de 64 números") },
                visualTransformation = PasswordVisualTransformation(), enabled = !busy, modifier = Modifier.fillMaxWidth().testTag("recovery-confirmation"))
            TextButton(onClick = { confirming = false; confirmation = "" }, enabled = !busy) { Text("Ver a chave novamente") }
            Button(enabled = !busy && RecoveryKeyInput.matches(key, confirmation), modifier = Modifier.testTag("recovery-activate"), onClick = {
                busy = true; error = null
                scope.launch {
                    try {
                        val status = model.repository.network { it.backupEnableRecoveryKey(key, model.repository.vault) }
                        done(status); key = ""; confirmation = ""; dismiss()
                    } catch (e: Exception) {
                        if (e is CancellationException) throw e
                        error = "Não foi possível concluir a cópia. Sua chave continua nesta tela. Confira a conexão e tente novamente."
                    } finally { busy = false }
                }
            }) { Text(if (busy) "Enviando cópia…" else "Ativar cópia") }
        }
        error?.let { Text(it, color = MaterialTheme.colorScheme.error, modifier = Modifier.testTag("recovery-error")) }
        Text("Se o envio falhar, tente novamente com esta mesma chave. Se fechar esta tela, a chave não ficará guardada no aplicativo.")
    }
}

@Composable
internal fun RecoveryKeyRestore(model: ZoenViewModel, dismiss: () -> Unit, done: () -> Unit) {
    var relay by remember { mutableStateOf(BuildConfig.RELAY_URL) }
    var handle by remember { mutableStateOf("") }
    var key by remember { mutableStateOf("") }
    var busy by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<String?>(null) }
    val scope = rememberCoroutineScope()
    RecoveryDialog("Recuperar minha conta", busy, dismiss) {
        Text("Use a chave de 64 números que você guardou ao ativar a cópia. Não é a senha do aparelho.")
        OutlinedTextField(relay, { relay = it }, label = { Text("Endereço do servidor") }, enabled = !busy, modifier = Modifier.fillMaxWidth())
        OutlinedTextField(handle, { handle = it }, label = { Text("Seu @") }, enabled = !busy, modifier = Modifier.fillMaxWidth())
        OutlinedTextField(key, { key = it }, label = { Text("Chave de 64 números") }, visualTransformation = PasswordVisualTransformation(), enabled = !busy,
            modifier = Modifier.fillMaxWidth().testTag("recovery-restore-key"))
        error?.let { Text(it, color = MaterialTheme.colorScheme.error) }
        Button(enabled = !busy && RecoveryKeyInput.valid(key) && handle.isNotBlank() && RecoveryKeyInput.validRelay(relay), modifier = Modifier.testTag("recovery-restore"), onClick = {
            busy = true; error = null
            scope.launch {
                try {
                    model.repository.restoreWithRecoveryKey(relay.trim(), handle.trim(), key)
                    key = ""; dismiss(); done()
                } catch (e: Exception) {
                    if (e is CancellationException) throw e
                    error = "Não foi possível recuperar a conta. Confira seu @, a chave, o servidor e a conexão. Cópias com senha precisam de um aparelho já conectado."
                } finally { busy = false }
            }
        }) { Text(if (busy) "Recuperando…" else "Recuperar minha conta") }
    }
}
