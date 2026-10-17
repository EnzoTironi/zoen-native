package xyz.tironi.zoen.ui

import org.junit.Assert.*
import org.junit.Test

class AppleChromeMetricsTest {
    @Test fun bottomBarMatchesAppleSource() {
        assertEquals(56, AppleChromeMetrics.Circle)
        assertEquals(62, AppleChromeMetrics.BarHeight)
        assertEquals(10, AppleChromeMetrics.Gap)
        assertEquals(16, AppleChromeMetrics.Inset)
        assertEquals(2, AppleChromeMetrics.Bottom)
    }
    @Test fun composerKeepsDistinctAttachmentAndTrailingSizes() {
        assertEquals(44, AppleChromeMetrics.Attachment)
        assertEquals(48, AppleChromeMetrics.ComposerHeight)
        assertEquals(230L, AppleChromeMetrics.ComposerHoldMillis)
        assertEquals(700L, AppleChromeMetrics.PlusHoldMillis)
    }
    @Test fun fourItemFanUsesAppleDerivedRadius() {
        assertEquals(106.9521f, fanRadius(4), .001f)
        assertEquals(64f, fanRadius(1), .001f)
    }
}
