package com.lumen.app.vm

import com.lumen.ui.screens.AppEntryUiModel
import com.lumen.ui.screens.SplitModeUi

/**
 * Auto-selection is intentionally conservative: an unknown application is never
 * routed differently just because it happens to be installed.
 */
internal object SplitAppAutoSelector {
    private val directPackages = setOf(
        "com.avito.android",
        "com.idamob.tinkoff.android",
        "com.wildberries.ru",
        "ru.alfabank.mobile.android",
        "ru.dublgis.dgismobile",
        "ru.gazprombank.android.mobilebank.app",
        "ru.goskey",
        "ru.mtsbank.android",
        "ru.nspk.mirpay",
        "ru.openbank",
        "ru.ozon.app.android",
        "ru.raiffeisennews",
        "ru.rosbank.android",
        "ru.rostel",
        "ru.russianpost.client",
        "ru.sberbankmobile",
        "ru.vtb24.mobilebanking.android",
        "ru.yandex.food",
        "ru.yandex.market",
        "ru.yandex.taxi"
    )

    fun select(
        mode: SplitModeUi,
        apps: List<AppEntryUiModel>,
        proxyPackages: Set<String>
    ): Set<String> {
        val wanted = when (mode) {
            SplitModeUi.ALLOW_LIST -> proxyPackages
            SplitModeUi.DISALLOW_LIST -> directPackages
            SplitModeUi.DISABLED -> emptySet()
        }
        return apps.asSequence()
            .map { it.packageName }
            // Framework and WebView traffic belongs to the selected host app; adding
            // these pseudo-apps to an allow-list can produce surprising captures.
            .filterNot { it == "android" || it == "com.google.android.webview" }
            .filter { it in wanted }
            .toSet()
    }
}
