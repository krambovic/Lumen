package com.lumen.app.update

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.pm.PackageInstaller
import android.os.Build
import android.widget.Toast
import com.lumen.core.vpn.VpnLogBus

/**
 * Receives the result of a PackageInstaller session. A regular app is not allowed
 * to update itself silently, so STATUS_PENDING_USER_ACTION is the expected path:
 * Android supplies the trusted confirmation activity which this receiver opens.
 */
class AndroidUpdateInstallReceiver : BroadcastReceiver() {

    override fun onReceive(context: Context, intent: Intent) {
        if (intent.action != ACTION_INSTALL_STATUS) return
        val status = intent.getIntExtra(
            PackageInstaller.EXTRA_STATUS,
            PackageInstaller.STATUS_FAILURE
        )
        when (status) {
            PackageInstaller.STATUS_PENDING_USER_ACTION -> {
                val confirmation = confirmationIntent(intent)
                if (confirmation == null) {
                    reportFailure(context, "Android did not provide an update confirmation")
                    return
                }
                runCatching {
                    confirmation.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
                    context.startActivity(confirmation)
                }.onFailure { error ->
                    reportFailure(
                        context,
                        error.message ?: "Android could not open the update confirmation"
                    )
                }
            }

            PackageInstaller.STATUS_SUCCESS -> {
                // Package replacement stops the old process. The launcher entry and
                // Android installer's Open action now point to the newly installed APK.
                VpnLogBus.info("UPDATE", "Android update installed successfully")
                launchUpdatedApp(context)
            }

            else -> {
                val message = intent.getStringExtra(PackageInstaller.EXTRA_STATUS_MESSAGE)
                    ?.takeIf(String::isNotBlank)
                    ?: "Package installer failed with status $status"
                reportFailure(context, message)
            }
        }
    }

    @Suppress("DEPRECATION")
    private fun confirmationIntent(source: Intent): Intent? =
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            source.getParcelableExtra(Intent.EXTRA_INTENT, Intent::class.java)
        } else {
            source.getParcelableExtra(Intent.EXTRA_INTENT)
        }

    private fun reportFailure(context: Context, message: String) {
        VpnLogBus.warning("UPDATE", "Android update installation failed: $message")
        Toast.makeText(
            context,
            "Lumen update failed: $message",
            Toast.LENGTH_LONG
        ).show()
    }

    private fun launchUpdatedApp(context: Context) {
        val launchIntent = context.packageManager
            .getLaunchIntentForPackage(context.packageName)
            ?.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TOP)
        if (launchIntent == null) {
            VpnLogBus.warning("UPDATE", "Updated Lumen has no launcher activity")
            return
        }
        runCatching { context.startActivity(launchIntent) }
            .onFailure { error ->
                // Some OEMs deliberately keep their installer on screen after success.
                // In that case its Open button and the launcher icon still start the
                // updated package, so a blocked automatic return is not an install error.
                VpnLogBus.warning(
                    "UPDATE",
                    "Updated Lumen could not be reopened automatically: ${error.message}"
                )
            }
    }

    companion object {
        const val ACTION_INSTALL_STATUS = "net.kramb.lumen.action.UPDATE_INSTALL_STATUS"
        const val EXTRA_SESSION_ID = "update_session_id"
    }
}
