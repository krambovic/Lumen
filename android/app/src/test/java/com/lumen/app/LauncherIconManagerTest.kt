package com.lumen.app

import android.content.pm.PackageManager
import com.lumen.ui.screens.LauncherIconOption
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The launcher-icon switch has one failure mode that cannot be recovered from inside
 * the app: zero enabled launcher components means the icon is gone from the home
 * screen and MainActivity can no longer be started. These tests pin the ordering
 * rules that make that state unreachable, including halfway through a switch.
 *
 * Everything exercised here is pure: no PackageManager instance is touched, and the
 * PackageManager.COMPONENT_ENABLED_STATE_* values are compile-time constants.
 */
class LauncherIconManagerTest {

    private val all = LauncherIconManager.ALIASES

    @Test
    fun everyOptionMapsToItsOwnAlias() {
        val aliases = LauncherIconOption.values().map { LauncherIconManager.aliasFor(it) }
        assertEquals("each option needs a distinct alias", aliases.size, aliases.toSet().size)
        aliases.forEach { assertTrue("$it is not declared in ALIASES", it in all) }
        assertEquals(all.size, aliases.size)
    }

    @Test
    fun aliasRoundTripsBackToItsOption() {
        LauncherIconOption.values().forEach { option ->
            assertEquals(option, LauncherIconManager.optionFor(LauncherIconManager.aliasFor(option)))
        }
        assertNull(LauncherIconManager.optionFor("com.lumen.app.NotAnAlias"))
    }

    /** SYSTEM must stay on the manifest's enabled alias so a fresh install matches it. */
    @Test
    fun systemIsTheManifestDefaultAlias() {
        assertEquals(
            LauncherIconManager.ALIAS_DEFAULT,
            LauncherIconManager.aliasFor(LauncherIconOption.SYSTEM)
        )
    }

    @Test
    fun untouchedComponentsFallBackToTheManifestDefaults() {
        val default = PackageManager.COMPONENT_ENABLED_STATE_DEFAULT
        assertTrue(LauncherIconManager.resolveEnabled(LauncherIconManager.ALIAS_DEFAULT, default))
        assertFalse(LauncherIconManager.resolveEnabled(LauncherIconManager.ALIAS_LIGHT, default))
        assertFalse(LauncherIconManager.resolveEnabled(LauncherIconManager.ALIAS_DARK, default))
    }

    @Test
    fun explicitStatesWinOverTheManifestDefaults() {
        assertTrue(
            LauncherIconManager.resolveEnabled(
                LauncherIconManager.ALIAS_DARK,
                PackageManager.COMPONENT_ENABLED_STATE_ENABLED
            )
        )
        listOf(
            PackageManager.COMPONENT_ENABLED_STATE_DISABLED,
            PackageManager.COMPONENT_ENABLED_STATE_DISABLED_USER,
            PackageManager.COMPONENT_ENABLED_STATE_DISABLED_UNTIL_USED
        ).forEach { state ->
            assertFalse(
                "state $state must read as disabled",
                LauncherIconManager.resolveEnabled(LauncherIconManager.ALIAS_DEFAULT, state)
            )
        }
    }

    @Test
    fun switchingIsANoOpWhenTheStateAlreadyMatches() {
        assertTrue(
            LauncherIconManager.switchPlan(
                LauncherIconManager.ALIAS_DEFAULT,
                listOf(LauncherIconManager.ALIAS_DEFAULT)
            ).isEmpty()
        )
    }

    @Test
    fun theNewAliasIsEnabledBeforeTheOldOneIsDisabled() {
        val plan = LauncherIconManager.switchPlan(
            LauncherIconManager.ALIAS_DARK,
            listOf(LauncherIconManager.ALIAS_DEFAULT)
        )
        assertEquals(2, plan.size)
        assertEquals(LauncherIconManager.Step(LauncherIconManager.ALIAS_DARK, true), plan[0])
        assertEquals(LauncherIconManager.Step(LauncherIconManager.ALIAS_DEFAULT, false), plan[1])
    }

    @Test
    fun theWantedAliasIsNeverDisabled() {
        all.forEach { wanted ->
            LauncherIconManager.switchPlan(wanted, all).forEach { step ->
                assertFalse("$wanted must not be disabled", step.alias == wanted && !step.enable)
            }
        }
    }

    /**
     * The invariant that matters: whatever the starting state, and wherever the
     * process dies inside the plan, at least one launcher alias stays enabled.
     */
    @Test
    fun noPrefixOfAnyPlanLeavesTheAppWithoutALauncherEntry() {
        val startStates = powerSet(all)
        startStates.forEach { start ->
            all.forEach { wanted ->
                val plan = LauncherIconManager.switchPlan(wanted, start)
                val live = start.toMutableSet()
                plan.forEachIndexed { index, step ->
                    if (step.enable) live += step.alias else live -= step.alias
                    assertTrue(
                        "start=$start wanted=$wanted died after step $index -> no launcher",
                        live.isNotEmpty()
                    )
                }
                assertEquals(
                    "start=$start wanted=$wanted must end on exactly one alias",
                    setOf(wanted),
                    live
                )
            }
        }
    }

    /** A botched earlier switch that left nothing enabled must repair itself. */
    @Test
    fun anEmptyStateIsRepairedByEnablingFirst() {
        val plan = LauncherIconManager.switchPlan(LauncherIconManager.ALIAS_LIGHT, emptyList())
        assertEquals(listOf(LauncherIconManager.Step(LauncherIconManager.ALIAS_LIGHT, true)), plan)
    }

    @Test
    fun aliasesFromAnotherAppAreIgnored() {
        val plan = LauncherIconManager.switchPlan(
            LauncherIconManager.ALIAS_DEFAULT,
            listOf(LauncherIconManager.ALIAS_DEFAULT, "com.other.app.Alias")
        )
        assertTrue(plan.isEmpty())
    }

    private fun powerSet(items: List<String>): List<Set<String>> =
        (0 until (1 shl items.size)).map { mask ->
            items.filterIndexed { index, _ -> mask and (1 shl index) != 0 }.toSet()
        }
}
