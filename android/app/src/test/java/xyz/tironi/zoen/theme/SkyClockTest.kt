package xyz.tironi.zoen.theme

import org.junit.Assert.*
import org.junit.Test
import kotlin.math.abs

class SkyClockTest {
    @Test fun continuousAcrossAnchorsAndMidnight() {
        SkyClock.hours.forEach { h ->
            val (a, sa) = SkyClock.sample(h - .00001)
            val (b, sb) = SkyClock.sample(h + .00001)
            assertTrue(abs(a.red - b.red) < .00001f)
            assertTrue(abs(a.green - b.green) < .00001f)
            assertTrue(abs(a.blue - b.blue) < .00001f)
            assertTrue(abs(sa - sb) < .00001f)
        }
        assertEquals(SkyClock.sample(-1.0), SkyClock.sample(23.0))
        assertEquals(SkyClock.sample(Double.NaN), SkyClock.sample(12.0))
        assertEquals(1f, SkyClock.sample(0.0).second)
        assertEquals(0f, SkyClock.sample(12.0).second)
    }
}
