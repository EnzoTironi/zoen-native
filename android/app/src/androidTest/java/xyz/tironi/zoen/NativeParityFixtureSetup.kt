package xyz.tironi.zoen

import android.content.Context
import android.system.Os
import androidx.test.platform.app.InstrumentationRegistry
import java.io.File
import java.util.Locale
import java.util.TimeZone

internal object NativeParityFixtureSetup {
    fun install(context: Context) {
        val args = InstrumentationRegistry.getArguments()
        val fixture = requireNotNull(args.getString("parityFixture")) { "parityFixture is required" }
        require(fixture == NativeParityFixture.ID) { "Unknown parity fixture: $fixture" }
        val revision = requireNotNull(args.getString("parityRevision")) { "parityRevision is required" }
        require(revision.matches(Regex("[0-9a-f]{40}"))) { "parityRevision must be a git SHA" }
        Os.setenv("ZOEN_PARITY_FIXTURE", fixture, true)
        Os.setenv("ZOEN_PARITY_REVISION", revision, true)
        Locale.setDefault(Locale.US)
        TimeZone.setDefault(TimeZone.getTimeZone("UTC"))
        val configuration = android.content.res.Configuration(context.resources.configuration)
        configuration.setLocale(Locale.US)
        @Suppress("DEPRECATION")
        context.resources.updateConfiguration(configuration, context.resources.displayMetrics)
        File(context.cacheDir, "device-evidence/android-parity/manifest.json").delete()
    }

    fun attest(context: Context, state: xyz.tironi.zoen.data.AppState) {
        val manifest = NativeParityFixture.appliedManifest(state)
        File(context.cacheDir, "device-evidence/android-parity").apply { mkdirs() }
            .resolve("manifest.json").writeText(manifest.toString(2))
    }
}
