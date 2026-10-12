package xyz.tironi.zoen.miniapps

import org.junit.Assert.*
import org.junit.Test

class NativeGlobeTest {
    @Test fun tappingTheCenterSelectsTheCenterGeographicCoordinates() {
        val center = GlobePoint(25.0, 10.0)
        assertEquals(center, OrthographicGlobe.inverse(0.0, 0.0, center))
        val east = OrthographicGlobe.inverse(.5, 0.0, GlobePoint(0.0, 0.0))!!
        assertEquals(30.0, east.lon, 1e-9)
        assertEquals(0.0, east.lat, 1e-9)
    }

    @Test fun tapOutsideTheSphereAndNonfiniteInputCannotBecomeAGuess() {
        val center = GlobePoint(0.0, 0.0)
        assertNull(OrthographicGlobe.inverse(1.01, 0.0, center))
        assertNull(OrthographicGlobe.inverse(Double.NaN, 0.0, center))
        assertNull(OrthographicGlobe.inverse(0.0, Double.POSITIVE_INFINITY, center))
    }

    @Test fun visibleCoordinatesRoundTripThroughInverseProjection() {
        for (center in listOf(GlobePoint(25.0, 10.0), GlobePoint(-70.0, 170.0), GlobePoint(80.0, -150.0))) {
            for (lat in -80..80 step 20) for (lon in -180..180 step 30) {
                val point = GlobePoint(lat.toDouble(), lon.toDouble())
                val projected = OrthographicGlobe.project(point, center)
                if (projected.z <= .001) continue
                val restored = OrthographicGlobe.inverse(projected.x, projected.y, center)!!
                assertEquals(point.lat, restored.lat, 1e-7)
                assertEquals(0.0, OrthographicGlobe.longitude(point.lon - restored.lon), 1e-7)
            }
        }
    }

    @Test fun greatCircleArcTakesTheShortWayAcrossTheDateLine() {
        val middle = OrthographicGlobe.slerp(GlobePoint(0.0, 170.0), GlobePoint(0.0, -170.0), .5)
        assertEquals(0.0, middle.lat, 1e-9)
        assertEquals(180.0, kotlin.math.abs(middle.lon), 1e-9)
    }

    @Test fun AntipodalAndIdenticalAnswersHaveFiniteRevealArcs() {
        val a = GlobePoint(0.0, 0.0); val b = GlobePoint(0.0, 180.0)
        for (step in 0..10) {
            val point = OrthographicGlobe.slerp(a, b, step / 10.0)
            assertTrue(point.lat.isFinite() && point.lon.isFinite())
            assertTrue(point.lat in -90.0..90.0 && point.lon in -180.0..180.0)
        }
        assertEquals(a, OrthographicGlobe.slerp(a, b, 0.0))
        assertEquals(b, OrthographicGlobe.slerp(a, b, 1.0))
        assertEquals(a, OrthographicGlobe.slerp(a, a, .5))
    }

    @Test fun mapAssetCoordinatesMustBeValidPairs() {
        val parsed = OrthographicGlobe.land("[[0,0,10,10,-20,-20]]")
        assertEquals(3, parsed.first().size)
        assertEquals(GlobePoint(10.0, 10.0), parsed.first()[1])
        for (bad in listOf("[[0,0,10,10,20]]", "[[0,0,181,10,-20,-20]]", "[[0,91,10,10,-20,-20]]")) {
            assertThrows(Exception::class.java) { OrthographicGlobe.land(bad) }
        }
    }
}
