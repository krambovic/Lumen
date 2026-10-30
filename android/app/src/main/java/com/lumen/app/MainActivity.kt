package com.lumen.app

import android.Manifest
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.content.res.Configuration
import android.net.VpnService
import android.os.Build
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.layout.fillMaxSize
import androidx.activity.result.contract.ActivityResultContracts
import androidx.activity.viewModels
import androidx.compose.runtime.getValue
import androidx.core.splashscreen.SplashScreen.Companion.installSplashScreen
import androidx.core.content.ContextCompat
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.lumen.app.navigation.LumenApp
import com.lumen.app.vm.MainViewModel
import androidx.lifecycle.lifecycleScope
import com.lumen.core.vpn.LumenVpnService
import com.lumen.ui.theme.LumenTheme
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch
import kotlinx.coroutines.withTimeoutOrNull
import java.util.Locale

import androidx.activity.SystemBarStyle
import androidx.activity.enableEdgeToEdge
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.SideEffect
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.graphics.asImageBitmap
import com.lumen.ui.screens.ThemeMode
import com.lumen.ui.screens.ThemePreset
import com.lumen.ui.screens.SettingsUiState

class MainActivity : ComponentActivity() {
    private var launchBackground = 0
    private var launchInk = 0
    private val refreshRateController = com.lumen.app.display.HighRefreshRateController(this)
    private val viewModel: MainViewModel by viewModels()
    private val firstRunVpnPermissionLauncher = registerForActivityResult(
        ActivityResultContracts.StartActivityForResult()
    ) { result ->
        if (result.resultCode != RESULT_OK) {
            viewModel.log("VPN permission was not granted during first-run setup")
        }
    }
    private val runtimePermissionLauncher = registerForActivityResult(
        ActivityResultContracts.RequestMultiplePermissions()
    ) {
        requestFirstRunVpnConsent()
    }
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
        val startupPrefs = getSharedPreferences(MainViewModel.PREFS_NAME, Context.MODE_PRIVATE)
        val startupSettings = SettingsUiState(
            themePreset = runCatching { ThemePreset.valueOf(startupPrefs.getString("theme_preset", "DARK")!!) }.getOrDefault(ThemePreset.DARK),
            themeMode = runCatching { ThemeMode.valueOf(startupPrefs.getString("theme_mode", "SYSTEM")!!) }.getOrDefault(ThemeMode.SYSTEM),
            useAmoledBlack = startupPrefs.getBoolean("use_amoled_black", false))
        val startupLight = when (startupSettings.themeMode) {
            ThemeMode.LIGHT -> true
            ThemeMode.DARK -> false
            ThemeMode.SYSTEM -> resources.configuration.uiMode and Configuration.UI_MODE_NIGHT_MASK != Configuration.UI_MODE_NIGHT_YES
        }
        setTheme(splashThemeFor(startupSettings, startupLight))
        val splash = installSplashScreen()
        applySystemBarStyle(dark = !startupLight)
        super.onCreate(savedInstanceState)
        splash.setOnExitAnimationListener { provider ->
            // core-splashscreen applies Theme.Lumen's windowLightStatusBar (false: Material dark)
            // right before this listener and again in remove(); restore the palette's bar style
            // both times, or the icons flash white over the light theme.
            systemBarsDark?.let(::applySystemBarStyle)
            if (launchBackground != 0) provider.view.setBackgroundColor(launchBackground)
            (provider.iconView as? android.widget.ImageView)?.drawable?.setTint(launchInk)
            val elapsed = android.os.SystemClock.uptimeMillis() - provider.iconAnimationStartMillis
            val remaining = if (android.animation.ValueAnimator.areAnimatorsEnabled())
                (provider.iconAnimationDurationMillis - elapsed).coerceIn(0L, 620L) else 0L
            provider.view.animate().alpha(0f).setStartDelay(remaining).setDuration(180L)
                .withEndAction {
                    provider.remove()
                    // On API 31+ core-splashscreen's remove() re-applies the activity theme's
                    // windowLightStatusBar (false: Theme.Lumen is Material dark), which turned the
                    // status-bar icons white over the light theme until the theme was toggled.
                    systemBarsDark?.let(::applySystemBarStyle)
                }.start()
        }

        handleIntent(intent)
        setContent {
            val settings by viewModel.settings.collectAsStateWithLifecycle()
            val isDark = when (settings.themeMode) {
                com.lumen.ui.screens.ThemeMode.SYSTEM -> androidx.compose.foundation.isSystemInDarkTheme()
                com.lumen.ui.screens.ThemeMode.LIGHT -> false
                com.lumen.ui.screens.ThemeMode.DARK -> true
            }
            DisposableEffect(isDark) {
                applySystemBarStyle(isDark)
                onDispose {}
            }
            LumenTheme(
                themePreset = settings.themePreset,
                useAmoledBlack = settings.useAmoledBlack,
                useMaterialYou = settings.useMaterialYou,
                themeMode = settings.themeMode,
                accentColor = settings.accentColor,
                language = settings.language
            ) {
                val colors = androidx.compose.material3.MaterialTheme.colorScheme
                SideEffect {
                    launchBackground = colors.background.toArgb()
                    launchInk = colors.primary.toArgb()
                    window.setBackgroundDrawable(android.graphics.drawable.ColorDrawable(colors.background.toArgb()))
                }
                DisposableEffect(settings.themePreset, settings.themeMode, settings.useAmoledBlack, isDark) {
                    // Android persists this resource theme, so its starting window is already
                    // in the selected app palette before our process is created next time.
                    if (Build.VERSION.SDK_INT >= 31) splashScreen.setSplashScreenTheme(splashThemeFor(settings, !isDark))
                    onDispose { }
                }
                val qrEncoder = androidx.compose.runtime.remember { { text: String -> encodeQrBitmap(text, 512).asImageBitmap() } }
                androidx.compose.runtime.CompositionLocalProvider(com.lumen.ui.components.LocalQrEncoder provides qrEncoder) {
                    LumenApp(viewModel, ::toggleConnection, ::restartVpnForNewServer) { recreate() }
                }
            }
        }
        lifecycleScope.launch {
            viewModel.onboarding.collectLatest { state ->
                if (!state.visible) {
                    delay(350)
                    requestFirstRunPermissions()
                }
            }
        }
    }

    /** Last bar style requested for the app palette, re-applied after the splash is removed. */
    private var systemBarsDark: Boolean? = null

    private fun applySystemBarStyle(dark: Boolean) {
        systemBarsDark = dark
        val style = if (dark) SystemBarStyle.dark(android.graphics.Color.TRANSPARENT)
        else SystemBarStyle.light(android.graphics.Color.TRANSPARENT, android.graphics.Color.TRANSPARENT)
        enableEdgeToEdge(statusBarStyle = style, navigationBarStyle = style)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            window.isStatusBarContrastEnforced = false
            window.isNavigationBarContrastEnforced = false
        }
    }

    override fun onStart() {
        super.onStart()
        viewModel.checkForAndroidUpdateIfDue()
    }

    override fun onResume() {
        super.onResume()
        refreshRateController.start()
    }

    override fun onPause() {
        refreshRateController.stop()
        super.onPause()
    }

    override fun onWindowFocusChanged(hasFocus: Boolean) {
        super.onWindowFocusChanged(hasFocus)
        if (hasFocus) refreshRateController.request()
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        // Without this getIntent() keeps returning the launch intent, so a language
        // change (recreate) replays the original deep link in onCreate.
        setIntent(intent)
        handleIntent(intent)
    }

    private fun toggleConnection() {
        if (LumenVpnService.isRunning.value || LumenVpnService.isStarting.value) {
            startService(viewModel.buildStopIntent(this))
        } else {
            if (viewModel.nodes.value.isEmpty()) {
                com.lumen.ui.screens.showLumenNotice("No servers available. Please import a server.")
                return
            }
            if (viewModel.settings.value.proxyOnly) {
                startVpn()
            } else {
                VpnService.prepare(this)?.let(vpnPermissionLauncher::launch) ?: startVpn()
            }
        }
    }

    private fun requestFirstRunPermissions() {
        val prefs = getSharedPreferences(MainViewModel.PREFS_NAME, Context.MODE_PRIVATE)
        if (prefs.getBoolean("first_run_permissions_requested", false)) return
        prefs.edit().putBoolean("first_run_permissions_requested", true).apply()

        // CAMERA is deliberately absent: a VPN asking for the camera on first launch
        // looks like spyware. The QR scanner asks for it when it is opened instead
        // (openQrScanner in NavGraph).
        val required = buildList {
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU &&
                ContextCompat.checkSelfPermission(
                    this@MainActivity,
                    Manifest.permission.POST_NOTIFICATIONS
                ) != PackageManager.PERMISSION_GRANTED
            ) {
                add(Manifest.permission.POST_NOTIFICATIONS)
            }
        }
        if (required.isEmpty()) {
            requestFirstRunVpnConsent()
        } else {
            runtimePermissionLauncher.launch(required.toTypedArray())
        }
    }

    private fun requestFirstRunVpnConsent() {
        if (viewModel.settings.value.proxyOnly) return
        VpnService.prepare(this)?.let(firstRunVpnPermissionLauncher::launch)
    }

    /**
     * Restarts the tunnel on the newly selected server. A selection made while
     * startup is still in progress must cancel that attempt as well; otherwise the
     * UI names the new server while the old generated config finishes connecting.
     */
    private fun restartVpnForNewServer() {
        if (!LumenVpnService.isRunning.value && !LumenVpnService.isStarting.value) return
        lifecycleScope.launch {
            startService(viewModel.buildStopIntent(this@MainActivity))
            val stopped = withTimeoutOrNull(10_000) {
                combine(
                    LumenVpnService.isRunning,
                    LumenVpnService.isStarting
                ) { running, starting -> !running && !starting }
                    .first { it }
            } == true
            if (!stopped) {
                viewModel.reportConnectError("Could not stop the previous VPN session")
                return@launch
            }
            delay(250)
            if (viewModel.settings.value.proxyOnly) {
                startVpn()
            } else {
                VpnService.prepare(this@MainActivity)?.let(vpnPermissionLauncher::launch) ?: startVpn()
            }
        }
    }

    private fun startVpn() {
        if (viewModel.nodes.value.isEmpty()) {
            com.lumen.ui.screens.showLumenNotice("No servers available. Please import a server.")
            return
        }
        lifecycleScope.launch {
            val intent = viewModel.buildStartIntent(this@MainActivity) ?: run {
                com.lumen.ui.screens.showLumenNotice("No valid server configuration found.")
                return@launch
            }
            // A very large AUTO pool can parcel past the Binder limit; the config is
            // already persisted in prefs, so fail with a message instead of crashing.
            runCatching { ContextCompat.startForegroundService(this@MainActivity, intent) }
                .onSuccess { viewModel.markConnecting() }
                .onFailure { error ->
                    viewModel.log("Failed to start the VPN service: ${error.message}")
                    com.lumen.ui.screens.showLumenNotice("Could not start the VPN service.")
                }
        }
    }

    private fun handleIntent(intent: Intent?) {
        intent ?: return
        // Streams are read by the view model on IO with a size bound: shared content
        // is untrusted and used to be materialised on the UI thread.
        var streamUri: android.net.Uri? = null
        val importText = runCatching {
            when {
                intent.action == Intent.ACTION_SEND -> {
                    val extraText = intent.getStringExtra(Intent.EXTRA_TEXT)
                    if (!extraText.isNullOrBlank()) {
                        extraText
                    } else {
                        @Suppress("DEPRECATION")
                        val stream = intent.getParcelableExtra<android.net.Uri>(Intent.EXTRA_STREAM)
                        streamUri = stream
                        null
                    }
                }
                intent.data != null -> {
                    val uri = intent.data ?: return@runCatching null
                    val scheme = uri.scheme?.lowercase(Locale.ROOT)
                    if (scheme == "content" || scheme == "file") {
                        streamUri = uri
                        null
                    } else if (scheme == "lumen") {
                        uri.getQueryParameter("url")?.takeIf { it.isNotBlank() }
                            ?: uri.toString().removePrefix("lumen://add/").removePrefix("lumen://import/")
                    } else uri.toString()
                }
                else -> null
            }
        }.getOrNull()

        // Mark the intent consumed so a recreate() does not re-import it.
        intent.data = null
        intent.removeExtra(Intent.EXTRA_TEXT)
        intent.removeExtra(Intent.EXTRA_STREAM)

        val uri = streamUri
        if (uri != null) {
            // A config file opened from outside the app belongs to the manual bucket;
            // switch there up front so the user does not land on the dashboard.
            viewModel.focusDefaultServerGroup()
            viewModel.prepareImportFromUri(this, uri, asFile = true)
        } else if (!importText.isNullOrBlank()) {
            viewModel.prepareImportText(importText)
        }
    }

    companion object {
        private val SUPPORTED_LANGUAGES = setOf("en", "ru", "fa", "zh")
    }
}
