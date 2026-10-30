package com.lumen.app.onboarding

import android.app.Activity
import android.content.Intent
import android.net.Uri
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.spring
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.runtime.*
import androidx.compose.material3.MaterialTheme
import androidx.compose.ui.graphics.luminance
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import androidx.compose.ui.window.DialogWindowProvider
import androidx.core.view.WindowCompat
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.lumen.app.vm.MainViewModel
import com.lumen.ui.screens.*

@Composable
internal fun OnboardingRoute(
    viewModel: MainViewModel, onLanguage: (String) -> Unit,
    onImportClipboard: () -> Unit, onImportFile: () -> Unit, onImportQr: () -> Unit,
    onRevealHome: () -> Unit
) {
    val onboarding by viewModel.onboarding.collectAsStateWithLifecycle()
    var presented by remember { mutableStateOf(onboarding.visible) }
    var retained by remember { mutableStateOf(onboarding) }
    val dismissal = remember { Animatable(0f) }
    val revealHome by rememberUpdatedState(onRevealHome)
    if (onboarding.visible) SideEffect { retained = onboarding }
    LaunchedEffect(onboarding.visible) {
        if (onboarding.visible) {
            dismissal.snapTo(0f)
            presented = true
        } else if (presented) {
            // Put Home beneath the wizard before revealing it; keep this window alive until it exits.
            revealHome()
            withFrameNanos { }
            dismissal.animateTo(1f, spring(dampingRatio = 1f, stiffness = 220f))
            presented = false
        }
    }
    if (!presented && !onboarding.visible) return
    val displayed = if (onboarding.visible) onboarding else retained
    val settings by viewModel.settings.collectAsStateWithLifecycle()
    val nodes by viewModel.nodes.collectAsStateWithLifecycle()
    val importing by viewModel.importState.collectAsStateWithLifecycle()
    val context = LocalContext.current
    val minimize = { (context as? Activity)?.moveTaskToBack(true); Unit }
    Dialog(onDismissRequest = minimize,
        properties = DialogProperties(usePlatformDefaultWidth = false, decorFitsSystemWindows = false)) {
        val view = LocalView.current
        val light = MaterialTheme.colorScheme.background.luminance() > .5f
        SideEffect {
            (view.parent as? DialogWindowProvider)?.window?.let { window ->
                window.attributes = window.attributes.apply {
                    windowAnimations = 0
                    dimAmount = 0f
                }
                window.statusBarColor = android.graphics.Color.TRANSPARENT
                window.navigationBarColor = android.graphics.Color.TRANSPARENT
                WindowCompat.getInsetsController(window, view).apply {
                    isAppearanceLightStatusBars = light
                    isAppearanceLightNavigationBars = light
                }
            }
        }
        OnboardingScreen(displayed, settings, nodes.size, importing.phase == ImportPhaseUi.IMPORTING,
            onUpdate = viewModel::updateSettings, onLanguage = { language ->
                viewModel.updateSettings(settings.copy(language = language))
                onLanguage(language)
            },
            onImportText = viewModel::prepareImportText, onImportClipboard = onImportClipboard,
            onImportFile = onImportFile, onImportQr = onImportQr,
            onGenerateWarp = viewModel::generateWarpProfiles, onCancelWarp = viewModel::cancelWarpGeneration,
            onCancelPreparation = viewModel::cancelOnboardingPreparation,
            onFinish = { if (onboarding.visible) viewModel.completeOnboarding() }, onMinimize = minimize,
            modifier = Modifier.graphicsLayer { translationY = size.height * dismissal.value },
            onOpenLink = { url ->
                runCatching { context.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(url))) }
            })
    }
}
