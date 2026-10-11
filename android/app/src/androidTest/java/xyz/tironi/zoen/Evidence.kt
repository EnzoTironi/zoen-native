package xyz.tironi.zoen

import androidx.test.platform.app.InstrumentationRegistry
import java.io.File

/** AGP copies this instrumentation output directory before uninstalling the tested app. */
internal object Evidence {
    private val component = Regex("[A-Za-z0-9][A-Za-z0-9._-]*")

    fun outputFile(subdirectory: String, name: String): File {
        require(component.matches(subdirectory) && component.matches(name)) { "Evidence names must be single path components" }
        val configured = InstrumentationRegistry.getArguments().getString("additionalTestOutputDir")?.takeIf { it.isNotBlank() }
        val root = configured?.let(::File)
            ?: File(InstrumentationRegistry.getInstrumentation().targetContext.cacheDir, "device-evidence")
        return File(File(root, subdirectory), name).apply {
            val directory = checkNotNull(parentFile)
            check(directory.isDirectory || directory.mkdirs()) { "Cannot create test evidence directory: $directory" }
        }
    }
}
