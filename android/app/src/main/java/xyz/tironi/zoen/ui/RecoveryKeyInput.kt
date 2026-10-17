package xyz.tironi.zoen.ui

internal object RecoveryKeyInput {
    fun digits(value: String) = value.filter { it in '0'..'9' }
    fun valid(value: String) = digits(value).length == 64 && value.all { it in '0'..'9' || it in " \t\r\n\u000b\u000c" }
    fun matches(key: String, confirmation: String) = valid(key) && valid(confirmation) && digits(key) == digits(confirmation)
    fun validRelay(value: String): Boolean = runCatching {
        val uri = java.net.URI(value.trim())
        uri.host != null && uri.rawUserInfo == null && uri.rawQuery == null && uri.rawFragment == null &&
            (uri.path.isNullOrEmpty() || uri.path == "/") &&
            (uri.scheme == "https" || (uri.scheme == "http" && uri.host in listOf("localhost", "127.0.0.1", "::1", "[::1]")))
    }.getOrDefault(false)
}
