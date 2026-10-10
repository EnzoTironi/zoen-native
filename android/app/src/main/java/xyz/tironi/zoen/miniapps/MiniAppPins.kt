package xyz.tironi.zoen.miniapps

import android.content.SharedPreferences
import org.json.JSONArray

/** Pins change only this device's presentation. Removing a tile never deletes the Item. */
class MiniAppPins(private val preferences: SharedPreferences) {
    fun hidden(scope: String = HOME): Set<String> = preferences.getStringSet("miniapps.hidden:$scope", emptySet()).orEmpty().toSet()
    fun order(scope: String = HOME): List<String> = runCatching {
        val values = JSONArray(preferences.getString("miniapps.order:$scope", "[]"))
        (0 until values.length()).mapNotNull { values.opt(it) as? String }.distinct()
    }.getOrDefault(emptyList())
    fun hide(id: String, scope: String = HOME) {
        preferences.edit().putStringSet("miniapps.hidden:$scope", hidden(scope) + id).apply()
    }
    fun show(id: String, scope: String = HOME) {
        preferences.edit().putStringSet("miniapps.hidden:$scope", hidden(scope) - id).apply()
    }
    fun reorder(ids: List<String>, scope: String = HOME) {
        preferences.edit().putString("miniapps.order:$scope", JSONArray(ids.distinct()).toString()).apply()
    }
    fun <T> visible(items: List<T>, id: (T) -> String, scope: String = HOME): List<T> {
        val hidden = hidden(scope)
        val order = order(scope).withIndex().associate { it.value to it.index }
        return items.filter { id(it).substringBefore('#') !in hidden }
            .withIndex().sortedWith(compareBy<IndexedValue<T>> { order[id(it.value)] ?: Int.MAX_VALUE }.thenBy { it.index })
            .map { it.value }
    }
    companion object {
        const val HOME = "home"
        fun chat(spaceId: String) = "chat:$spaceId"
    }
}
