package xyz.tironi.zoen.ui

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.PathFillType
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.unit.dp

/** Native semantic silhouettes, not Material chat/grid substitutions.
 * Vector contours remain subject to native screenshot comparison. */
internal object NativeTabGlyphs {
    val Chats = ImageVector.Builder("Chats", 26.dp, 26.dp, 26f, 26f).apply {
        path(fill = SolidColor(Color.Black)) {
            moveTo(3f, 3f); lineTo(23f, 3f); lineTo(23f, 18f)
            lineTo(12f, 18f); lineTo(6f, 23f); lineTo(6f, 18f)
            lineTo(3f, 18f); close()
        }
    }.build()
    val Spaces = ImageVector.Builder("Spaces", 26.dp, 26.dp, 26f, 26f).apply {
        path(fill = null, stroke = SolidColor(Color.Black), strokeLineWidth = 1.8f,
            strokeLineCap = StrokeCap.Round, strokeLineJoin = StrokeJoin.Round,
            pathFillType = PathFillType.NonZero) {
            moveTo(13f, 2f); lineTo(24f, 8f); lineTo(13f, 14f); lineTo(2f, 8f); close()
            moveTo(2f, 13f); lineTo(13f, 19f); lineTo(24f, 13f)
            moveTo(2f, 18f); lineTo(13f, 24f); lineTo(24f, 18f)
        }
    }.build()
    val Bell = ImageVector.Builder("Bell", 26.dp, 26.dp, 26f, 26f).apply {
        path(fill = null, stroke = SolidColor(Color.Black), strokeLineWidth = 1.8f,
            strokeLineCap = StrokeCap.Round, strokeLineJoin = StrokeJoin.Round) {
            moveTo(4f, 19f); curveTo(7f, 16f, 6f, 12f, 7f, 8f)
            curveTo(8f, 2f, 18f, 2f, 19f, 8f)
            curveTo(20f, 12f, 19f, 16f, 22f, 19f); close()
            moveTo(10f, 22f); curveTo(11f, 25f, 15f, 25f, 16f, 22f)
        }
    }.build()
}
