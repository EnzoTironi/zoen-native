package xyz.tironi.zoen

/** Explicit test-only clock; ordinary launches always use the system clock. */
object NativeParityFixture {
    const val ID = "native-parity-v1"
    const val FIXED_TIME = "2026-01-15T12:00:00Z"
    const val FIXED_MS = 1768478400000L
    /** Attests projected core data, not merely requested instrumentation arguments. */
    fun appliedManifest(state: xyz.tironi.zoen.data.AppState): org.json.JSONObject {
        check(android.system.Os.getenv("ZOEN_PARITY_FIXTURE") == ID)
        check(state.ready && state.demo && state.failure == null)
        val me = checkNotNull(state.me)
        check(me.id == "ebb0c4c11b6711870fc165c5966f73739d6729ed463d18f948b54d546db453ec")
        check(me.name == "Enzo" && me.tintHex == "#4F7CFF" && me.initials == "E")
        val marina = state.spaces.single { it.id == "sp_01kf0rdqg0000000000000007d" }
        check(marina.title == "Marina" && marina.lastAtMs == 1768475880000L)
        check(marina.lastPreview == "Leaving at eight. I'll text when I get there 🙂")
        val counterpart = checkNotNull(marina.counterpart)
        check(counterpart.id == "133cfc4240dea1fab2e8a1a50af5ab9fe5dba49214f0f31b5a2f285ee952a4bd")
        check(counterpart.tintHex == "#FF5C8A" && counterpart.initials == "M")
        check(nowMs() == FIXED_MS)
        check(java.util.Locale.getDefault() == java.util.Locale.US)
        check(java.util.TimeZone.getDefault().id == "UTC")
        val revision = checkNotNull(android.system.Os.getenv("ZOEN_PARITY_REVISION"))
        check(revision.matches(Regex("[0-9a-f]{40}")))
        return org.json.JSONObject().put("fixture_id", ID).put("fixed_time", FIXED_TIME)
            .put("locale", "en_US").put("timezone", "UTC").put("seed", 128)
            .put("source_revision", revision)
    }

    fun nowMs(): Long = if (android.system.Os.getenv("ZOEN_PARITY_FIXTURE") == ID) FIXED_MS else System.currentTimeMillis()
}
