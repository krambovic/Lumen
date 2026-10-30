package com.lumen.ui.components

import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.graphics.ImageBitmap

/** The host supplies its existing QR codec; UI invokes it off the main thread. */
val LocalQrEncoder = staticCompositionLocalOf<(String) -> ImageBitmap?> { { null } }
