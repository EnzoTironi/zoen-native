package xyz.tironi.zoen.ui.ink

import kotlin.math.*

internal enum class DoodleArt(val seed: ULong) {
    Pot(11uL), Ballot(18uL), Notepad(19uL), Trip(21uL), Hike(22uL);

    fun strokes(t: Double): List<InkStroke> {
        fun line(vararg p: Pair<Double, Double>, width: Double = .024, color: InkColor = InkPalette.ink,
                 start: Double = 0.0, span: Double = .55, opacity: Double = 1.0, smooth: Boolean = true) =
            InkStroke(p.map { InkPoint(it.first, it.second) }, width = width, color = color, start = start,
                span = span, opacity = opacity, smooth = smooth)
        fun shape(vararg p: Pair<Double, Double>, fill: InkColor, width: Double = .024,
                  start: Double = 0.0, span: Double = .55, smooth: Boolean = true) =
            InkStroke(p.map { InkPoint(it.first, it.second) }, closed = true, fill = fill, width = width,
                start = start, span = span, smooth = smooth)
        fun frac(value: Double) = value - floor(value)
        return when (this) {
            Pot -> buildList {
                repeat(3) { i ->
                    val ph = frac(t * .55 + i.toDouble() / 3)
                    val x = .38 + i * .12
                    val sway = sin(t * 2 + i) * .012
                    add(line(x to .33, x + .03 + sway to .27, x - .03 + sway to .2, x + .02 to .13,
                        width = .018, color = InkPalette.steam, start = .75, span = .25,
                        opacity = (1 - ph) * .9).moved(0.0, -ph * .06))
                }
                val r = frac(t / 2.6)
                val rattle = if (r < .16) -abs(sin(r / .16 * PI * 3)) * .014 else 0.0
                addAll(listOf(
                    line(.06 to .87, .5 to .86, .94 to .875, width = .012, opacity = .45),
                    shape(.21 to .46, .79 to .46, .76 to .73, .66 to .8, .34 to .8, .24 to .73, fill = InkPalette.tomato, width = .026),
                    line(.21 to .53, .11 to .52, .1 to .6, .22 to .61, start = .3),
                    line(.79 to .53, .89 to .52, .9 to .6, .78 to .61, start = .35),
                    line(.3 to .55, .31 to .68, width = .02, color = InkColor.White, start = .6, span = .2, opacity = .85),
                    shape(.23 to .45, .3 to .38, .5 to .35, .7 to .38, .77 to .45, fill = InkPalette.butter, start = .4).moved(0.0, rattle),
                    InkStroke.ellipse(.5, .33, .035, .022, fill = InkPalette.ink, width = .02, start = .55, span = .2).moved(0.0, rattle),
                    line(.16 to .465, .84 to .455, width = .03, start = .2),
                ))
            }
            Ballot -> {
                val drop = min(1.0, frac(t / 2.2) / .6) * .2
                listOf(
                    InkStroke.box(.38, .16, .24, .3, r = .1, fill = InkPalette.paper, width = .02).moved(0.0, drop),
                    line(.44 to .3, .49 to .35, .57 to .24, width = .026, color = InkPalette.mint, start = .3, span = .2, smooth = false).moved(0.0, drop),
                    InkStroke.box(.2, .5, .6, .34, r = .08, fill = InkPalette.sky, start = .2),
                    line(.34 to .5, .66 to .5, width = .04, start = .5, smooth = false),
                    line(.28 to .66, .5 to .65, width = .018, color = InkColor.White, start = .6, opacity = .8),
                )
            }
            Notepad -> buildList {
                val tick = frac(t / 3)
                add(InkStroke.box(.24, .16, .52, .7, r = .06, fill = InkPalette.paper))
                repeat(3) { i -> add(InkStroke.ellipse(.35 + i * .15, .16, .025, .04, closed = false, width = .018, start = .25)) }
                repeat(4) { i ->
                    val y = .33 + i * .13
                    add(InkStroke.box(.31, y - .03, .06, .06, r = .2, width = .016, start = .3 + i * .08, span = .15))
                    add(line(.42 to y, .68 - (i % 2) * .08 to y, width = .016, start = .35 + i * .08, opacity = .65))
                    if (i < tick * 5) add(line(.31 to y - .01, .34 to y + .025, .39 to y - .05,
                        width = .024, color = InkPalette.tomato, start = .6, span = .1, smooth = false))
                }
            }
            Trip -> buildList {
                val bob = sin(t * 1.8) * .012
                val drift = sin(t * .4) * .03
                add(InkStroke.ellipse(.7, .3, .13, .13, fill = InkPalette.butter, width = .022))
                add(shape(.0 to .62, .18 to .44, .34 to .52, .5 to .4, .66 to .62, fill = InkPalette.mint, width = .022, start = .2))
                repeat(3) { i ->
                    val y = .68 + i * .09
                    val ph = t * 1.4 + i
                    val points = (0..6).map { k -> k.toDouble() / 6 to y + sin(ph + k * 1.3) * .012 }
                    add(line(*points.toTypedArray(), width = .018, color = InkPalette.sky, start = .35 + i * .08, span = .3, opacity = .9 - i * .2))
                }
                add(shape(.5 to .7, .74 to .7, .7 to .76, .54 to .76, fill = InkPalette.tomato, width = .02, start = .5, smooth = false).moved(drift, bob))
                add(line(.62 to .7, .62 to .5, width = .018, start = .6, smooth = false).moved(drift, bob))
                add(shape(.63 to .52, .72 to .67, .63 to .67, fill = InkPalette.paper, width = .018, start = .65, smooth = false).moved(drift, bob))
            }
            Hike -> {
                val wave = sin(t * 3) * .015
                listOf(
                    InkStroke.ellipse(.8, .2, .07, .07, fill = InkPalette.butter, width = .02),
                    shape(.42 to .82, .66 to .36, .96 to .82, fill = InkPalette.sky.opacity(.7), width = .022, start = .1, smooth = false),
                    shape(.04 to .84, .38 to .26, .74 to .84, fill = InkPalette.mint, width = .026, start = .2, smooth = false),
                    shape(.31 to .38, .38 to .26, .45 to .38, .4 to .35, .36 to .39, fill = InkPalette.paper, width = .018, start = .35, smooth = false),
                    line(.22 to .82, .44 to .7, .28 to .6, .44 to .5, .36 to .42, width = .02, color = InkPalette.tomato, start = .45, span = .35, smooth = false, opacity = .9),
                    line(.38 to .26, .38 to .12, width = .018, start = .75, span = .1, smooth = false),
                    shape(.385 to .12, .48 + wave to .145, .385 to .17, fill = InkPalette.tomato, width = .016, start = .8, span = .1, smooth = false),
                    line(.02 to .86, .5 to .85, .98 to .86, width = .012, start = .05, opacity = .45),
                )
            }
        }
    }
}
