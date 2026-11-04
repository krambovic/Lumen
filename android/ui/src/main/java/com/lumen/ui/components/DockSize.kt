package com.lumen.ui.components

/** Stable preference values migrate the former continuous scale to three sizes. */
enum class DockSize(val percent: Int, val width: Int, val height: Int, val iconSize: Int) {
    SMALL(85, 288, 50, 18), DEFAULT(100, 320, 54, 20), LARGE(115, 368, 62, 23);

    companion object {
        fun fromPercent(value: Int): DockSize = entries.minBy { kotlin.math.abs(it.percent.toLong() - value) }
    }
}
