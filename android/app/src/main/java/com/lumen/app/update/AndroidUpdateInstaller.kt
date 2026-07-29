package com.lumen.app.update

import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.content.pm.PackageInfo
import android.content.pm.PackageInstaller
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import android.provider.Settings
import androidx.core.content.pm.PackageInfoCompat
import net.kramb.lumen.BuildConfig
import java.io.File
import java.io.FileOutputStream
import java.net.HttpURLConnection
import java.net.URI
import java.net.URL
import java.security.MessageDigest
import java.util.Locale

/**
 * Downloads and verifies a GitHub release APK before handing it to Android's package
 * installer. Android still owns the confirmation UI and the actual package replacement.
 */
internal object AndroidUpdateInstaller {
    private const val UPDATE_DIR = "updates"
    private const val PART_FILE = "lumen-update.apk.part"
    private const val APK_FILE = "lumen-update.apk"
    private const val APK_MIME = "application/vnd.android.package-archive"
    private const val MAX_APK_BYTES = 300L * 1024L * 1024L
    private const val MIN_APK_BYTES = 64L * 1024L
    private const val CONNECT_TIMEOUT_MS = 15_000
    private const val READ_TIMEOUT_MS = 30_000
    private const val RELEASE_HOST = "github.com"
    private const val RELEASE_PATH_PREFIX = "/krambovic/Lumen/releases/download/"

    fun canRequestPackageInstalls(context: Context): Boolean =
        context.packageManager.canRequestPackageInstalls()

    fun unknownSourcesIntent(context: Context): Intent =
        Intent(
            Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES,
            Uri.parse("package:${context.packageName}")
        )

    fun fallbackSecuritySettingsIntent(): Intent = Intent(Settings.ACTION_SECURITY_SETTINGS)

    /**
     * Stages the verified monolithic APK in Android's package installer. The returned
     * session id is also the PendingIntent request code, so simultaneous stale sessions
     * cannot overwrite each other's status callback.
     */
    fun commitInstall(context: Context, apk: File): Int {
        val updateRoot = File(context.cacheDir, UPDATE_DIR).canonicalFile
        val canonicalApk = apk.canonicalFile
        require(canonicalApk.parentFile == updateRoot && canonicalApk.name == APK_FILE) {
            "Refusing to install an APK outside Lumen's update cache"
        }
        check(canonicalApk.isFile && canonicalApk.length() in MIN_APK_BYTES..MAX_APK_BYTES) {
            "The prepared update APK is missing or invalid"
        }

        val installer = context.packageManager.packageInstaller
        val params = PackageInstaller.SessionParams(
            PackageInstaller.SessionParams.MODE_FULL_INSTALL
        ).apply {
            setAppPackageName(context.packageName)
            setSize(canonicalApk.length())
            setInstallReason(PackageManager.INSTALL_REASON_USER)
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
                setRequireUserAction(PackageInstaller.SessionParams.USER_ACTION_REQUIRED)
                setInstallScenario(PackageManager.INSTALL_SCENARIO_FAST)
            }
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
                setPackageSource(PackageInstaller.PACKAGE_SOURCE_DOWNLOADED_FILE)
            }
        }
        val sessionId = installer.createSession(params)
        try {
            installer.openSession(sessionId).use { session ->
                canonicalApk.inputStream().use { input ->
                    session.openWrite(APK_FILE, 0L, canonicalApk.length()).use { output ->
                        input.copyTo(output)
                        session.fsync(output)
                    }
                }
                val callback = Intent(context, AndroidUpdateInstallReceiver::class.java)
                    .setAction(AndroidUpdateInstallReceiver.ACTION_INSTALL_STATUS)
                    .putExtra(AndroidUpdateInstallReceiver.EXTRA_SESSION_ID, sessionId)
                val flags = PendingIntent.FLAG_UPDATE_CURRENT or
                    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
                        PendingIntent.FLAG_MUTABLE
                    } else {
                        0
                    }
                val statusReceiver = PendingIntent.getBroadcast(
                    context,
                    sessionId,
                    callback,
                    flags
                )
                session.commit(statusReceiver.intentSender)
            }
            return sessionId
        } catch (error: Throwable) {
            runCatching { installer.abandonSession(sessionId) }
            throw error
        }
    }

    fun isTrustedReleaseAssetUrl(raw: String): Boolean = runCatching {
        val uri = URI(raw)
        uri.scheme.equals("https", ignoreCase = true) &&
            uri.host.equals(RELEASE_HOST, ignoreCase = true) &&
            uri.path.startsWith(RELEASE_PATH_PREFIX, ignoreCase = false) &&
            uri.userInfo == null &&
            uri.fragment == null
    }.getOrDefault(false)

    fun downloadAndValidate(
        context: Context,
        release: AndroidRelease,
        onProgress: (Int?) -> Unit
    ): File {
        val url = release.apkUrl
            ?: throw IllegalStateException("The Android release has no compatible APK")
        require(isTrustedReleaseAssetUrl(url)) {
            "The update APK is not hosted in the official Lumen GitHub release"
        }

        val directory = File(context.cacheDir, UPDATE_DIR)
        check(directory.exists() || directory.mkdirs()) {
            "Could not create the private update directory"
        }
        val target = File(directory, APK_FILE)
        if (target.isFile) {
            runCatching {
                validateDownloadedPackage(context, target, release.version)
                return target
            }.onFailure { target.delete() }
        }

        val partial = File(directory, PART_FILE)
        partial.delete()
        var connection: HttpURLConnection? = null
        try {
            connection = (URL(url).openConnection() as HttpURLConnection).apply {
                requestMethod = "GET"
                instanceFollowRedirects = true
                connectTimeout = CONNECT_TIMEOUT_MS
                readTimeout = READ_TIMEOUT_MS
                setRequestProperty("Accept", APK_MIME)
                setRequestProperty("User-Agent", "Lumen-Android-Updater/${BuildConfig.VERSION_NAME}")
            }
            val status = connection.responseCode
            if (status !in 200..299) {
                throw IllegalStateException("GitHub returned HTTP $status while downloading the APK")
            }
            check(connection.url.protocol.equals("https", ignoreCase = true)) {
                "GitHub redirected the update download away from HTTPS"
            }
            val expected = connection.contentLengthLong
            if (expected > MAX_APK_BYTES) {
                throw IllegalStateException("The update APK is unexpectedly large")
            }
            onProgress(if (expected > 0L) 0 else null)

            var copied = 0L
            var lastProgress = -1
            connection.inputStream.use { input ->
                FileOutputStream(partial).use { output ->
                    val buffer = ByteArray(DEFAULT_BUFFER_SIZE)
                    while (true) {
                        val count = input.read(buffer)
                        if (count < 0) break
                        copied += count
                        if (copied > MAX_APK_BYTES) {
                            throw IllegalStateException("The update APK exceeded the size limit")
                        }
                        output.write(buffer, 0, count)
                        if (expected > 0L) {
                            val progress = ((copied * 100L) / expected)
                                .toInt()
                                .coerceIn(0, 100)
                            if (progress != lastProgress) {
                                lastProgress = progress
                                onProgress(progress)
                            }
                        }
                    }
                    output.fd.sync()
                }
            }
            if (copied < MIN_APK_BYTES || (expected > 0L && copied != expected)) {
                throw IllegalStateException("The downloaded APK is incomplete")
            }
            if (!partial.renameTo(target)) {
                partial.copyTo(target, overwrite = true)
                partial.delete()
            }
            validateDownloadedPackage(context, target, release.version)
            onProgress(100)
            return target
        } catch (error: Throwable) {
            partial.delete()
            target.delete()
            throw error
        } finally {
            connection?.disconnect()
        }
    }

    fun validateDownloadedPackage(
        context: Context,
        apk: File,
        expectedVersion: String
    ) {
        if (!apk.isFile || apk.length() < MIN_APK_BYTES || apk.length() > MAX_APK_BYTES) {
            throw IllegalStateException("The downloaded APK file is invalid")
        }
        val packageManager = context.packageManager
        val archive = archivePackageInfo(packageManager, apk)
            ?: throw IllegalStateException("Android could not read the downloaded APK")
        if (archive.packageName != context.packageName) {
            throw IllegalStateException("The downloaded APK belongs to another application")
        }
        if (archive.versionName.orEmpty() != expectedVersion) {
            throw IllegalStateException(
                "APK version ${archive.versionName.orEmpty()} does not match release $expectedVersion"
            )
        }
        val current = installedPackageInfo(packageManager, context.packageName)
        if (PackageInfoCompat.getLongVersionCode(archive) <=
            PackageInfoCompat.getLongVersionCode(current)
        ) {
            throw IllegalStateException("The downloaded APK is not newer than the installed version")
        }

        val installedSigners = signerDigests(current)
        val archiveSigners = signerDigests(archive)
        if (installedSigners.isEmpty() || archiveSigners.isEmpty() ||
            installedSigners.intersect(archiveSigners).isEmpty()
        ) {
            throw IllegalStateException("The update APK is signed with a different certificate")
        }
    }

    @Suppress("DEPRECATION")
    private fun archivePackageInfo(packageManager: PackageManager, apk: File): PackageInfo? =
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            packageManager.getPackageArchiveInfo(
                apk.absolutePath,
                PackageManager.PackageInfoFlags.of(
                    PackageManager.GET_SIGNING_CERTIFICATES.toLong()
                )
            )
        } else if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
            packageManager.getPackageArchiveInfo(
                apk.absolutePath,
                PackageManager.GET_SIGNING_CERTIFICATES
            )
        } else {
            packageManager.getPackageArchiveInfo(
                apk.absolutePath,
                PackageManager.GET_SIGNATURES
            )
        }

    @Suppress("DEPRECATION")
    private fun installedPackageInfo(
        packageManager: PackageManager,
        packageName: String
    ): PackageInfo =
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            packageManager.getPackageInfo(
                packageName,
                PackageManager.PackageInfoFlags.of(
                    PackageManager.GET_SIGNING_CERTIFICATES.toLong()
                )
            )
        } else if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
            packageManager.getPackageInfo(packageName, PackageManager.GET_SIGNING_CERTIFICATES)
        } else {
            packageManager.getPackageInfo(packageName, PackageManager.GET_SIGNATURES)
        }

    @Suppress("DEPRECATION")
    private fun signerDigests(info: PackageInfo): Set<String> {
        val signatures = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
            val signingInfo = info.signingInfo ?: return emptySet()
            if (signingInfo.hasMultipleSigners()) {
                signingInfo.apkContentsSigners
            } else {
                signingInfo.signingCertificateHistory
            }
        } else {
            info.signatures ?: return emptySet()
        }
        return signatures.mapTo(mutableSetOf()) { signature ->
            MessageDigest.getInstance("SHA-256")
                .digest(signature.toByteArray())
                .joinToString("") { byte -> "%02x".format(Locale.US, byte) }
        }
    }
}
