package xyz.tironi.zoen.pages

class PageSaveCoordinator {
    private val editors = mutableMapOf<String, suspend () -> Unit>()
    fun register(id: String, save: suspend () -> Unit) { editors[id] = save }
    fun unregister(id: String, save: suspend () -> Unit) { if (editors[id] === save) editors.remove(id) }
    suspend fun flush(id: String) { editors[id]?.invoke() }
}
