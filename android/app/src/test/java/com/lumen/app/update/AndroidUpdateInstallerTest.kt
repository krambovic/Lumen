package com.lumen.app.update

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class AndroidUpdateInstallerTest {
    @Test
    fun acceptsOnlyOfficialLumenReleaseAssetsOverHttps() {
        assertTrue(
            AndroidUpdateInstaller.isTrustedReleaseAssetUrl(
                "https://github.com/krambovic/Lumen/releases/download/android-v1.2.0/Lumen-1.2.0-arm64-v8a.apk"
            )
        )
        assertFalse(
            AndroidUpdateInstaller.isTrustedReleaseAssetUrl(
                "http://github.com/krambovic/Lumen/releases/download/android-v1.2.0/Lumen.apk"
            )
        )
        assertFalse(
            AndroidUpdateInstaller.isTrustedReleaseAssetUrl(
                "https://github.com/other/Lumen/releases/download/android-v1.2.0/Lumen.apk"
            )
        )
        assertFalse(
            AndroidUpdateInstaller.isTrustedReleaseAssetUrl(
                "https://user@github.com/krambovic/Lumen/releases/download/android-v1.2.0/Lumen.apk"
            )
        )
    }
}
