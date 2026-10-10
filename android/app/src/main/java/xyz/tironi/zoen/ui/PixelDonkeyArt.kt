package xyz.tironi.zoen.ui

internal fun drawPixelDonkey(asleep: Boolean, blink: Boolean = false, block: (Int, Int, Int, Int, Long) -> Unit) {
    val rows = if (asleep) OriginalPetArt.sleeping else if (blink) OriginalPetArt.blinking else OriginalPetArt.front
    rows.forEachIndexed { y, row ->
        row.forEachIndexed { x, character -> OriginalPetArt.palette[character]?.let { block(x, y, 1, 1, it) } }
    }
}
