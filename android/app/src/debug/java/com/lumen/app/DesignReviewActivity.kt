package com.lumen.app

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.SystemBarStyle
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import com.lumen.ui.components.ConnectionState
import com.lumen.ui.screens.DashboardStyle
import com.lumen.ui.screens.DesignReviewScreen

/** Isolated native previews, reachable only in debug builds through an explicit intent. */
class DesignReviewActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        com.lumen.ui.screens.LumenVersion.appVersion = net.kramb.lumen.BuildConfig.VERSION_NAME
        val dark = intent.getStringExtra("theme") == "dark"
        val bars = if (dark) SystemBarStyle.dark(android.graphics.Color.TRANSPARENT)
            else SystemBarStyle.light(android.graphics.Color.TRANSPARENT, android.graphics.Color.TRANSPARENT)
        enableEdgeToEdge(statusBarStyle = bars, navigationBarStyle = bars)
        val style = DashboardStyle.entries.firstOrNull { it.name.equals(intent.getStringExtra("style"), true) }
            ?: DashboardStyle.DEFAULT
        val connection = ConnectionState.entries.firstOrNull { it.name.equals(intent.getStringExtra("state"), true) }
            ?: ConnectionState.Disconnected
        setContent {
            DesignReviewScreen(intent.getStringExtra("page") ?: "home", dark, style, connection,
                intent.getStringExtra("language") ?: "ru")
        }
    }
}
