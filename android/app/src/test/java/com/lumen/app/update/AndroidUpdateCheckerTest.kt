package com.lumen.app.update

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class AndroidUpdateCheckerTest {
    @Test
    fun selectsAndroidTagInsteadOfNewerWindowsRelease() {
        val release = AndroidUpdateChecker.selectLatest(
            """
            [
              {"tag_name":"v9.9.9","draft":false,"prerelease":false,
               "html_url":"https://example/windows","assets":[]},
              {"tag_name":"android-v1.2.0","draft":false,"prerelease":false,
               "html_url":"https://example/android","assets":[
                 {"name":"Lumen-1.2.0-universal.apk","browser_download_url":"https://example/u.apk"},
                 {"name":"Lumen-1.2.0-arm64-v8a.apk","browser_download_url":"https://example/a.apk"}
               ]}
            ]
            """.trimIndent(),
            listOf("arm64-v8a", "armeabi-v7a")
        )

        assertEquals("android-v1.2.0", release?.tag)
        assertEquals("1.2.0", release?.version)
        assertEquals("Lumen-1.2.0-arm64-v8a.apk", release?.apkName)
    }

    @Test
    fun ignoresDraftsPrereleasesAndNonAndroidNames() {
        val release = AndroidUpdateChecker.selectLatest(
            """
            [
              {"tag_name":"android-v3.0.0","draft":true,"prerelease":false,"assets":[]},
              {"tag_name":"android-v2.0.0","draft":false,"prerelease":true,"assets":[]},
              {"tag_name":"v1.9.9","draft":false,"prerelease":false,"assets":[]}
            ]
            """.trimIndent(),
            emptyList()
        )
        assertNull(release)
    }

    @Test
    fun comparesVersionsNumerically() {
        assertTrue(AndroidUpdateChecker.isNewer("1.10.0", "1.9.9"))
        assertFalse(AndroidUpdateChecker.isNewer("1.1.0", "1.1.0"))
        assertFalse(AndroidUpdateChecker.isNewer("1.0.9", "1.1.0"))
    }

    @Test
    fun newestReleaseWithoutCompatibleApkDoesNotHideInstallableRelease() {
        val release = AndroidUpdateChecker.selectLatest(
            """
            [
              {"tag_name":"android-v2.0.0","draft":false,"prerelease":false,
               "html_url":"https://example/new","assets":[
                 {"name":"Lumen-2.0.0-x86_64.apk","browser_download_url":"https://example/x.apk"}
               ]},
              {"tag_name":"android-v1.9.0","draft":false,"prerelease":false,
               "html_url":"https://example/old","assets":[
                 {"name":"Lumen-1.9.0-universal.apk","browser_download_url":"https://example/u.apk"}
               ]}
            ]
            """.trimIndent(),
            listOf("arm64-v8a")
        )

        assertEquals("android-v1.9.0", release?.tag)
        assertEquals("Lumen-1.9.0-universal.apk", release?.apkName)
    }

    @Test
    fun incompatibleSingletonApkIsNeverSelected() {
        assertNull(
            AndroidUpdateChecker.selectApkAsset(
                listOf("Lumen-2.0.0-x86_64.apk" to "https://example/x.apk"),
                listOf("arm64-v8a")
            )
        )
    }
}
