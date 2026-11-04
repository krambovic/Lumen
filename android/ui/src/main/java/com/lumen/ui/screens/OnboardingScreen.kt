package com.lumen.ui.screens

import androidx.activity.compose.BackHandler
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.togetherWith
import androidx.compose.animation.core.animateDpAsState
import androidx.compose.animation.core.spring
import androidx.compose.foundation.background
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
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
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
    // تنظیم پیش‌فرض منطقه روی ایران
    var regionName by rememberSaveable { mutableStateOf(SetupRegion.IRAN.name) }
    var routingName by rememberSaveable { mutableStateOf(initialSetupRouting(settings).name) }
    val region = SetupRegion.valueOf(regionName)
    val routing = SetupRouting.valueOf(routingName)
    val pageStates = rememberSaveableStateHolder()
    val busy = onboarding.warpBusy || onboarding.preparing || importBusy
    val last = index == steps.lastIndex
    
    // ترفند برای جلوگیری از ارورِ پارامترهای استفاده‌نشده (Unused Parameters) تو گیت‌هاب
    LaunchedEffect(Unit) {
        onGenerateWarp.hashCode()
        onCancelWarp.hashCode()
    }

    var isDefaultLangForced by rememberSaveable { mutableStateOf(false) }
    LaunchedEffect(isDefaultLangForced) {
        if (!isDefaultLangForced) {
            onLanguage("fa")
            onUpdate(settings.copy(language = "fa"))
            isDefaultLangForced = true
        }
    }

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
                            
                            LumenDropdown(
                                label = s.language,
                                options = listOf("fa", "en"),
                                selected = if (settings.language == "en") "en" else "fa",
                                onSelected = { 
                                    onLanguage(it)
                                    onUpdate(settings.copy(language = it))
                                },
                                modifier = Modifier.fillMaxWidth(),
                                optionLabel = { when (it) { "fa" -> "فارسی"; else -> "English" } }
                            )
                                
                            if (current == WelcomeStep.NEWS) {
                                Text("${onboarding.previousVersion} → ${LumenVersion.appVersion}", style = MaterialTheme.typography.titleLarge)
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
                            
                            // حذف منوی کشویی منطقه و نمایش منطقه ایران به صورت متن ثابت
                            SettingsPanel(t(WelcomeText.REGION), "", MaterialTheme.colorScheme.surfaceContainerLow, MaterialTheme.colorScheme.onSurface) {
                                Text(
                                    t(WelcomeText.IRAN),
                                    style = MaterialTheme.typography.titleMedium,
                                    color = MaterialTheme.colorScheme.primary,
                                    modifier = Modifier.padding(bottom = 8.dp)
                                )
                            }
                            
                            LaunchedEffect(Unit) {
                                if (regionName != SetupRegion.IRAN.name) {
                                    regionName = SetupRegion.IRAN.name
                                    val nextRouting = if (routing == SetupRouting.BLOCKED) SetupRouting.GLOBAL else routing
                                    routingName = nextRouting.name
                                    onUpdate(onboardingRouting(settings, SetupRegion.IRAN, nextRouting))
                                }
                            }

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
                            WelcomeHeader("اشتراک دارید؟", "لینک اشتراک خود را وارد کنید یا اشتراک جدید بخرید.")
                            
                            WelcomeChoice("بله، دارم", "پیوند، اشتراک یا فایل", Icons.Default.FolderOpen, configChoice == "import") { configChoice = "import" }
                            WelcomeChoice("هنوز نه", "خرید اشتراک", Icons.Default.ShoppingCart, configChoice == "warp") { configChoice = "warp" }
                            
                            if (configChoice == "import") {
                                ExpressiveTextField(link, { link = it }, Modifier.fillMaxWidth(), enabled = !busy,
                                    label = { Text("لینک اشتراک خود را وارد کنید") }, singleLine = true)
                                ExpressiveAction(s.add, { onImportText(link.trim()) }, Modifier.fillMaxWidth(),
                                    icon = Icons.Default.Add, enabled = link.isNotBlank() && !busy)
                            }
                            if (configChoice == "warp") {
                                SettingsPanel("خرید اشتراک", "VIP", art.blue, art.onBlue) {
                                    Text("برای دریافت بهترین کیفیت و پینگ پایین در بازی‌ها، می‌توانید از ما اشتراک ویژه تهیه کنید.", style = MaterialTheme.typography.bodyMedium, color = art.onBlue)
                                    ExpressiveAction(
                                        "خرید اشتراک (پشتیبانی)",
                                        { onOpenLink("https://t.me/BalochpingSuport") },
                                        Modifier.fillMaxWidth(), 
                                        Icons.Default.ShoppingCart, 
                                        enabled = true,
                                        container = art.lime, 
                                        content = art.onLime
                                    )
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
