package xyz.tironi.zoen.pages

import android.content.SharedPreferences
import kotlinx.serialization.json.*
import xyz.tironi.zoen.core.PageDto
import xyz.tironi.zoen.core.SecretVault
import xyz.tironi.zoen.data.AndroidSecretVault
import java.util.UUID

data class PageDraft(val content: String, val base: String, val context: String, val revision: Long = 0, val editId: String = UUID.randomUUID().toString()) {
    fun bindBeforeRestore(page: PageDto): PageDraft {
        if (context.isNotEmpty()) return this
        require(page.editContext.isNotEmpty()) { "This page is read only" }
        return copy(context = page.editContext, editId = UUID.randomUUID().toString())
    }

    companion object {
        fun fromPage(page: PageDto, revision: Long = 0): PageDraft {
            val encoded = PageEditing.encode(page.blocks)
            return PageDraft(encoded, encoded, page.editContext, revision)
        }
    }
}

/** Pending keystrokes are device-encrypted before the debounced core edit runs. */
class PageDraftStore internal constructor(private val vault: SecretVault, private val key: String,
                                          private val loadStored: (String) -> ByteArray?) {
    constructor(vault: AndroidSecretVault, key: String) : this(vault, key, vault::loadStored)

    fun load(preferences: SharedPreferences, legacyKey: String): PageDraft? {
        loadStored(key)?.let {
            val draft = decode(it.toString(Charsets.UTF_8))
            clearLegacy(preferences, legacyKey)
            return draft
        }
        val legacy = preferences.getString(legacyKey, null) ?: return null
        val base = preferences.getString("$legacyKey:base", null) ?: legacy
        val draft = PageDraft(legacy, base, "")
        require(PageEditing.decode(legacy) != null && PageEditing.decode(base) != null) { "The saved draft could not be read" }
        save(draft)
        clearLegacy(preferences, legacyKey)
        return draft
    }

    fun save(draft: PageDraft) {
        val source = buildJsonObject {
            put("content", draft.content); put("base", draft.base)
            put("context", draft.context); put("revision", draft.revision)
            put("editId", draft.editId)
        }.toString()
        check(vault.save(key, source.toByteArray(Charsets.UTF_8))) { "Could not save the draft on this device" }
    }

    fun delete() {
        vault.delete(key)
        check(loadStored(key) == null) { "Could not remove the previous draft" }
    }

    fun replaceAfterRestore(preferences: SharedPreferences, legacyKey: String, draft: PageDraft?) {
        if (draft != null) save(draft)
        clearLegacy(preferences, legacyKey)
        if (draft == null) delete()
    }

    private fun clearLegacy(preferences: SharedPreferences, legacyKey: String) {
        if (preferences.contains(legacyKey) || preferences.contains("$legacyKey:base")) {
            check(preferences.edit().remove(legacyKey).remove("$legacyKey:base").commit()) { "Could not finish moving the draft" }
        }
    }

    private fun decode(source: String): PageDraft {
        val value = Json.parseToJsonElement(source).jsonObject
        val content = value.getValue("content").jsonPrimitive.content
        val base = value.getValue("base").jsonPrimitive.content
        require(PageEditing.decode(content) != null && PageEditing.decode(base) != null) { "The saved draft could not be read" }
        return PageDraft(content, base, value.getValue("context").jsonPrimitive.content,
            value["revision"]?.jsonPrimitive?.long ?: 0, value.getValue("editId").jsonPrimitive.content)
    }
}
