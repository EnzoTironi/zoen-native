package xyz.tironi.zoen.miniapps

import java.time.DayOfWeek
import java.time.ZonedDateTime
import java.time.temporal.TemporalAdjusters
import java.util.concurrent.ConcurrentHashMap
import org.json.JSONArray
import org.json.JSONObject
import xyz.tironi.zoen.core.ItemDetail
import xyz.tironi.zoen.core.formatMoney

/** Validated, local-art-only content shared by live tiles and Android app widgets. */
data class WidgetSnapshot(
    val id: String,
    val appId: String,
    val template: Template,
    val title: String,
    val eyebrow: String? = null,
    val value: String? = null,
    val detail: String? = null,
    val bars: List<Bar> = emptyList(),
    val rows: List<Row> = emptyList(),
    val targetMs: Long? = null,
    val accentHex: String,
    val symbol: String,
    val art: String? = null,
    val sensitive: Boolean = false,
    val deepLink: String,
    val actions: List<Action> = emptyList(),
    val codes: List<String> = emptyList(),
    val places: List<String> = emptyList(),
    val times: List<String> = emptyList(),
    val photo: String? = null,
) {
    enum class Template { STAT, PROGRESS, COUNTDOWN, LIST, CAPTION, TICKET, PHOTO }
    data class Bar(val label: String, val value: Double)
    data class Row(val text: String, val done: Boolean)
    data class Action(val tool: String, val label: String)
    data class Remaining(val days: Long, val hours: Long, val minutes: Long)

    fun remaining(nowMs: Long = System.currentTimeMillis()): Remaining {
        val minutes = ((targetMs ?: nowMs) - nowMs).coerceAtLeast(0) / 60_000
        return Remaining(minutes / 1440, minutes % 1440 / 60, minutes % 60)
    }

    fun toJson(): String = JSONObject().apply {
        put("id", id); put("appId", appId); put("template", template.name.lowercase()); put("title", title)
        put("eyebrow", eyebrow); put("value", value); put("detail", detail); put("targetMs", targetMs)
        put("accentHex", accentHex); put("symbol", symbol); put("art", art); put("sensitive", sensitive)
        put("deepLink", deepLink); put("photo", photo)
        put("bars", JSONArray(bars.map { JSONObject().put("label", it.label).put("value", it.value) }))
        put("rows", JSONArray(rows.map { JSONObject().put("text", it.text).put("done", it.done) }))
        put("actions", JSONArray(actions.map { JSONObject().put("tool", it.tool).put("label", it.label) }))
        put("codes", JSONArray(codes)); put("places", JSONArray(places)); put("times", JSONArray(times))
    }.toString()

    companion object {
        private val ids = Regex("[A-Za-z0-9_.-]{1,128}")
        private val appIds = Regex("[a-z][a-z0-9_.-]{0,63}")
        private val accents = Regex("#[0-9a-fA-F]{6}")
        private val symbols = Regex("[a-z0-9.]{1,40}")
        private val tools = Regex("[a-z_]{1,32}")
        val photos = setOf("hike-tomales", "hike-steep", "hike-lands")
        private val arts = setOf("pet", "pet.asleep", "pet.gone", "globe", "pot", "ballot", "notepad", "trip", "hike")
        private fun clean(value: String?, length: Int = 48): String? {
            if (value == null || Regex("(?i)(://|https?:|file:|content:|javascript:|data:|mailto:)").containsMatchIn(value)) return null
            return String(value.codePoints().limit(length.toLong()).toArray(), 0, value.codePointCount(0, value.length).coerceAtMost(length))
        }
        private fun JSONObject.text(key: String): String? = opt(key)?.takeIf { it is String } as? String
        private fun JSONObject.objects(key: String, limit: Int): List<JSONObject> = optJSONArray(key)?.let { array ->
            (0 until array.length().coerceAtMost(limit)).mapNotNull { array.optJSONObject(it) }
        }.orEmpty()
        private fun JSONObject.texts(key: String, limit: Int, length: Int): List<String> = optJSONArray(key)?.let { array ->
            (0 until array.length().coerceAtMost(limit)).mapNotNull { (array.opt(it) as? String)?.let { value -> clean(value, length) } }
        }.orEmpty()

        fun decode(json: String, expectedItemId: String? = null): WidgetSnapshot? = runCatching {
            require(json.toByteArray().size <= 16_384)
            val obj = JSONObject(json)
            val id = obj.getString("id")
            val appId = obj.getString("appId")
            require(ids.matches(id) && appIds.matches(appId) && (expectedItemId == null || id == expectedItemId))
            val link = obj.getString("deepLink")
            require(link == "zoen://app/$id")
            val accent = obj.getString("accentHex")
            val symbol = obj.getString("symbol")
            require(accents.matches(accent) && symbols.matches(symbol))
            val title = clean(obj.getString("title"))?.takeIf { it.isNotBlank() } ?: error("Invalid title")
            val template = Template.valueOf(obj.getString("template").uppercase())
            val photo = obj.text("photo")?.takeIf { it in photos }
            val codes = obj.texts("codes", 2, 5)
            require(template != Template.PHOTO || photo != null)
            require(template != Template.TICKET || codes.size == 2)
            val bars = obj.objects("bars", 3).mapNotNull { bar ->
                val label = clean(bar.text("label"), 16) ?: return@mapNotNull null
                val progress = bar.optDouble("value", Double.NaN)
                if (progress.isFinite()) Bar(label, progress.coerceIn(0.0, 1.0)) else null
            }
            WidgetSnapshot(id, appId, template, title,
                clean(obj.text("eyebrow")), clean(obj.text("value")), clean(obj.text("detail")), bars,
                obj.objects("rows", 4).mapNotNull { row -> clean(row.text("text"))?.let { Row(it, row.optBoolean("done")) } },
                (obj.opt("targetMs") as? Number)?.toLong()?.takeIf { it in 0..253_402_300_799_999L }, accent, symbol,
                obj.text("art")?.takeIf { it in arts }, obj.optBoolean("sensitive"), link,
                obj.objects("actions", 2).mapNotNull { action ->
                    val tool = action.text("tool")?.takeIf { tools.matches(it) } ?: return@mapNotNull null
                    clean(action.text("label"), 14)?.takeIf { it.isNotBlank() }?.let { Action(tool, it) }
                }, codes, obj.texts("places", 2, 20), obj.texts("times", 2, 8), photo)
        }.getOrNull()

        fun from(item: ItemDetail): WidgetSnapshot? {
            val app = item.app ?: return null
            return MiniAppSnapshots.overrides[item.id]?.takeIf { it.version == item.version }?.let { decode(it.json, item.id) }
                ?: decode(app.snapshotJson, item.id)
        }

        /** The SDK omits identity fields; the host binds them to this Item, never the payload. */
        fun published(item: ItemDetail, json: JSONObject): WidgetSnapshot? {
            val app = item.app ?: return null
            val bound = JSONObject(json.toString()).put("id", item.id).put("appId", app.appId).put("deepLink", "zoen://app/${item.id}")
            return decode(bound.toString(), item.id)
        }
    }
}

object MiniAppSnapshots {
    internal data class Published(val version: UInt, val json: String)
    internal val overrides = ConcurrentHashMap<String, Published>()
    fun publish(item: ItemDetail, json: JSONObject): Boolean {
        val snapshot = WidgetSnapshot.published(item, json) ?: return false
        overrides[item.id] = Published(item.version, snapshot.toJson())
        return true
    }
    fun clear() { overrides.clear() }

    fun tiles(item: ItemDetail, locale: String, nowMs: Long = System.currentTimeMillis()): List<WidgetSnapshot> {
        item.plan?.let { plan ->
            val count = plan.sections.sumOf { it.lines.size }
            val detail = "$count ${if (locale.startsWith("pt")) "itens" else "items"} · ${formatMoney(plan.totalCents, locale)}"
            val snapshot = WidgetSnapshot(item.id, "plan", WidgetSnapshot.Template.CAPTION, item.title,
                detail = detail, accentHex = "#3D7A28", symbol = "list.bullet", art = "notepad", deepLink = "zoen://app/${item.id}")
            return listOfNotNull(WidgetSnapshot.decode(snapshot.toJson(), item.id))
        }
        val primary = WidgetSnapshot.from(item) ?: return emptyList()
        if (item.app?.appId != "hike") return listOf(primary)
        val data = runCatching { JSONObject(item.app!!.viewJson) }.getOrNull() ?: return listOf(primary)
        val pt = locale.startsWith("pt")
        val trails = data.optJSONArray("trails")
        val trailRows = (0 until (trails?.length() ?: 0)).mapNotNull { trails?.optJSONObject(it) }
        val people = trailRows.sumOf { it.optJSONArray("votes")?.length() ?: 0 }
        val decided = data.optString("decided").takeIf { it in setOf("tomales", "steep", "lands") }
        val trail = decided ?: trailRows.maxByOrNull { it.optJSONArray("votes")?.length() ?: 0 }?.optString("id")?.takeIf { it in setOf("tomales", "steep", "lands") } ?: "tomales"
        val name = trailRows.firstOrNull { it.optString("id") == trail }?.optString("name") ?: primary.title
        val photo = primary.copy(template = WidgetSnapshot.Template.PHOTO, title = name, photo = "hike-$trail", targetMs = null, art = null, actions = emptyList())
        val now = java.time.Instant.ofEpochMilli(nowMs).atZone(java.time.ZoneId.systemDefault())
        val saturday = now.with(TemporalAdjusters.nextOrSame(DayOfWeek.SATURDAY)).withHour(8).withMinute(0).withSecond(0).withNano(0)
        val day = (if (saturday <= now) saturday.plusWeeks(1) else saturday).toInstant().toEpochMilli()
        val second = if (decided != null) photo.copy(title = data.optString("title", primary.title), targetMs = data.optLong("dayMs", day))
        else primary.copy(template = WidgetSnapshot.Template.STAT, title = if (pt) "Vote: 3 trilhas" else "Vote: 3 trails", value = people.toString(), detail = if (people == 0) if (pt) "Sem votos ainda" else "No votes yet" else name, art = "ballot", photo = null, actions = emptyList())
        val itinerary = data.optJSONArray("itinerary")
        val route = if ((itinerary?.length() ?: 0) > 1) primary.copy(template = WidgetSnapshot.Template.TICKET,
            title = if (pt) "Carona" else "Ride", codes = listOf(if (pt) "SP" else "SF", if (pt) mapOf("tomales" to "PDS", "steep" to "PGR", "lands" to "JRG")[trail] ?: "TRL" else mapOf("tomales" to "TML", "steep" to "STR", "lands" to "LDE")[trail] ?: "TRL"),
            places = listOf(if (pt) "São Paulo" else "San Francisco", name), times = listOf(itinerary!!.optJSONObject(0).optString("time"), itinerary.optJSONObject(itinerary.length()-1).optString("time")), art = null, photo = null, actions = emptyList()) else null
        return listOfNotNull(photo, second, route).mapNotNull { WidgetSnapshot.decode(it.toJson(), item.id) }
    }
}
