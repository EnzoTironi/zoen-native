package xyz.tironi.zoen.ui

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.padding
import androidx.compose.ui.unit.dp
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.launch
import xyz.tironi.zoen.ZoenViewModel
import xyz.tironi.zoen.core.DeviceDto

@Composable
internal fun DeviceConnections(model: ZoenViewModel, accountId: String) {
    val scope = rememberCoroutineScope()
    var devices by remember(accountId) { mutableStateOf<List<DeviceDto>?>(null) }
    var revision by remember(accountId) { mutableIntStateOf(0) }
    var loading by remember(accountId) { mutableStateOf(true) }
    var busy by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<String?>(null) }
    var code by remember { mutableStateOf("") }
    var check by remember { mutableStateOf<Pair<String, String>?>(null) }
    var removing by remember { mutableStateOf<DeviceDto?>(null) }
    LaunchedEffect(accountId, revision) {
        loading = true
        try { devices = model.repository.network { it.devices() }; error = null }
        catch (e: CancellationException) { throw e }
        catch (_: Exception) { error = "Não foi possível atualizar os aparelhos. Tente de novo." }
        finally { loading = false }
    }
    Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        Text("Aparelhos conectados", style = MaterialTheme.typography.titleMedium)
        Text("Aparelhos que estão usando sua conta.", style = MaterialTheme.typography.bodyMedium)
        if (loading) LinearProgressIndicator(Modifier.fillMaxWidth())
        val active = devices?.filter { !it.revoked }
        if (active?.isEmpty() == true) Text("Nenhum aparelho conectado.")
        active?.forEach { device ->
            Card(Modifier.fillMaxWidth(), colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surfaceContainerLow)) {
                ListItem(
                    headlineContent = { Text(if (device.thisDevice) "Este aparelho" else "Outro aparelho") },
                    supportingContent = { Text("Código ${device.deviceId.take(8)}") },
                    trailingContent = { if (device.thisDevice) Text("Agora", style = MaterialTheme.typography.labelMedium) },
                    colors = ListItemDefaults.colors(containerColor = MaterialTheme.colorScheme.surfaceContainerLow),
                )
                if (!device.thisDevice) TextButton(modifier = Modifier.padding(horizontal = 8.dp), onClick = { removing = device }, enabled = !busy && !loading) { Text("Desconectar aparelho", color = MaterialTheme.colorScheme.error) }
            }
        }
        error?.let { Text(it, color = MaterialTheme.colorScheme.error) }
        TextButton(onClick = { revision++ }, enabled = !busy && !loading) { Text("Atualizar aparelhos") }
        Text("Conectar aparelho", style = MaterialTheme.typography.titleMedium)
        Text("No outro aparelho, abra o Zoen e copie o código de conexão. Cole aqui e confira os seis números nas duas telas.")
        OutlinedTextField(value = code, onValueChange = { code = it }, enabled = !busy,
            label = { Text("Código de conexão") }, modifier = Modifier.fillMaxWidth())
        Button(enabled = !busy && code.isNotBlank(), onClick = {
            val candidate = code.trim()
            busy = true
            scope.launch {
                try { check = candidate to model.repository.query { it.linkCodeCheck(candidate) }; error = null }
                catch (e: CancellationException) { throw e }
                catch (_: Exception) { error = "Código inválido. Copie o código do outro aparelho e tente de novo." }
                finally { busy = false }
            }
        }) { Text("Conferir código") }
        if (busy) CircularProgressIndicator()
    }
    check?.let { (approvedCode, digits) ->
        AlertDialog(onDismissRequest = { check = null }, title = { Text("Os números são iguais?") },
            text = { Text("Confira nas duas telas: $digits. Só conecte se forem iguais.") },
            dismissButton = { TextButton(onClick = { check = null }) { Text("Cancelar") } },
            confirmButton = { TextButton(onClick = {
                check = null; busy = true
                scope.launch {
                    try {
                        model.repository.network { it.linkDevice(approvedCode, model.repository.vault) }
                        code = ""; revision++
                    } catch (e: CancellationException) { throw e }
                    catch (_: Exception) { error = "Não foi possível conectar. Confira o código e tente de novo." }
                    finally { busy = false }
                }
            }) { Text("Conectar") } })
    }
    removing?.let { device ->
        AlertDialog(onDismissRequest = { removing = null }, title = { Text("Desconectar aparelho?") },
            text = { Text("Esse aparelho deixará de receber suas mensagens.") },
            dismissButton = { TextButton(onClick = { removing = null }) { Text("Cancelar") } },
            confirmButton = { TextButton(onClick = {
                removing = null; busy = true
                scope.launch {
                    try { model.repository.network { it.unlinkDevice(device.deviceId) }; revision++ }
                    catch (e: CancellationException) { throw e }
                    catch (_: Exception) { error = "Não foi possível desconectar. Tente de novo." }
                    finally { busy = false }
                }
            }) { Text("Desconectar") } })
    }
}
