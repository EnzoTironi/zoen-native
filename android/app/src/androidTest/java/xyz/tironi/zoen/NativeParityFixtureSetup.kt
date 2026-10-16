package xyz.tironi.zoen

import android.content.Context
import android.system.Os
import androidx.test.platform.app.InstrumentationRegistry
import java.io.File
import java.util.Locale
import java.util.TimeZone
import org.json.JSONObject

internal object NativeParityFixtureSetup {
    fun install(context: Context) {
        val args = InstrumentationRegistry.getArguments()
        val fixture = requireNotNull(args.getString("parityFixture")) { "parityFixture is required" }
        require(fixture == NativeParityFixture.ID) { "Unknown parity fixture: $fixture" }
        val revision = requireNotNull(args.getString("parityRevision")) { "parityRevision is required" }
        require(revision.matches(Regex("[0-9a-f]{40}"))) { "parityRevision must be a git SHA" }
        Os.setenv("ZOEN_PARITY_FIXTURE", fixture, true)
        Locale.setDefault(Locale.US)
        TimeZone.setDefault(TimeZone.getTimeZone("UTC"))
        val configuration = android.content.res.Configuration(context.resources.configuration)
        configuration.setLocale(Locale.US)
        @Suppress("DEPRECATION")
        context.resources.updateConfiguration(configuration, context.resources.displayMetrics)
        val manifest = JSONObject().put("fixture_id", fixture)
            .put("fixed_time", NativeParityFixture.FIXED_TIME).put("locale", "en_US")
            .put("timezone", "UTC").put("seed", 128).put("source_revision", revision)
        File(context.cacheDir, "device-evidence/android-parity").apply { mkdirs() }
            .resolve("manifest.json").writeText(manifest.toString(2))
    }
}
