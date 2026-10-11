package xyz.tironi.zoen.ui.ink

import kotlin.math.*
import xyz.tironi.zoen.ui.MascotPose
import xyz.tironi.zoen.ui.ink.MascotArt.pt
import xyz.tironi.zoen.ui.ink.MascotArt.polar
import xyz.tironi.zoen.ui.ink.MascotArt.frac
import xyz.tironi.zoen.ui.ink.MascotArt.turn
import xyz.tironi.zoen.ui.ink.MascotArt.hop
import xyz.tironi.zoen.ui.ink.MascotArt.curve
import xyz.tironi.zoen.ui.ink.MascotArt.blob
import xyz.tironi.zoen.ui.ink.MascotArt.oval
import xyz.tironi.zoen.ui.ink.MascotArt.dot
import xyz.tironi.zoen.ui.ink.MascotArt.leaf
import xyz.tironi.zoen.ui.ink.MascotArt.draw
import xyz.tironi.zoen.ui.ink.MascotArt.blinking
import xyz.tironi.zoen.ui.ink.MascotArt.body
import xyz.tironi.zoen.ui.ink.MascotArt.belly
import xyz.tironi.zoen.ui.ink.MascotArt.line
import xyz.tironi.zoen.ui.ink.MascotArt.accent

internal object MascotPoses {
    fun strokes(pose: MascotPose, t: Double): List<InkStroke> {
        val r = MascotRig()
        val behind = arrayListOf<InkStroke>(); val held = arrayListOf<InkStroke>(); val world = arrayListOf<InkStroke>()
        r.squash = sin(t * 2.0) * 0.022; r.cowlick = 0.12 * sin(t * 2.0 - 1.0)
        fun turnTo(target: Double, every: Double? = null, glanceTo: Double = 0.12) {
            val (yaw, glance) = turn(target, t, every, glanceTo); r.yaw = yaw; r.glance = glance
        }
        fun bounce(w: Double, height: Double) { val (lift, squash) = hop(w, height); r.lift = lift; r.squash += squash }
        when (pose) {
            MascotPose.Wave -> {
                turnTo(-0.18); bounce(t * 2.2, 0.008)
                val soft = frac(t / 4.6) > 0.72
                r.brow = if (soft) 0.25 else 0.8; r.lid = if (soft) 0.18 else 0.32
                r.mouth = if (soft) MascotMouth.Smirk else MascotMouth.Pout
                r.look = pt(0.1, 0.05); r.tilt = -0.04
                val wv = sin(t * 6.5); val hand = polar(r.shoulderR, 0.11, -1.0 + 0.4 * wv)
                r.armR = hand; r.cowlick = 0.18 * sin(t * 6.5 - 1.3)
                repeat(2) { k ->
                    val rad = 0.065 + k * 0.02
                    held.add(curve(listOf(polar(hand, rad, -1.6), polar(hand, rad + 0.005, -1.1), polar(hand, rad, -0.6)), width = 0.01, start = 0.7, span = 0.15, opacity = 0.3 + 0.5 * abs(wv)))
                }
            }
            MascotPose.Map -> {
                turnTo(0.5, 4.2); r.cx = 0.42; r.lid = 0.5; r.slant = 0.3; r.brow = 0.9
                r.look = pt(0.85, 0.55 + 0.15 * sin(t * 0.8)); r.tilt = 0.05
                r.mouth = if (frac(t / 3.6) > 0.8) MascotMouth.O else MascotMouth.Flat
                val wob = 0.05 * sin(t * 1.4); val o = pt(0.76, 0.62)
                val m = arrayListOf(
                    blob(listOf(pt(0.6, 0.53), pt(0.7, 0.5), pt(0.8, 0.54), pt(0.92, 0.5), pt(0.93, 0.74), pt(0.81, 0.78), pt(0.71, 0.74), pt(0.61, 0.77)), InkPalette.paper, start = 0.35, span = 0.3, smooth = false),
                    blob(listOf(pt(0.62, 0.55), pt(0.7, 0.52), pt(0.71, 0.62), pt(0.63, 0.65)), belly.opacity(0.6), width = 0.001, color = InkColor.Clear, start = 0.55, misregister = 0.6),
                    curve(listOf(pt(0.7, 0.5), pt(0.71, 0.74)), width = 0.01, opacity = 0.5),
                    curve(listOf(pt(0.8, 0.54), pt(0.81, 0.78)), width = 0.01, start = 0.52, opacity = 0.5))
                val route = listOf(pt(0.65, 0.71), pt(0.69, 0.66), pt(0.73, 0.68), pt(0.77, 0.63), pt(0.81, 0.65), pt(0.85, 0.6), pt(0.88, 0.58))
                val shown = 2 + (frac(t / 2.6) * 6).toInt()
                repeat(min(shown, route.lastIndex)) { i -> val p = route[i]; m.add(curve(listOf(p, pt(p.x + 0.013, p.y - 0.005)), width = 0.01, color = accent, start = 0.6, span = 0.1)) }
                val x = route.last()
                m.add(curve(listOf(pt(x.x - 0.015, x.y - 0.015), pt(x.x + 0.015, x.y + 0.015)), width = 0.014, color = accent, start = 0.65, span = 0.1))
                m.add(curve(listOf(pt(x.x + 0.015, x.y - 0.015), pt(x.x - 0.015, x.y + 0.015)), width = 0.014, color = accent, start = 0.68, span = 0.1))
                val u = 1 - (1 - min(1.0, t / 1.1)).pow(3)
                held.addAll(m.map { s -> s.copy(points = s.points.map { pt(0.6 + (it.x - 0.6) * (0.12 + 0.88 * u), it.y + (1 - u) * (it.y - 0.64) * -0.2) }).rotated(wob, o) })
                r.armR = pt(0.62, 0.64)
                if (u < 1) { r.mouth = MascotMouth.O; r.look = pt(0.9, 0.3) }
            }
            MascotPose.Juggle -> {
                turnTo(-0.25, 5.0); bounce(t * 4.2, 0.016); r.brow = 0.55; r.lid = 0.15
                r.mouth = if (frac(t / 3) < 0.5) MascotMouth.O else MascotMouth.Flat
                r.tilt = 0.03 * sin(t * 2.8)
                val colors = listOf(InkPalette.butter, InkPalette.sky, InkPalette.blush); var topCard = pt(0.5, 1.0)
                repeat(3) { i ->
                    val a = t * 2.8 + i * 2.094; val p = pt(0.5 + 0.2 * cos(a), 0.27 + 0.1 * sin(a))
                    if (p.y < topCard.y) topCard = p
                    val card = listOf(InkStroke.box(p.x - 0.04, p.y - 0.052, 0.08, 0.104, r = 0.22, fill = colors[i], width = 0.014, color = line, start = 0.55, span = 0.2), oval(p, 0.014, 0.014, body, width = 0.008, start = 0.65, n = 8))
                    world.addAll(card.map { it.rotated(a * 0.5, p) })
                }
                r.look = pt((topCard.x - 0.5).times(4).coerceIn(-1.0, 1.0), -1.0)
                r.armL = pt(r.cx - 0.2, r.cy - 0.16 + 0.05 * sin(t * 5.6)); r.armR = pt(r.cx + 0.2, r.cy - 0.16 - 0.05 * sin(t * 5.6))
                r.cowlick = 0.25 * sin(t * 5.6)
            }
            MascotPose.Run -> {
                turnTo(1.0); r.wind = 1.0; val w = t * 10; val h = abs(sin(w))
                r.lift = h * 0.035; r.squash += (1 - h).pow(6) * 0.13 - h * 0.06
                r.tilt = 0.14; r.headband = true; r.brow = 1.0; r.lid = 0.32; r.slant = 0.7
                r.look = pt(0.9, 0.0); r.mouth = MascotMouth.Flat
                r.stepL = pt(sin(w) * 0.06, -max(0.0, cos(w)) * 0.035); r.stepR = pt(-sin(w) * 0.06, -max(0.0, -cos(w)) * 0.035)
                r.armL = pt(r.shoulderL.x - 0.035 - cos(w) * 0.012, r.shoulderL.y + 0.01 + sin(w) * 0.045)
                r.armR = pt(r.shoulderR.x + 0.035 + cos(w) * 0.012, r.shoulderR.y + 0.01 - sin(w) * 0.045)
                r.cowlick = -0.6 + 0.15 * sin(w * 2)
                repeat(3) { k ->
                    val ph = frac(t * 1.6 + k.toDouble() / 3); val c = pt(0.3 - ph * 0.2, 0.84 - ph * 0.05); val rr = 0.02 + ph * 0.04
                    world.add(InkStroke.ellipse(c.x, c.y, rr, rr * 0.75, closed = false, width = 0.01, color = InkColor(0xb49a78), span = 0.0, n = 9, opacity = (1 - ph) * 0.8))
                }
                repeat(3) { k -> val x = frac(k.toDouble() / 3 - t * 1.2); world.add(InkStroke(listOf(pt(x, 0.885), pt(x + 0.12, 0.885)), width = 0.01, color = line, span = 0.0, smooth = false, opacity = 0.35)) }
                val sp = frac(t * 1.3); val d = pt(0.33 - sp * 0.12, 0.4 - sin(sp * PI) * 0.06)
                world.add(blob(listOf(pt(d.x, d.y - 0.02), pt(d.x + 0.012, d.y + 0.004), pt(d.x, d.y + 0.014), pt(d.x - 0.012, d.y + 0.004)), InkPalette.sky, width = 0.008, start = 0.7, span = 0.1, opacity = 1 - sp))
            }
            MascotPose.Phone -> {
                turnTo(0.55, 3.6); r.cx = 0.4; val ringing = frac(t / 2.4) < 0.5
                val pb = pt(0.81, 0.86); val shake = if (ringing) 0.12 * sin(t * 70) else 0.0
                val ph = listOf(InkStroke.box(0.75, 0.62, 0.12, 0.24, r = 0.24, fill = line, width = 0.014, color = line, start = 0.4),
                    InkStroke.box(0.762, 0.642, 0.096, 0.19, r = 0.18, fill = InkPalette.sky, width = 0.007, color = line, start = 0.5),
                    InkStroke.box(0.77, 0.67, 0.08, 0.035, r = 0.4, fill = InkColor.White, width = 0.006, color = line, start = 0.6),
                    dot(pt(0.783, 0.687), 0.009, accent, start = 0.65))
                behind.addAll(ph.map { it.rotated(shake, pb) })
                if (ringing) {
                    repeat(2) { k ->
                        val rad = 0.1 + k * 0.034; val o = pt(0.81, 0.72); val op = 0.5 + 0.5 * sin(t * 20 + k)
                        world.add(curve(listOf(polar(o, rad, -0.5), polar(o, rad + 0.005, 0.0), polar(o, rad, 0.5)), width = 0.012, color = accent, start = 0.6, span = 0.1, opacity = op))
                        world.add(curve(listOf(polar(o, rad, PI - 0.5), polar(o, rad + 0.005, PI), polar(o, rad, PI + 0.5)), width = 0.012, color = accent, start = 0.6, span = 0.1, opacity = op))
                    }
                    r.brow = 1.0; r.lid = 0.35; r.slant = 0.8; r.mouth = MascotMouth.Grit; r.tilt = -0.07; r.look = pt(1.0, 0.5)
                    r.armL = pt(r.shoulderL.x - 0.02, r.cy - 0.1); r.armR = pt(r.shoulderR.x + 0.03, r.cy - 0.09)
                    r.squash += sin(t * 50) * 0.012; r.cowlick = 0.3 * sin(t * 40)
                    val m = pt(r.cx + r.rx * 0.9, r.top + 0.03)
                    repeat(4) { k -> val a = k * PI / 2 + PI / 4; world.add(curve(listOf(polar(m, 0.012, a - 0.5), polar(m, 0.026, a), polar(m, 0.012, a + 0.5)), width = 0.01, color = accent, start = 0.6, span = 0.1)) }
                } else {
                    r.look = pt(1.0, 0.6); r.tilt = 0.06; r.lid = 0.45; r.slant = 0.6; r.mouth = MascotMouth.Pout
                    r.armR = polar(r.shoulderR, 0.1, 0.2)
                }
            }
            MascotPose.Walk -> {
                turnTo(1.0, 5.2, 0.3); r.wind = 0.35; r.cx = 0.42; val w = t * 6.2; val h = abs(sin(w))
                r.lift = h * 0.016; r.squash += (1 - h).pow(6) * 0.1 - h * 0.03
                r.stepL = pt(sin(w) * 0.04, -max(0.0, cos(w)) * 0.022); r.stepR = pt(-sin(w) * 0.04, -max(0.0, -cos(w)) * 0.022)
                r.armL = pt(r.shoulderL.x - 0.03 - sin(w) * 0.03, r.shoulderL.y + 0.035); r.armR = pt(r.shoulderR.x + 0.03 + sin(w) * 0.03, r.shoulderR.y + 0.035)
                r.tilt = 0.06; r.look = pt(1.0, -0.1); r.brow = 0.6; r.lid = 0.25; r.mouth = MascotMouth.Flat; r.cowlick = -0.4 + 0.15 * sin(w * 2)
                repeat(5) { k -> val x = frac(k.toDouble() / 5 - t * 0.45); val edge = min(1.0, min(x, 1 - x) * 6)
                    world.add(InkStroke(listOf(pt(x * 0.95, 0.885), pt(x * 0.95 + 0.07, 0.886)), width = 0.01, span = 0.0, smooth = false, opacity = 0.45 * edge)) }
                val pc = pt(0.84, 0.3 + sin(t * 3) * 0.015)
                world.add(blob(listOf(pt(pc.x, pc.y + 0.11), pt(pc.x - 0.05, pc.y + 0.03), pt(pc.x - 0.05, pc.y - 0.02), pt(pc.x, pc.y - 0.06), pt(pc.x + 0.05, pc.y - 0.02), pt(pc.x + 0.05, pc.y + 0.03)), accent, start = 0.55, span = 0.25))
                world.add(oval(pt(pc.x, pc.y - 0.005), 0.02, 0.02, InkPalette.paper, width = 0.01, start = 0.7, n = 8))
                val pulse = frac(t / 1.6)
                repeat(2) { k -> val f = frac(pulse + k.toDouble() / 2); val rr = 0.075 + f * 0.05
                    world.add(InkStroke.ellipse(pc.x, pc.y - 0.005, rr, rr, closed = false, width = 0.008, color = accent, start = 0.7, span = 0.1, opacity = (1 - f) * 0.5)) }
            }
            MascotPose.Zen -> {
                r.lid = 0.42; r.slant = 0.45; r.brow = 0.75; r.mouth = MascotMouth.Pout
                r.look = pt(-0.2 + 0.4 * sin(t * 0.5), 0.25); r.squash = sin(t * 1.4) * 0.03
                repeat(3) { k -> behind.add(InkStroke.ellipse(0.5, 0.875, 0.27 + k * 0.08, 0.045 + k * 0.016, closed = false, width = 0.008, color = line, span = 0.4, n = 16, opacity = 0.28)) }
                behind.add(oval(pt(0.16, 0.86), 0.05, 0.026, InkColor(0x9c9a92), width = 0.01, start = 0.2))
                behind.add(oval(pt(0.86, 0.9), 0.035, 0.018, InkColor(0x7f7d76), width = 0.009, start = 0.25, n = 9))
                repeat(4) { i -> val ph = frac(t * 0.11 + i.toDouble() / 4); val x = 1.08 - ph * 1.2
                    val y = 0.12 + i * 0.16 + sin(t * 1.3 + i * 2) * 0.03
                    world.addAll(leaf(pt(x, y), 0.034 + (i % 2) * 0.01, sin(t * 1.7 + i) * 0.7 + 0.3, opacity = min(1.0, min(ph, 1 - ph) * 6))) }
            }
            MascotPose.Roar -> {
                val burst = frac(t / 2.6); val roaring = burst < 0.6
                r.brow = 1.0; r.lid = 0.28; r.slant = 0.9; r.mouth = if (roaring) MascotMouth.Roar else MascotMouth.Grit
                r.wide = roaring; r.look = pt(0.0, 0.1)
                r.armL = pt(r.shoulderL.x - 0.06, r.cy - 0.04); r.armR = pt(r.shoulderR.x + 0.06, r.cy - 0.04)
                if (roaring) {
                    r.cx += sin(t * 55) * 0.004; r.squash = -0.06 + sin(t * 30) * 0.01; r.cowlick = 0.35 * sin(t * 45)
                    repeat(3) { k -> val a = -0.5 + k * 0.5
                        for (side in listOf(-1, 1)) { val o = pt(0.5, r.mouthY); val ang = if (side > 0) a else PI - a
                            world.add(curve(listOf(polar(o, 0.29, ang), polar(o, 0.35 + burst * 0.04, ang)), width = 0.012, start = 0.6, span = 0.1, opacity = 0.6)) } }
                    repeat(3) { i -> val ph = frac(t * 0.9 + i.toDouble() / 3)
                        world.addAll(leaf(pt(0.5 + (if (i % 2 == 0) 1 else -1) * (0.3 + ph * 0.3), 0.3 + i * 0.15), 0.03, t * 6 + i, opacity = 1 - ph)) }
                }
            }
            MascotPose.Shield, MascotPose.Coin, MascotPose.Cheer -> extra(pose, r, t, held, world)
        }
        if (r.glance > 0) r.look = pt(r.look.x + (-r.yaw * 0.7 - r.look.x) * r.glance, r.look.y * (1 - r.glance))
        if (!r.closed && blinking(t) && pose != MascotPose.Roar) r.lid = 1.0
        return draw(r, t, behind, held, world)
    }

    private fun extra(pose: MascotPose, r: MascotRig, t: Double, held: MutableList<InkStroke>, world: MutableList<InkStroke>) {
        when (pose) {
            MascotPose.Shield -> {
                val (yaw, glance) = turn(-0.3, t, 4.0); r.yaw = yaw; r.glance = glance
                r.tilt = 0.03 * sin(t * 1.3); r.brow = 0.9; r.lid = 0.3; r.slant = 0.6
                r.look = pt(-0.8 + 1.6 * if (frac(t / 5) > 0.5) 1 else 0, 0.1); r.mouth = MascotMouth.Flat
                val c = pt(r.cx, r.cy + r.ry * 0.78 + sin(t * 2.1) * 0.004); val w = 0.115; val h = 0.14
                val shield = listOf(pt(c.x - w, c.y - h * 0.55), pt(c.x, c.y - h * 0.7), pt(c.x + w, c.y - h * 0.55), pt(c.x + w * 0.9, c.y + h * 0.1), pt(c.x, c.y + h * 0.62), pt(c.x - w * 0.9, c.y + h * 0.1))
                held.add(blob(shield, InkPalette.sky, width = 0.018, span = 0.25))
                held.add(blob(shield.map { pt(c.x + (it.x - c.x) * 0.72, c.y + (it.y - c.y) * 0.72) }, InkColor(0xdcebf5), width = 0.008, color = line.opacity(0.4), start = 0.55, span = 0.2, misregister = 0.3))
                held.add(curve(listOf(pt(c.x - 0.03, c.y - 0.005), pt(c.x - 0.03, c.y - 0.05), pt(c.x, c.y - 0.07), pt(c.x + 0.03, c.y - 0.05), pt(c.x + 0.03, c.y - 0.005)), width = 0.014, start = 0.62, span = 0.15))
                held.add(InkStroke.box(c.x - 0.045, c.y - 0.01, 0.09, 0.07, r = 0.2, fill = InkPalette.butter, width = 0.014, color = line, start = 0.66, span = 0.15))
                held.add(dot(pt(c.x, c.y + 0.018), 0.009, line, start = 0.72))
                held.add(curve(listOf(pt(c.x, c.y + 0.02), pt(c.x, c.y + 0.04)), width = 0.008, start = 0.73, span = 0.1))
                val g = frac(t / 2.8)
                if (g < 0.3) { val gx = c.x - w + g / 0.3 * w * 2
                    held.add(curve(listOf(pt(gx - 0.02, c.y + 0.05), pt(gx + 0.02, c.y - 0.07)), width = 0.012, color = InkColor.White, start = 0.8, span = 0.05, opacity = 0.7)) }
                r.armL = pt(c.x - w * 0.95, c.y - 0.01); r.armR = pt(c.x + w * 0.95, c.y - 0.01)
            }
            MascotPose.Coin -> {
                val (yaw, glance) = turn(0.35, t, 4.5); r.yaw = yaw; r.glance = glance
                val (lift, squash) = hop(t * 1.6, 0.006); r.lift = lift; r.squash += squash
                r.brow = 1.0; r.lid = 0.4; r.slant = 0.7; r.look = pt(0.6, -0.9)
                r.mouth = if (frac(t / 4) > 0.75) MascotMouth.Smirk else MascotMouth.Flat
                val c = pt(r.cx + 0.14, r.top - 0.03 + sin(t * 2.2) * 0.008); val phase = frac(t / 3.2)
                val sx = if (phase < 0.18) abs(cos(phase / 0.18 * PI * 2)) else 1.0; val radius = 0.075
                held.add(oval(c, radius * max(0.08, sx), radius, InkColor(0xf2c94c), width = 0.016, n = 14, misregister = 0.5))
                held.add(oval(c, radius * 0.72 * max(0.08, sx), radius * 0.72, null, width = 0.008, color = InkColor(0xb8860b), start = 0.58, n = 12))
                if (sx > 0.5) {
                    held.add(curve(listOf(pt(c.x - 0.012 * sx, c.y - 0.03), pt(c.x - 0.012 * sx, c.y + 0.03)), width = 0.01, color = InkColor(0x8a6508), start = 0.62, span = 0.1))
                    held.add(curve(listOf(pt(c.x + 0.02 * sx, c.y - 0.02), pt(c.x - 0.022 * sx, c.y - 0.012), pt(c.x - 0.02 * sx, c.y + 0.004), pt(c.x + 0.022 * sx, c.y + 0.008), pt(c.x + 0.02 * sx, c.y + 0.022), pt(c.x - 0.024 * sx, c.y + 0.024)), width = 0.01, color = InkColor(0x8a6508), start = 0.64, span = 0.1))
                }
                val sp = frac(t / 1.6); val so = pt(c.x + radius * 0.8, c.y - radius * 0.8); val ss = sin(sp * PI) * 0.025
                world.add(curve(listOf(pt(so.x - ss, so.y), pt(so.x + ss, so.y)), width = 0.008, color = InkColor(0xe0a800), start = 0.0, span = 0.0))
                world.add(curve(listOf(pt(so.x, so.y - ss), pt(so.x, so.y + ss)), width = 0.008, color = InkColor(0xe0a800), start = 0.0, span = 0.0))
                r.armR = pt(c.x - 0.02, c.y + radius * 0.9); r.armL = pt(r.shoulderL.x - 0.03, r.shoulderL.y + 0.04)
            }
            MascotPose.Cheer -> {
                r.yaw = 4 * (1 - (1 - min(1.0, t / 1.3)).pow(3))
                val (lift, squash) = hop(t * 4.4, 0.05); r.lift = lift; r.squash += squash
                r.closed = frac(t / 2.2) < 0.6; r.happy = true; r.brow = 0.15; r.mouth = if (r.closed) MascotMouth.O else MascotMouth.Smirk
                r.armL = pt(r.cx - 0.2, r.cy - 0.17 - sin(t * 8.8) * 0.02); r.armR = pt(r.cx + 0.2, r.cy - 0.17 + sin(t * 8.8) * 0.02)
                r.cowlick = 0.4 * sin(t * 8.8)
                val colors = listOf(InkPalette.tomato, InkPalette.butter, InkPalette.sky, InkPalette.lilac, belly, InkPalette.blush); val rng = InkRng(5150uL)
                repeat(18) { i ->
                    val x0 = 0.08 + rng.unit() * 0.84; val speed = 0.25 + rng.unit() * 0.2; val ph = frac(t * speed + rng.unit())
                    val p = pt(x0 + sin(t * 2 + i) * 0.03, 0.02 + ph * 0.8); val a = t * (2 + rng.unit() * 3) + i
                    world.add(InkStroke.box(p.x - 0.012, p.y - 0.006, 0.024, 0.012, r = 0.2, fill = colors[i % colors.size], width = 0.005, color = line.opacity(0.6), span = 0.0, opacity = 1 - ph * 0.6).rotated(a, p))
                }
            }
            else -> error("Not an extra mascot pose: $pose")
        }
    }
}
