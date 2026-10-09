package xyz.tironi.zoen

import androidx.test.ext.junit.runners.AndroidJUnit4
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import xyz.tironi.zoen.miniapps.McpPayload
import xyz.tironi.zoen.miniapps.WidgetSnapshot

@RunWith(AndroidJUnit4::class)
class McpSecurityTest {
    @Test fun payloadsRejectBatchesWrongIdsExcessiveNestingAndOversizedMessages() {
        assertNotNull(McpPayload.parse("{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"tools/call\",\"params\":{\"name\":\"pet_feed\",\"arguments\":{}}}"))
        listOf("[]", "{\"jsonrpc\":\"1.0\",\"id\":1,\"method\":\"ping\"}", "{\"jsonrpc\":\"2.0\",\"id\":{},\"method\":\"ping\"}", "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\",\"params\":[]}", "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\",\"result\":{}}", "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\",\"params\":{\"x\":${"[".repeat(30)}1${"]".repeat(30)}}}", " ".repeat(McpPayload.MAX_BYTES + 1)).forEach { assertNull(McpPayload.parse(it)) }
    }
    @Test fun widgetSchemaRejectsUrlsForeignItemsAndUnknownPhotoTemplates() {
        fun json() = JSONObject().put("id", "it_test").put("appId", "pet").put("template", "stat").put("title", "Donkey").put("value", "7").put("accentHex", "#3D7A28").put("symbol", "pawprint.fill").put("deepLink", "zoen://app/it_test")
        assertNotNull(WidgetSnapshot.decode(json().toString(), "it_test"))
        assertNull(WidgetSnapshot.decode(json().toString(), "it_other"))
        assertNull(WidgetSnapshot.decode(json().put("title", "https://evil.example").toString()))
        assertNull(WidgetSnapshot.decode(json().put("deepLink", "zoen://app/it_other").toString()))
        assertNull(WidgetSnapshot.decode(json().put("accentHex", "red;evil").toString()))
        assertNull(WidgetSnapshot.decode(json().put("template", "photo").put("photo", "https://evil.example/a.jpg").toString()))
        assertNull(WidgetSnapshot.decode(json().put("template", "ticket").toString()))
    }
}
