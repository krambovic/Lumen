package com.lumen.app

import android.content.Context
import android.content.Intent
import android.content.res.Configuration
import android.net.VpnService
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.result.contract.ActivityResultContracts
import androidx.activity.viewModels
import androidx.compose.runtime.getValue
import androidx.core.content.ContextCompat
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.lumen.app.navigation.LumenApp
import com.lumen.app.vm.MainViewModel
import androidx.lifecycle.lifecycleScope
import com.lumen.core.vpn.LumenVpnService
import com.lumen.ui.theme.LumenTheme
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch
import kotlinx.coroutines.withTimeoutOrNull
import java.util.Locale

import androidx.activity.SystemBarStyle
import androidx.activity.enableEdgeToEdge
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.runtime.DisposableEffect
import com.lumen.ui.screens.ThemeMode

class MainActivity : ComponentActivity() {
    private val viewModel: MainViewModel by viewModels()
    private val vpnPermissionLauncher = registerForActivityResult(
        ActivityResultContracts.StartActivityForResult()
    ) { result ->
        if (result.resultCode == RESULT_OK) startVpn()
        else viewModel.log("VPN permission denied by user")
    }

    override fun attachBaseContext(newBase: Context) {
        val prefs = newBase.getSharedPreferences(MainViewModel.PREFS_NAME, Context.MODE_PRIVATE)
        val saved = prefs.getString("language", "en").orEmpty()
        val language = if (saved in SUPPORTED_LANGUAGES) saved else "en"
        val locale = Locale.forLanguageTag(language)
        Locale.setDefault(locale)
        val config = Configuration(newBase.resources.configuration).apply { setLocale(locale) }
        super.attachBaseContext(newBase.createConfigurationContext(config))
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)

        // Request 120Hz High Refresh Rate on supporting displays
        if (android.os.Build.VERSION.SDK_INT >= android.os.Build.VERSION_CODES.M) {
            val currentDisplay = if (android.os.Build.VERSION.SDK_INT >= android.os.Build.VERSION_CODES.R) {
                display
            } else {
                @Suppress("DEPRECATION")
                windowManager.defaultDisplay
            }
            val modes = currentDisplay?.supportedModes ?: emptyArray()
            val maxMode = modes.maxByOrNull { it.refreshRate }
            if (maxMode != null && maxMode.refreshRate >= 90f) {
                val lp = window.attributes
                lp.preferredDisplayModeId = maxMode.modeId
                if (android.os.Build.VERSION.SDK_INT >= android.os.Build.VERSION_CODES.O) {
                    lp.preferredRefreshRate = maxMode.refreshRate
                }
                window.attributes = lp
            }
        }

        handleIntent(intent)
        setContent {
            val settings by viewModel.settings.collectAsStateWithLifecycle()
            val isDark = settings.themePreset != com.lumen.ui.screens.ThemePreset.LIGHT
            DisposableEffect(isDark) {
                enableEdgeToEdge(
                    statusBarStyle = if (isDark) {
                        SystemBarStyle.dark(android.graphics.Color.TRANSPARENT)
                    } else {
                        SystemBarStyle.light(android.graphics.Color.TRANSPARENT, android.graphics.Color.TRANSPARENT)
                    },
                    navigationBarStyle = if (isDark) {
                        SystemBarStyle.dark(android.graphics.Color.TRANSPARENT)
                    } else {
                        SystemBarStyle.light(android.graphics.Color.TRANSPARENT, android.graphics.Color.TRANSPARENT)
                    }
                )
                onDispose {}
            }
            LumenTheme(
                themePreset = settings.themePreset,
                useAmoledBlack = settings.useAmoledBlack,
                useMaterialYou = settings.useMaterialYou
            ) {
                LumenApp(viewModel, ::toggleConnection, ::restartVpnForNewServer) { recreate() }
            }
        }
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        handleIntent(intent)
    }

    private fun toggleConnection() {
        if (LumenVpnService.isRunning.value) {
            startService(viewModel.buildStopIntent(this))
        } else {
            if (viewModel.nodes.value.isEmpty()) {
                android.widget.Toast.makeText(this, "No servers available. Please import a server.", android.widget.Toast.LENGTH_SHORT).show()
                return
            }
            VpnService.prepare(this)?.let(vpnPermissionLauncher::launch) ?: startVpn()
        }
    }

    /**
     * Restarts the tunnel on the newly selected server. Called when the user
     * picks another node while the VPN is running.
     */
    private fun restartVpnForNewServer() {
        if (!LumenVpnService.isRunning.value) return
        lifecycleScope.launch {
            startService(viewModel.buildStopIntent(this@MainActivity))
            withTimeoutOrNull(6_000) { LumenVpnService.isRunning.first { running -> !running } }
            delay(250)
            VpnService.prepare(this@MainActivity)?.let(vpnPermissionLauncher::launch) ?: startVpn()
        }
    }

    private fun startVpn() {
        if (viewModel.nodes.value.isEmpty()) {
            android.widget.Toast.makeText(this, "No servers available. Please import a server.", android.widget.Toast.LENGTH_SHORT).show()
            return
        }
        val intent = viewModel.buildStartIntent(this) ?: run {
            android.widget.Toast.makeText(this, "No valid server configuration found.", android.widget.Toast.LENGTH_SHORT).show()
            return
        }
        viewModel.markConnecting()
        ContextCompat.startForegroundService(this, intent)
    }

    private fun handleIntent(intent: Intent?) {
        intent ?: return
        val importText = runCatching {
            when {
                intent.action == Intent.ACTION_SEND -> {
                    val extraText = intent.getStringExtra(Intent.EXTRA_TEXT)
                    if (!extraText.isNullOrBlank()) {
                        extraText
                    } else {
                        @Suppress("DEPRECATION")
                        val streamUri = intent.getParcelableExtra<android.net.Uri>(Intent.EXTRA_STREAM)
                        if (streamUri != null) {
                            contentResolver.openInputStream(streamUri)?.bufferedReader()?.use { it.readText() }
                        } else null
                    }
                }
                intent.data != null -> {
                    val uri = intent.data ?: return@runCatching null
                    val scheme = uri.scheme?.lowercase(Locale.ROOT)
                    if (scheme == "content" || scheme == "file") {
                        contentResolver.openInputStream(uri)?.bufferedReader()?.use { it.readText() }
                    } else if (scheme == "lumen") {
                        uri.getQueryParameter("url")?.takeIf { it.isNotBlank() }
                            ?: uri.toString().removePrefix("lumen://add/").removePrefix("lumen://import/")
                    } else uri.toString()
                }
                else -> null
            }
        }.getOrNull()

        if (!importText.isNullOrBlank()) {
            viewModel.prepareImportText(importText)
        }
    }

    companion object {
        private val SUPPORTED_LANGUAGES = setOf("en", "ru", "fa", "zh")
    }
}
