package com.lumen.app.vm

import com.lumen.ui.screens.AppEntryUiModel
import com.lumen.ui.screens.SplitModeUi
import org.junit.Assert.assertEquals
import org.junit.Test

class SplitAppAutoSelectorTest {
    private val apps = listOf(
        AppEntryUiModel(packageName = "com.discord", label = "Discord"),
        AppEntryUiModel(packageName = "com.google.android.youtube", label = "YouTube", isSystem = true),
        AppEntryUiModel(packageName = "android", label = "Android", isSystem = true),
        AppEntryUiModel(packageName = "ru.sberbankmobile", label = "Sber"),
        AppEntryUiModel(packageName = "ru.rostel", label = "Gosuslugi"),
        AppEntryUiModel(packageName = "dev.unknown.app", label = "Unknown")
    )

    @Test
    fun allowListSelectsOnlyKnownProxyApps() {
        val selected = SplitAppAutoSelector.select(
            mode = SplitModeUi.ALLOW_LIST,
            apps = apps,
            proxyPackages = setOf("android", "com.discord", "com.google.android.youtube")
        )

        assertEquals(setOf("com.discord", "com.google.android.youtube"), selected)
    }

    @Test
    fun disallowListSelectsOnlyKnownDirectApps() {
        val selected = SplitAppAutoSelector.select(
            mode = SplitModeUi.DISALLOW_LIST,
            apps = apps,
            proxyPackages = setOf("com.discord", "com.google.android.youtube")
        )

        assertEquals(setOf("ru.sberbankmobile", "ru.rostel"), selected)
    }

    @Test
    fun disabledModeSelectsNothing() {
        assertEquals(
            emptySet<String>(),
            SplitAppAutoSelector.select(SplitModeUi.DISABLED, apps, setOf("com.discord"))
        )
    }
}
