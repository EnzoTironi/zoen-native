package xyz.tironi.zoen

import android.os.Build
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.toPixelMap
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.test.captureToImage
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.unit.dp
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import xyz.tironi.zoen.ui.ZoenGlassBackdrop
import xyz.tironi.zoen.ui.zoenGlass
import xyz.tironi.zoen.ui.zoenGlassSource

/** Pixel proof: an EMPTY overlay must diffuse a sibling's stripes, not its own contents. */
class GlassBackdropTest {
    @get:Rule val compose = createComposeRule()

    @Test fun lightBackdropDiffusesUnderlayAndApi28RetainsTranslucency() = checkBackdrop(false)
    @Test fun darkBackdropDiffusesUnderlayAndApi28RetainsTranslucency() = checkBackdrop(true)

    private fun checkBackdrop(dark: Boolean) {
        compose.setContent {
            MaterialTheme(colorScheme = if (dark) darkColorScheme() else lightColorScheme()) {
                ZoenGlassBackdrop {
                    Box(Modifier.size(240.dp).testTag("scene")) {
                        Canvas(Modifier.fillMaxSize().zoenGlassSource()) {
                            val stripe = 4.dp.toPx()
                            var x = 0f
                            var index = 0
                            while (x < size.width) {
                                drawRect(if (index++ % 2 == 0) Color.Black else Color.White,
                                    Offset(x, 0f), Size(stripe, size.height))
                                x += stripe
                            }
                        }
                        Box(Modifier.align(Alignment.Center).size(160.dp)
                            .zoenGlass(RoundedCornerShape(24.dp)))
                        // Sharp foreground control is deliberately outside the captured source.
                        Box(Modifier.align(Alignment.Center).size(12.dp).background(Color.Red))
                    }
                }
            }
        }
        compose.waitForIdle()
        val pixels = compose.onNodeWithTag("scene").captureToImage().toPixelMap()
        fun contrast(y: Int): Float {
            val values = (pixels.width / 3 until pixels.width * 2 / 3).map { pixels[it, y].red }
            return values.max() - values.min()
        }
        val raw = contrast(pixels.height / 12)
        val glass = contrast(pixels.height / 3)
        assertTrue("Underlying stripes are present", raw > 0.9f)
        if (Build.VERSION.SDK_INT >= 31) {
            assertTrue("Backdrop, not foreground content, must diffuse: raw=$raw glass=$glass", glass < raw * 0.2f)
            val mean = (pixels.width / 3 until pixels.width * 2 / 3)
                .map { pixels[it, pixels.height / 3].red }.average()
            // A solid tint/no captured source can also erase stripes. Require mixed underlay
            // luminance, proving this is a sampled sibling backdrop rather than an opaque fill.
            val range = if (dark) 0.15..0.40 else 0.55..0.85
            assertTrue("Captured black/white underlay must contribute to tint: $mean", mean in range)
        } else {
            assertTrue("API28–30 unblurred translucent material retains stripe contrast: $glass", glass > 0.04f)
        }
        val center = pixels[pixels.width / 2, pixels.height / 2]
        assertTrue("Foreground must remain sharp red", center.red > 0.95f && center.green < 0.05f)
    }
}
