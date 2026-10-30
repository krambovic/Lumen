package com.lumen.app.navigation

import androidx.navigation.NavHostController

internal val MAIN_TAB_ROUTES = listOf("dashboard", "servers", "network", "settings")

internal fun isSettingsChild(route: String?): Boolean =
    route == "logs" || route == "routing" || route?.startsWith("routing/") == true

internal fun NavHostController.openMainTab(route: String) {
    val current = currentDestination?.route
    if (current == route) return
    if (route == "settings" && isSettingsChild(current) && popBackStack("settings", false)) return
    if (route == "dashboard") {
        popBackStack("dashboard", false)
        return
    }
    if (!popBackStack(route, false)) {
        navigate(route) {
            launchSingleTop = true
            restoreState = false
        }
    }
}

/** Never pop the root entry: doing so leaves a blank NavHost when the task is resumed. */
internal fun NavHostController.backOrMinimize(exit: () -> Unit) {
    val current = currentDestination?.route
    if (current == "dashboard" || previousBackStackEntry == null) {
        exit()
        return
    }
    if (!popBackStack()) {
        exit()
    }
}
