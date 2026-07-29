package com.lumen.app

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Assume.assumeTrue
import org.junit.Test
import org.w3c.dom.Element
import java.io.File
import javax.xml.parsers.DocumentBuilderFactory

/**
 * Reads the real AndroidManifest.xml and guards the parts the launcher-icon picker
 * depends on. Manifest mistakes here are silent at build time and expensive at
 * runtime: a stray MAIN/LAUNCHER filter left on MainActivity gives two home-screen
 * entries, and a deep-link filter accidentally moved onto an alias breaks importing
 * by `lumen://` link and "open with Lumen".
 *
 * The manifest is located relative to the module directory; the assertions are
 * skipped rather than failed if the test runner is rooted somewhere unexpected.
 */
class ManifestLauncherAliasTest {

    private val manifest: File? = sequenceOf(
        File("src/main/AndroidManifest.xml"),
        File("app/src/main/AndroidManifest.xml"),
        File("android/app/src/main/AndroidManifest.xml")
    ).firstOrNull { it.isFile }

    private fun elements(tag: String): List<Element> {
        val file = requireNotNull(manifest)
        val document = DocumentBuilderFactory.newInstance().newDocumentBuilder().parse(file)
        val nodes = document.getElementsByTagName(tag)
        return (0 until nodes.length).map { nodes.item(it) as Element }
    }

    /** Every android:name under this element's <action> tags, across all its filters. */
    private fun Element.actionNames(): List<String> = named("action")

    /** Every android:name under this element's <category> tags. */
    private fun Element.categoryNames(): List<String> = named("category")

    private fun Element.named(tag: String): List<String> {
        val nodes = getElementsByTagName(tag)
        return (0 until nodes.length).map { (nodes.item(it) as Element).getAttribute("android:name") }
    }

    private fun mainActivity(): Element =
        elements("activity").single { it.getAttribute("android:name") == MAIN_ACTIVITY }

    @Test
    fun launcherFiltersLiveOnTheAliasesAndNotOnTheActivity() {
        assumeTrue("manifest not found from ${File(".").absolutePath}", manifest != null)

        assertTrue(
            "MAIN/LAUNCHER must live on the aliases, not on MainActivity",
            CATEGORY_LAUNCHER !in mainActivity().categoryNames()
        )

        val aliases = elements("activity-alias")
        assertEquals(
            LauncherIconManager.ALIASES.toSet(),
            aliases.map { it.getAttribute("android:name") }.toSet()
        )
        aliases.forEach { alias ->
            val name = alias.getAttribute("android:name")
            assertEquals(
                "$name must alias the fully qualified MainActivity",
                MAIN_ACTIVITY,
                alias.getAttribute("android:targetActivity")
            )
            // Carrying an intent-filter without this is fatal on API 31+.
            assertEquals("$name must be exported", "true", alias.getAttribute("android:exported"))
            assertTrue(
                "$name needs its own icon",
                alias.getAttribute("android:icon").isNotBlank() &&
                    alias.getAttribute("android:roundIcon").isNotBlank()
            )
            assertEquals("$name must carry LAUNCHER", listOf(CATEGORY_LAUNCHER), alias.categoryNames())
            // An alias must not steal the deep links: those belong on the activity,
            // which stays enabled while an alias is switched off.
            assertEquals("$name must carry only MAIN", listOf(ACTION_MAIN), alias.actionNames())
        }
    }

    /** Exactly one alias is enabled in the manifest, and it is the auto-theming default. */
    @Test
    fun theManifestShipsWithOneEnabledAliasAndItIsTheDefault() {
        assumeTrue(manifest != null)
        val enabled = elements("activity-alias")
            .filter { it.getAttribute("android:enabled") != "false" }
            .map { it.getAttribute("android:name") }
        assertEquals(listOf(LauncherIconManager.ALIAS_DEFAULT), enabled)
    }

    @Test
    fun everyAliasPointsAtItsMatchingArtwork() {
        assumeTrue(manifest != null)
        val expected = mapOf(
            LauncherIconManager.ALIAS_DEFAULT to
                ("@mipmap/ic_launcher" to "@mipmap/ic_launcher_round"),
            LauncherIconManager.ALIAS_LIGHT to
                ("@mipmap/ic_launcher_light" to "@mipmap/ic_launcher_light_round"),
            LauncherIconManager.ALIAS_DARK to
                ("@mipmap/ic_launcher_dark" to "@mipmap/ic_launcher_dark_round")
        )
        elements("activity-alias").forEach { alias ->
            val name = alias.getAttribute("android:name")
            val artwork = requireNotNull(expected[name]) { "unexpected launcher alias $name" }
            assertEquals("$name uses the wrong icon", artwork.first, alias.getAttribute("android:icon"))
            assertEquals(
                "$name uses the wrong round icon",
                artwork.second,
                alias.getAttribute("android:roundIcon")
            )
        }
    }

    /** The filters that make imports work must all still be on MainActivity. */
    @Test
    fun mainActivityKeepsEveryOtherIntentFilter() {
        assumeTrue(manifest != null)
        val main = mainActivity()
        val actions = main.actionNames().toSet()
        assertTrue("ACTION_VIEW must stay on MainActivity", ACTION_VIEW in actions)
        assertTrue("ACTION_SEND must stay on MainActivity", ACTION_SEND in actions)

        val nodes = main.getElementsByTagName("data")
        val schemes = (0 until nodes.length)
            .map { (nodes.item(it) as Element).getAttribute("android:scheme") }
        listOf("lumen", "vless", "wireguard", "content", "file").forEach { scheme ->
            assertTrue("scheme $scheme must stay on MainActivity", scheme in schemes)
        }
    }

    @Test
    fun updaterHasPermissionAndAPrivateInstallerStatusReceiver() {
        assumeTrue(manifest != null)
        val permissions = elements("uses-permission")
            .map { it.getAttribute("android:name") }
        assertTrue(
            "in-app updates require REQUEST_INSTALL_PACKAGES",
            "android.permission.REQUEST_INSTALL_PACKAGES" in permissions
        )

        val receiver = elements("receiver").single {
            it.getAttribute("android:name") ==
                "com.lumen.app.update.AndroidUpdateInstallReceiver"
        }
        assertEquals("installer status receiver must stay private", "false",
            receiver.getAttribute("android:exported"))
        assertTrue(
            "the updater no longer needs to expose a FileProvider",
            elements("provider").none {
                it.getAttribute("android:name") == "androidx.core.content.FileProvider"
            }
        )
    }

    private companion object {
        const val MAIN_ACTIVITY = "com.lumen.app.MainActivity"

        // android.content.Intent is not usable from a plain JVM unit test, and the
        // manifest stores these strings verbatim anyway.
        const val ACTION_MAIN = "android.intent.action.MAIN"
        const val ACTION_VIEW = "android.intent.action.VIEW"
        const val ACTION_SEND = "android.intent.action.SEND"
        const val CATEGORY_LAUNCHER = "android.intent.category.LAUNCHER"
    }
}
