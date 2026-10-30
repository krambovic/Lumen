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
        val display = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) activity.display else {
            @Suppress("DEPRECATION")
            activity.windowManager.defaultDisplay
        } ?: return
        val current = display.mode
        val best = highestRefreshMode(current.physicalWidth, current.physicalHeight,
            display.supportedModes.map { DisplayModeOption(it.modeId, it.physicalWidth, it.physicalHeight, it.refreshRate) }) ?: return
        val attributes = activity.window.attributes
        // On Android 14+ a rate preference is seamless and leaves resolution selection to the OS.
        val modeId = if (Build.VERSION.SDK_INT >= 34) 0 else best.id
        if (attributes.preferredDisplayModeId != modeId || attributes.preferredRefreshRate != best.refreshRate) {
            attributes.preferredDisplayModeId = modeId
            attributes.preferredRefreshRate = best.refreshRate
            activity.window.attributes = attributes
        }
    }

    override fun onDisplayChanged(displayId: Int) { if (listening) request() }
    override fun onDisplayAdded(displayId: Int) { if (listening) request() }
    override fun onDisplayRemoved(displayId: Int) = Unit
}
