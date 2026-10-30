package com.lumen.app.onboarding

import com.lumen.core.config.builder.SingboxConfigBuilder
import com.lumen.ui.screens.SettingsUiState
import com.lumen.ui.screens.onboardingRuleSets
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.ensureActive
import java.io.File
import java.net.HttpURLConnection
import java.net.URL

/** Stage all missing lists before changing any active file. Called on IO, never on launch. */
internal suspend fun prepareOnboardingGeoResources(
    settings: SettingsUiState, directory: File, version: String,
    replace: (File, File) -> Unit
) {
    directory.mkdirs()
    val staged = mutableListOf<Pair<File, File>>()
    try {
        for ((kind, code) in onboardingRuleSets(settings)) {
            currentCoroutineContext().ensureActive()
            val target = File(directory, "$kind-$code.srs")
            if (target.isFile && target.length() > 0) continue
            val temporary = File.createTempFile("welcome-", ".download", directory)
            staged += temporary to target
            val connection = URL(SingboxConfigBuilder.ruleSetUrl(kind, code, settings.geoResourceSource)).openConnection() as HttpURLConnection
            try {
                connection.connectTimeout = 8_000
                connection.readTimeout = 15_000
                connection.setRequestProperty("User-Agent", "Lumen/$version")
                check(connection.responseCode in 200..299)
                connection.inputStream.use { input -> temporary.outputStream().use { output ->
                    val buffer = ByteArray(8192)
                    var length = 0L
                    while (true) {
                        currentCoroutineContext().ensureActive()
                        val read = input.read(buffer)
                        if (read < 0) break
                        length += read
                        check(length <= 256L * 1024 * 1024)
                        output.write(buffer, 0, read)
                    }
                    check(length >= 64)
                } }
            } finally { connection.disconnect() }
        }
        currentCoroutineContext().ensureActive()
        staged.forEach { (temporary, target) -> replace(temporary, target) }
    } finally {
        staged.forEach { (temporary, _) -> temporary.delete() }
    }
}
