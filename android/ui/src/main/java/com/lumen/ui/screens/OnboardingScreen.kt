package com.lumen.ui.screens

import androidx.activity.compose.BackHandler
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.togetherWith
import androidx.compose.animation.core.animateDpAsState
import androidx.compose.animation.core.spring
import androidx.compose.foundation.background
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.saveable.rememberSaveableStateHolder
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.lumen.ui.components.*
import com.lumen.ui.theme.expressivePalette

/** Full-screen, resumable setup. Every setting and action uses the real application callbacks. */
@Composable
fun OnboardingScreen(
    onboarding: OnboardingUiState, settings: SettingsUiState, serverCount: Int, importBusy: Boolean,
    onUpdate: (SettingsUiState) -> Unit, onLanguage: (String) -> Unit,
    onImportText: (String) -> Unit, onImportClipboard: () -> Unit, onImportFile: () -> Unit, onImportQr: () -> Unit,
    onGenerateWarp: () -> Unit, onCancelWarp: () -> Unit, onFinish: () -> Unit,
    onCancelPreparation: () -> Unit,
    onOpenLink: (String) -> Unit, onMinimize: () -> Unit, modifier: Modifier = Modifier,
    initialStep: WelcomeStep? = null
) {
    val s = LocalStrings.current
    val scanQr = rememberQrScanRequest(onImportQr)
    val art = expressivePalette()
    fun t(text: WelcomeText) = text.inLanguage(settings.language)
    val steps = remember(onboarding.setupRequired, onboarding.previousVersion) {
        onboardingSteps(onboarding.setupRequired, onboarding.previousVersion)
    }
    var stepIndex by rememberSaveable { mutableIntStateOf(steps.indexOf(initialStep).coerceAtLeast(0)) }
    val index = stepIndex.coerceIn(steps.indices)
    val step = steps[index]
    var configChoice by rememberSaveable { mutableStateOf(if (serverCount > 0) "import" else "") }
    var link by rememberSaveable { mutableStateOf("") }
    var regionName by rememberSaveable { mutableStateOf(SetupRegion.entries.firstOrNull { settings.geoResourceSource == it.source }?.name ?: SetupRegion.RUSSIA.name) }
    var routingName by rememberSaveable { mutableStateOf(initialSetupRouting(settings).name) }
    val region = SetupRegion.valueOf(regionName)
    val routing = SetupRouting.valueOf(routingName)
    val pageStates = rememberSaveableStateHolder()
    val busy = onboarding.warpBusy || onboarding.preparing || importBusy
    val last = index == steps.lastIndex
    fun back() {
        if (onboarding.preparing) onCancelPreparation()
        else if (index > 0) stepIndex = index - 1 else onMinimize()
    }
    BackHandler(onBack = ::back)

    Scaffold(modifier.fillMaxSize().background(MaterialTheme.colorScheme.background)
        .statusBarsPadding().navigationBarsPadding().imePadding(),
        containerColor = MaterialTheme.colorScheme.background,
        contentWindowInsets = WindowInsets(0, 0, 0, 0),
        topBar = {
            Column(Modifier.fillMaxWidth().padding(horizontal = 20.dp, vertical = 12.dp)) {
                Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                    Text("Lumen", style = MaterialTheme.typography.titleLarge, fontWeight = FontWeight.Bold,
                        modifier = Modifier.weight(1f))
                    if (!last) TextButton(onClick = onFinish, enabled = !busy) { Text(t(WelcomeText.SKIP)) }
                }
                Row(Modifier.padding(top = 16.dp), horizontalArrangement = Arrangement.spacedBy(5.dp)) {
                    steps.forEachIndexed { position, _ ->
                        val width by animateDpAsState(if (position == index) 32.dp else 10.dp,
                            spring(dampingRatio = .8f, stiffness = 500f), label = "welcome_step")
                        Box(Modifier.size(width, 6.dp).clip(RoundedCornerShape(10.dp))
                            .background(if (position <= index) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.surfaceContainerHighest))
                    }
                }
            }
        },
        bottomBar = {
            Column {
            if (onboarding.preparing) {
                LinearProgressIndicator(Modifier.fillMaxWidth())
                Row(Modifier.fillMaxWidth().padding(horizontal = 16.dp), verticalAlignment = Alignment.CenterVertically) {
                    Text(t(WelcomeText.PREPARING), Modifier.weight(1f), style = MaterialTheme.typography.bodySmall)
                    TextButton(onCancelPreparation) { Text(s.cancel) }
                }
            }
            if (onboarding.preparationError) Text(t(WelcomeText.PREPARATION_ERROR),
                Modifier.padding(horizontal = 16.dp), color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodySmall)
            Row(Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 12.dp),
                verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                if (index > 0) IconButton(onClick = ::back) { Icon(Icons.AutoMirrored.Filled.ArrowBack, s.back) }
                Spacer(Modifier.weight(1f))
                ExpressiveAction(t(if (last) WelcomeText.OPEN else if (index == 0) WelcomeText.START else WelcomeText.NEXT),
                    onClick = { if (last) onFinish() else stepIndex = index + 1 },
                    enabled = !busy, container = art.lime, content = art.onLime)
            }
            }
        }
    ) { padding ->
        AnimatedContent(step, modifier = Modifier.fillMaxSize().padding(padding), transitionSpec = {
            val forward = steps.indexOf(targetState) > steps.indexOf(initialState)
            expressivePageEnter(forward).togetherWith(expressivePageExit(forward))
        }, label = "welcome_pages") { current ->
            pageStates.SaveableStateProvider(current.name) {
                Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(20.dp),
                    verticalArrangement = Arrangement.spacedBy(14.dp)) {
                    when (current) {
                        WelcomeStep.WELCOME, WelcomeStep.NEWS -> {
                            if (current == WelcomeStep.WELCOME) WelcomeIntro(settings.language)
                            else WelcomeHero(t(WelcomeText.NEWS), t(WelcomeText.NEW_VERSION))
                            if (current == WelcomeStep.NEWS) Text(t(
                                if (LumenVersion.appVersion == "1.5.0") WelcomeText.NEWS_COPY else WelcomeText.UPDATED_COPY),
                                style = MaterialTheme.typography.bodyLarge, color = MaterialTheme.colorScheme.onSurfaceVariant)
                            LumenDropdown(s.language, listOf("ru", "en", "zh", "fa"), settings.language, onLanguage,
                                Modifier.fillMaxWidth(), optionLabel = { when (it) { "ru" -> "Русский"; "zh" -> "简体中文"; "fa" -> "فارسی"; else -> "English" } })
                            if (current == WelcomeStep.NEWS) {
                                Text("${onboarding.previousVersion} → ${LumenVersion.appVersion}", style = MaterialTheme.typography.titleLarge)
                                WelcomeSupport(settings.language, onOpenLink)
                            } else {
                                WelcomeFeature(t(WelcomeText.LOOK), t(WelcomeText.LOOK_COPY), Icons.Default.Palette)
                                WelcomeFeature(t(WelcomeText.ROUTES), t(WelcomeText.GLOBAL_COPY), Icons.Default.Route)
                                WelcomeFeature(t(WelcomeText.PROTOCOLS), "VLESS · Trojan · Hysteria2 · WireGuard · MASQUE", Icons.Default.Hub)
                            }
                        }
                        WelcomeStep.LOOK -> {
                            WelcomeHeader(t(WelcomeText.LOOK), t(WelcomeText.LOOK_COPY))
                            ThemePalettePicker(settings, onUpdate)
                            PreferenceSwitchRow(s.materialYou, s.materialYouDesc, Icons.Default.Wallpaper,
                                settings.useMaterialYou, android.os.Build.VERSION.SDK_INT >= 31) { onUpdate(settings.copy(useMaterialYou = it)) }
                            AccentColorSetting(settings.accentColor, settings.language) { onUpdate(settings.copy(accentColor = it)) }
                            DashboardLayoutPicker(settings, onUpdate)
                            ServerLayoutPicker(settings, onUpdate)
                            PreferenceSwitchRow(s.amoledBlack, s.amoledBlackDesc, Icons.Default.DarkMode,
                                settings.useAmoledBlack, true) { onUpdate(settings.copy(useAmoledBlack = it)) }
                        }
                        WelcomeStep.ROUTES -> {
                            WelcomeHeader(t(WelcomeText.ROUTES), t(WelcomeText.ROUTES_COPY))
                            LumenDropdown(t(WelcomeText.REGION), SetupRegion.entries.map { it.name }, regionName, { value ->
                                regionName = value
                                val nextRegion = SetupRegion.valueOf(value)
                                val nextRouting = if (nextRegion == SetupRegion.IRAN && routing == SetupRouting.BLOCKED) SetupRouting.GLOBAL else routing
                                routingName = nextRouting.name
                                onUpdate(onboardingRouting(settings, nextRegion, nextRouting))
                            }, Modifier.fillMaxWidth(), optionLabel = { t(when (SetupRegion.valueOf(it)) { SetupRegion.RUSSIA -> WelcomeText.RUSSIA; SetupRegion.CHINA -> WelcomeText.CHINA; SetupRegion.IRAN -> WelcomeText.IRAN }) })
                            SetupRouting.entries.filter { it != SetupRouting.BLOCKED || region != SetupRegion.IRAN }.forEach { choice ->
                                WelcomeChoice(t(when (choice) { SetupRouting.GLOBAL -> WelcomeText.GLOBAL; SetupRouting.BLOCKED -> WelcomeText.BLOCKED; SetupRouting.EXCEPT_LOCAL -> WelcomeText.EXCEPT_LOCAL }),
                                    t(when (choice) { SetupRouting.GLOBAL -> WelcomeText.GLOBAL_COPY; SetupRouting.BLOCKED -> WelcomeText.BLOCKED_COPY; SetupRouting.EXCEPT_LOCAL -> WelcomeText.EXCEPT_LOCAL_COPY }),
                                    Icons.Default.Route, choice == routing) {
                                    routingName = choice.name
                                    onUpdate(onboardingRouting(settings, region, choice))
                                }
                            }
                        }
                        WelcomeStep.CONFIGS -> {
                            WelcomeHeader(t(WelcomeText.CONFIGS), t(WelcomeText.CONFIGS_COPY))
                            WelcomeChoice(t(WelcomeText.HAVE), t(WelcomeText.HAVE_COPY), Icons.Default.FolderOpen, configChoice == "import") { configChoice = "import" }
                            WelcomeChoice(t(WelcomeText.NEED), t(WelcomeText.NEED_COPY), Icons.Default.Shield, configChoice == "warp") { configChoice = "warp" }
                            if (configChoice == "import") {
                                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                                    FilledTonalButton(onImportClipboard, enabled = !busy, modifier = Modifier.weight(1f)) {
                                        Icon(Icons.Default.ContentPaste, null, Modifier.size(18.dp))
                                        Text(t(WelcomeText.CLIPBOARD), Modifier.padding(start = 8.dp))
                                    }
                                    FilledTonalButton(onImportFile, enabled = !busy, modifier = Modifier.weight(1f)) {
                                        Icon(Icons.Default.FileOpen, null, Modifier.size(18.dp))
                                        Text(t(WelcomeText.FILE), Modifier.padding(start = 8.dp))
                                    }
                                }
                                OutlinedButton(scanQr, enabled = !busy, modifier = Modifier.fillMaxWidth()) {
                                    Icon(Icons.Default.QrCodeScanner, null, Modifier.size(18.dp))
                                    Text(t(WelcomeText.QR), Modifier.padding(start = 8.dp))
                                }
                                ExpressiveTextField(link, { link = it }, Modifier.fillMaxWidth(), enabled = !busy,
                                    label = { Text(t(WelcomeText.SUBSCRIPTION)) }, singleLine = true)
                                ExpressiveAction(s.add, { onImportText(link.trim()) }, Modifier.fillMaxWidth(),
                                    icon = Icons.Default.Add, enabled = link.isNotBlank() && !busy)
                            }
                            if (configChoice == "warp") {
                                SettingsPanel(t(WelcomeText.WARP), "WARP", art.blue, art.onBlue) {
                                    Text(t(WelcomeText.WARP_COPY), style = MaterialTheme.typography.bodyMedium, color = art.onBlue)
                                    if (onboarding.warpBusy) {
                                        LinearProgressIndicator(Modifier.fillMaxWidth())
                                        TextButton(onCancelWarp) { Text(s.cancel) }
                                    } else ExpressiveAction(t(when {
                                        onboarding.warpIncomplete || onboarding.warpError -> WelcomeText.RETRY_WARP
                                        onboarding.warpCount > 0 -> WelcomeText.WARP_READY
                                        else -> WelcomeText.GET_WARP
                                    }),
                                        onGenerateWarp, Modifier.fillMaxWidth(), Icons.Default.CloudDownload,
                                        enabled = onboarding.warpCount == 0 || onboarding.warpIncomplete || onboarding.warpError)
                                    if (onboarding.warpError) Text(t(WelcomeText.WARP_ERROR), color = MaterialTheme.colorScheme.error)
                                    if (onboarding.warpIncomplete) Text(t(WelcomeText.WARP_PARTIAL), color = art.onBlue)
                                }
                            }
                            if (importBusy) LinearProgressIndicator(Modifier.fillMaxWidth())
                            Text("${t(WelcomeText.SERVERS)}: $serverCount", style = MaterialTheme.typography.labelLarge)
                        }
                        WelcomeStep.SYSTEM -> {
                            WelcomeHeader(t(WelcomeText.SYSTEM), t(WelcomeText.SYSTEM_COPY))
                            PreferenceSwitchRow(s.autoCheckUpdates, t(WelcomeText.UPDATE_COPY), Icons.Default.SystemUpdate,
                                settings.autoCheckUpdates) { onUpdate(settings.copy(autoCheckUpdates = it)) }
                            PreferenceSwitchRow(s.autoConnect, t(WelcomeText.BOOT_COPY), Icons.Default.PowerSettingsNew,
                                settings.autoConnectOnBoot) { onUpdate(settings.copy(autoConnectOnBoot = it)) }
                            PreferenceSwitchRow(s.telemetryEnabled, s.telemetryEnabledDesc, Icons.Default.Analytics,
                                settings.telemetryEnabled) { onUpdate(settings.copy(telemetryEnabled = it)) }
                        }
                        WelcomeStep.DONE -> {
                            WelcomeHero(t(WelcomeText.DONE), "Lumen")
                            Text(t(WelcomeText.DONE_COPY), style = MaterialTheme.typography.bodyLarge)
                            WelcomeSummaryGrid(settings, region, routing, serverCount)
                            WelcomeSupport(settings.language, onOpenLink)
                        }
                    }
                    Spacer(Modifier.height(12.dp))
                }
            }
        }
    }
}

@Composable
private fun WelcomeHeader(title: String, description: String) {
    Text(title, style = MaterialTheme.typography.headlineLarge, fontWeight = FontWeight.Bold)
    Text(description, style = MaterialTheme.typography.bodyLarge, color = MaterialTheme.colorScheme.onSurfaceVariant)
}

@Composable
private fun WelcomeHero(title: String, eyebrow: String) {
    val art = expressivePalette()
    Row(Modifier.fillMaxWidth().padding(vertical = 12.dp), verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(16.dp)) {
        Box(Modifier.size(64.dp).clip(expressiveShape(Motif.Clover)).background(art.lime), contentAlignment = Alignment.Center) {
            Icon(Icons.Default.Check, null, tint = art.onLime, modifier = Modifier.size(32.dp))
        }
        Column(Modifier.weight(1f)) {
            Text(eyebrow, style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.primary)
            Text(title, style = MaterialTheme.typography.headlineLarge, fontWeight = FontWeight.Bold)
        }
    }
}

@Composable
private fun WelcomeFeature(title: String, body: String, icon: ImageVector) {
    Row(Modifier.fillMaxWidth().clip(RoundedCornerShape(22.dp)).background(MaterialTheme.colorScheme.surfaceContainerLow).padding(16.dp),
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(14.dp)) {
        Icon(icon, null, tint = MaterialTheme.colorScheme.primary)
        Column(Modifier.weight(1f)) {
            Text(title, style = MaterialTheme.typography.titleMedium)
            Text(body, style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
    }
}


@Composable
private fun WelcomeChoice(title: String, body: String, icon: ImageVector, chosen: Boolean, onClick: () -> Unit) {
    Row(Modifier.fillMaxWidth().semantics { selected = chosen }
        .clip(selectionShape(chosen, 14.dp, 28.dp))
        .background(if (chosen) MaterialTheme.colorScheme.primaryContainer else MaterialTheme.colorScheme.surfaceContainerLow)
        .clickable(role = Role.RadioButton, onClick = onClick).padding(16.dp), verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(14.dp)) {
        Icon(icon, null, tint = MaterialTheme.colorScheme.primary)
        Column(Modifier.weight(1f)) {
            Text(title, style = MaterialTheme.typography.titleMedium)
            if (body.isNotBlank()) Text(body, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
        RadioButton(chosen, onClick = null)
    }
}

@Composable
private fun WelcomeSupport(language: String, onOpenLink: (String) -> Unit) {
    val s = LocalStrings.current
    val art = expressivePalette()
    SettingsPanel(WelcomeText.SUPPORT.inLanguage(language), "♡", art.lavender, art.onLavender) {
        Text(WelcomeText.SUPPORT_COPY.inLanguage(language), color = art.onLavender)
        ExpressiveAction(WelcomeText.STAR.inLanguage(language), { onOpenLink("https://github.com/krambovic/Lumen") },
            Modifier.fillMaxWidth(), Icons.Default.Star)
        TextButton({ onOpenLink("https://www.donationalerts.com/r/studiobebraedition") }) {
            Icon(Icons.Default.Favorite, null, Modifier.size(18.dp))
            Text(s.supportProject, Modifier.padding(start = 8.dp))
        }
    }
}
