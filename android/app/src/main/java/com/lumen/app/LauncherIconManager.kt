package com.lumen.app

import android.content.ComponentName
import android.content.Context
import android.content.pm.PackageManager
import com.lumen.ui.screens.LauncherIconOption

/**
 * Switches the launcher icon between the three <activity-alias> entries declared in
 * AndroidManifest.xml.
 *
 * Android has no API for "change my icon"; the only supported mechanism is one alias
 * per icon, each carrying the MAIN/LAUNCHER filter and its own android:icon, enabled
 * and disabled through [PackageManager.setComponentEnabledSetting].
 *
 * The one thing that must never happen is ZERO enabled launcher components: the app
 * then disappears from the launcher and cannot be started at all, with no in-app way
 * back. Everything here is built around that:
 *
 *  * [switchPlan] always emits the enable step first and never emits a disable for the
 *    alias being switched to, so every prefix of the plan leaves at least one alias
 *    enabled — which is what makes the switch safe to interrupt (process death, a
 *    failing binder call) at any point.
 *  * [applyOption] aborts before touching any disable if the enable failed.
 *  * If it ever finds all three disabled anyway, the plan starts with an enable, so the
 *    next launch repairs the state instead of preserving it.
 *
 * Component-enabled state survives app updates but a fresh install starts from the
 * android:enabled defaults in the manifest, so [applyOption] is also called on startup with
 * the stored preference — see MainViewModel — to reconcile the two.
 */
object LauncherIconManager {

    /**
     * Manifest alias names. Persisted by the system and baked into pinned shortcuts:
     * renaming one silently resets every user's choice, so they are frozen.
     */
    const val ALIAS_DEFAULT = "com.lumen.app.LauncherAliasDefault"
    const val ALIAS_LIGHT = "com.lumen.app.LauncherAliasLight"
    const val ALIAS_DARK = "com.lumen.app.LauncherAliasDark"

    /** Every launcher alias, in manifest order. */
    val ALIASES: List<String> = listOf(ALIAS_DEFAULT, ALIAS_LIGHT, ALIAS_DARK)

    /**
     * android:enabled as declared in the manifest. A component nobody has touched
     * reports [PackageManager.COMPONENT_ENABLED_STATE_DEFAULT], which means "whatever
     * the manifest says" — resolving it needs this table.
     */
    private val MANIFEST_ENABLED: Map<String, Boolean> = mapOf(
        ALIAS_DEFAULT to true,
        ALIAS_LIGHT to false,
        ALIAS_DARK to false
    )

    /** One step of a switch: enable or disable a single alias. */
    data class Step(val alias: String, val enable: Boolean)

    /** The alias that carries the artwork for [option]. */
    fun aliasFor(option: LauncherIconOption): String = when (option) {
        LauncherIconOption.SYSTEM -> ALIAS_DEFAULT
        LauncherIconOption.LIGHT -> ALIAS_LIGHT
        LauncherIconOption.DARK -> ALIAS_DARK
    }

    /** Inverse of [aliasFor]; null for anything that is not one of ours. */
    fun optionFor(alias: String): LauncherIconOption? = when (alias) {
        ALIAS_DEFAULT -> LauncherIconOption.SYSTEM
        ALIAS_LIGHT -> LauncherIconOption.LIGHT
        ALIAS_DARK -> LauncherIconOption.DARK
        else -> null
    }

    /**
     * Resolves one component-enabled setting to a plain boolean.
     * [PackageManager.COMPONENT_ENABLED_STATE_DEFAULT] falls back to the manifest.
     */
    fun resolveEnabled(alias: String, setting: Int): Boolean = when (setting) {
        PackageManager.COMPONENT_ENABLED_STATE_ENABLED -> true
        PackageManager.COMPONENT_ENABLED_STATE_DISABLED,
        PackageManager.COMPONENT_ENABLED_STATE_DISABLED_USER,
        PackageManager.COMPONENT_ENABLED_STATE_DISABLED_UNTIL_USED -> false
        // COMPONENT_ENABLED_STATE_DEFAULT and anything a future API adds.
        else -> MANIFEST_ENABLED[alias] == true
    }

    /**
     * Ordered steps that move the enabled set to exactly [wanted].
     *
     * Empty when there is nothing to do. The enable, if any, is always first and
     * [wanted] never appears as a disable, so no prefix of the result can leave the
     * app without a launcher entry. Aliases outside [ALIASES] are ignored.
     */
    fun switchPlan(wanted: String, enabled: Collection<String>): List<Step> {
        require(wanted in ALIASES) { "unknown launcher alias: $wanted" }
        val current = enabled.filterTo(mutableSetOf()) { it in ALIASES }
        val steps = mutableListOf<Step>()
        if (wanted !in current) steps += Step(wanted, enable = true)
        ALIASES.forEach { alias ->
            if (alias != wanted && alias in current) steps += Step(alias, enable = false)
        }
        return steps
    }

    /** The aliases the system currently considers enabled. */
    fun enabledAliases(context: Context): List<String> {
        val packageManager = context.packageManager
        val packageName = context.packageName
        return ALIASES.filter { alias ->
            val setting = runCatching {
                packageManager.getComponentEnabledSetting(ComponentName(packageName, alias))
            }.getOrDefault(PackageManager.COMPONENT_ENABLED_STATE_DEFAULT)
            resolveEnabled(alias, setting)
        }
    }

    /**
     * Makes [option] the live launcher icon, and reconciles the component state with
     * the stored preference when called on startup — the two operations are the same
     * thing, and both are no-ops when the state already matches.
     *
     * Returns false if the switch could not be completed; the app always keeps at
     * least one enabled launcher alias either way.
     */
    fun applyOption(context: Context, option: LauncherIconOption): Boolean {
        val packageManager = context.packageManager
        val packageName = context.packageName
        val steps = switchPlan(aliasFor(option), enabledAliases(context))
        if (steps.isEmpty()) return true
        var ok = true
        for (step in steps) {
            val applied = runCatching {
                packageManager.setComponentEnabledSetting(
                    ComponentName(packageName, step.alias),
                    if (step.enable) {
                        PackageManager.COMPONENT_ENABLED_STATE_ENABLED
                    } else {
                        PackageManager.COMPONENT_ENABLED_STATE_DISABLED
                    },
                    // Without DONT_KILL_APP the system kills the process mid-switch,
                    // which would strand us with the old alias still enabled at best.
                    PackageManager.DONT_KILL_APP
                )
            }.isSuccess
            // The enable is always step one. If it did not take, stop here: disabling
            // the alias that is currently showing would leave nothing behind it.
            if (!applied && step.enable) return false
            if (!applied) ok = false
        }
        return ok
    }
}
