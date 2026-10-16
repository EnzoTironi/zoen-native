package xyz.tironi.zoen

/** Explicit test-only clock; ordinary launches always use the system clock. */
object NativeParityFixture {
    const val ID = "native-parity-v1"
    const val FIXED_TIME = "2026-01-15T12:00:00Z"
    const val FIXED_MS = 1768478400000L
    fun nowMs(): Long = if (android.system.Os.getenv("ZOEN_PARITY_FIXTURE") == ID) FIXED_MS else System.currentTimeMillis()
}
