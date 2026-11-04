package com.lumen.app.display

import android.app.Activity
import android.hardware.display.DisplayManager
import android.os.Build
import android.os.Handler
import android.os.Looper

internal data class DisplayModeOption(val id: Int, val width: Int, val height: Int, val refreshRate: Float)

internal fun highestRefreshMode(width: Int, height: Int, modes: List<DisplayModeOption>): DisplayModeOption? =
    modes.asSequence().filter { it.width == width && it.height == height && it.refreshRate.isFinite() && it.refreshRate > 0f }
        .maxByOrNull { it.refreshRate }

/** A window preference, never a global display setting or a synthetic frame loop. */
internal class HighRefreshRateController(private val activity: Activity) : DisplayManager.DisplayListener {
    private val manager by lazy { activity.getSystemService(DisplayManager::class.java) }
    private var listening = false

    fun start() {
        if (!listening) {
            manager?.registerDisplayListener(this, Handler(Looper.getMainLooper()))
            listening = true
        }
        request()
    }

    fun stop() {
        if (listening) manager?.unregisterDisplayListener(this)
        listening = false
    }

    fun request() {
        // Let Android select the rate from touch activity, animations and battery saver.
        // Pinning the fastest mode prevents the display from idling on a static screen.
        val attributes = activity.window.attributes
        if (attributes.preferredDisplayModeId != 0 || attributes.preferredRefreshRate != 0f) {
            attributes.preferredDisplayModeId = 0
            attributes.preferredRefreshRate = 0f
            activity.window.attributes = attributes
        }
    }

    override fun onDisplayChanged(displayId: Int) { if (listening) request() }
    override fun onDisplayAdded(displayId: Int) { if (listening) request() }
    override fun onDisplayRemoved(displayId: Int) = Unit
}
