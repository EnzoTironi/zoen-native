package xyz.tironi.zoen

import android.graphics.drawable.AnimatedVectorDrawable
import android.graphics.drawable.VectorDrawable
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

/** Exercise Android's real XML inflater/property animators, not just source wiring. */
@RunWith(AndroidJUnit4::class)
class NativeAnimatedDrawableTest {
    @Test fun everyProductionGlyphInflatesAndRunsNativeEffects() {
        val app = ApplicationProvider.getApplicationContext<ZoenApplication>()
        val names = app.resources.getStringArray(R.array.native_symbol_names)
        InstrumentationRegistry.getInstrumentation().runOnMainSync {
            for (name in names) {
                val still = app.getDrawable(app.resources.getIdentifier("native_$name", "drawable", app.packageName))
                assertTrue("$name is a native vector", still is VectorDrawable)
                for (effect in listOf("bounce", "wiggle", "active", "draw")) {
                    val id = app.resources.getIdentifier("native_${name}_$effect", "drawable", app.packageName)
                    val vector = app.getDrawable(id)
                    assertTrue("$name/$effect is an AnimatedVectorDrawable", vector is AnimatedVectorDrawable)
                    (vector as AnimatedVectorDrawable).start()
                    vector.stop()
                }
            }
        }
    }
}
