package xyz.tironi.zoen

import android.os.Looper
import android.util.Log
import org.junit.rules.TestWatcher
import org.junit.runner.Description

// DIAGNOSTIC (temporary): samples the main thread so a failure shows what keeps it busy.
class MainThreadSampler : TestWatcher() {
    @Volatile private var running = false
    private val samples = HashMap<String, Int>()
    private var thread: Thread? = null
    private var total = 0

    override fun starting(description: Description) {
        running = true
        val main = Looper.getMainLooper().thread
        thread = Thread {
            while (running) {
                val stack = main.stackTrace
                val key = stack.filter { f ->
                    f.className.startsWith("xyz.tironi") || f.className.startsWith("android.view.") || f.className.startsWith("android.graphics.") ||
                        f.className.startsWith("androidx.compose.runtime.Recomposer") || f.className.startsWith("androidx.compose.ui.platform") || f.className.startsWith("androidx.compose.ui.node")
                }.take(14).joinToString(" < ") { "${it.className.substringAfterLast('.')}.${it.methodName}:${it.lineNumber}" }
                synchronized(samples) { total++; samples[key] = (samples[key] ?: 0) + 1 }
                try { Thread.sleep(4) } catch (_: InterruptedException) { return@Thread }
            }
        }.apply { isDaemon = true; start() }
    }

    override fun failed(e: Throwable, description: Description) {
        synchronized(samples) {
            Log.e("MainSampler", "${description.methodName}: $total samples")
            samples.entries.sortedByDescending { it.value }.take(25).forEach { Log.e("MainSampler", "${it.value} | ${it.key.take(3000)}") }
        }
    }

    override fun finished(description: Description) { running = false; thread?.interrupt() }
}
