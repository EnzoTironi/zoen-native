package xyz.tironi.zoen.ui

internal fun drawPixelDonkey(asleep: Boolean, block: (Int, Int, Int, Int, Long) -> Unit) {
    val ink = 0xFF4B3B36L; val fur = 0xFFA0958CL; val light = 0xFFD3C5B4L
    block(6, 22, 21, 2, 0x1A443830L)
    block(8, 10, 15, 9, ink); block(9, 11, 13, 7, fur)
    block(20, 5, 9, 10, ink); block(21, 6, 7, 8, fur)
    block(21, 0, 2, 7, ink); block(22, 1, 1, 5, light)
    block(26, 0, 2, 7, ink); block(26, 1, 1, 5, light)
    block(25, 11, 6, 4, light); block(29, 12, 1, 1, ink)
    block(8, 18, 3, 5, ink); block(11, 18, 2, 4, fur); block(18, 18, 3, 5, ink); block(21, 18, 2, 4, fur)
    block(5, 11, 3, 2, ink); block(4, 12, 2, 4, ink)
    block(23, 8, if (asleep) 3 else 2, if (asleep) 1 else 2, ink)
    if (!asleep) block(23, 8, 1, 1, 0xFFFFFFFFL)
    block(20, 4, 6, 2, ink)
}
