package com.lumen.app

import com.lumen.ui.screens.*
import net.kramb.lumen.R

/** Stable resource themes are persisted by Android for the next cold launch. */
internal fun splashThemeFor(state: SettingsUiState, light: Boolean): Int {
    val variant = if (light) 0 else if (state.useAmoledBlack) 2 else 1
    return when (state.themePreset) {
        ThemePreset.LIGHT -> intArrayOf(R.style.Theme_Lumen_Starting_LIGHT_Light, R.style.Theme_Lumen_Starting_LIGHT_Dark, R.style.Theme_Lumen_Starting_LIGHT_Amoled)[variant]
        ThemePreset.DARK -> intArrayOf(R.style.Theme_Lumen_Starting_DARK_Light, R.style.Theme_Lumen_Starting_DARK_Dark, R.style.Theme_Lumen_Starting_DARK_Amoled)[variant]
        ThemePreset.DRACULA -> intArrayOf(R.style.Theme_Lumen_Starting_DRACULA_Light, R.style.Theme_Lumen_Starting_DRACULA_Dark, R.style.Theme_Lumen_Starting_DRACULA_Amoled)[variant]
        ThemePreset.CATPPUCCIN -> intArrayOf(R.style.Theme_Lumen_Starting_CATPPUCCIN_Light, R.style.Theme_Lumen_Starting_CATPPUCCIN_Dark, R.style.Theme_Lumen_Starting_CATPPUCCIN_Amoled)[variant]
        ThemePreset.NORD -> intArrayOf(R.style.Theme_Lumen_Starting_NORD_Light, R.style.Theme_Lumen_Starting_NORD_Dark, R.style.Theme_Lumen_Starting_NORD_Amoled)[variant]
        ThemePreset.GITHUB -> intArrayOf(R.style.Theme_Lumen_Starting_GITHUB_Light, R.style.Theme_Lumen_Starting_GITHUB_Dark, R.style.Theme_Lumen_Starting_GITHUB_Amoled)[variant]
        ThemePreset.GRUVBOX -> intArrayOf(R.style.Theme_Lumen_Starting_GRUVBOX_Light, R.style.Theme_Lumen_Starting_GRUVBOX_Dark, R.style.Theme_Lumen_Starting_GRUVBOX_Amoled)[variant]
        ThemePreset.TOKYO_NIGHT -> intArrayOf(R.style.Theme_Lumen_Starting_TOKYO_NIGHT_Light, R.style.Theme_Lumen_Starting_TOKYO_NIGHT_Dark, R.style.Theme_Lumen_Starting_TOKYO_NIGHT_Amoled)[variant]
        ThemePreset.MONOKAI -> intArrayOf(R.style.Theme_Lumen_Starting_MONOKAI_Light, R.style.Theme_Lumen_Starting_MONOKAI_Dark, R.style.Theme_Lumen_Starting_MONOKAI_Amoled)[variant]
        ThemePreset.MATERIAL -> intArrayOf(R.style.Theme_Lumen_Starting_MATERIAL_Light, R.style.Theme_Lumen_Starting_MATERIAL_Dark, R.style.Theme_Lumen_Starting_MATERIAL_Amoled)[variant]
        ThemePreset.SOLARIZED -> intArrayOf(R.style.Theme_Lumen_Starting_SOLARIZED_Light, R.style.Theme_Lumen_Starting_SOLARIZED_Dark, R.style.Theme_Lumen_Starting_SOLARIZED_Amoled)[variant]
        ThemePreset.ROSE_PINE -> intArrayOf(R.style.Theme_Lumen_Starting_ROSE_PINE_Light, R.style.Theme_Lumen_Starting_ROSE_PINE_Dark, R.style.Theme_Lumen_Starting_ROSE_PINE_Amoled)[variant]
    }
}
