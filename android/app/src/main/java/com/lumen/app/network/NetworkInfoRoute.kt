package com.lumen.app.network

import android.content.Intent
import android.net.ConnectivityManager
import android.net.LinkProperties
import android.net.Network
import android.net.Uri
import android.os.Handler
import android.os.Looper
import androidx.compose.runtime.*
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.AnnotatedString
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import com.lumen.app.vm.MainViewModel
import com.lumen.core.vpn.LumenVpnService
import com.lumen.ui.components.ConnectionState
import com.lumen.ui.screens.NetworkInfoScreen

@Composable
internal fun NetworkInfoRoute(main: MainViewModel, model: NetworkInfoViewModel = viewModel()) {
    val context = LocalContext.current
    val clipboard = LocalClipboardManager.current
    val owner = LocalLifecycleOwner.current
    val connection by main.connectionState.collectAsStateWithLifecycle()
    val server by main.activeNode.collectAsStateWithLifecycle()
    val session by LumenVpnService.activeProxySession.collectAsStateWithLifecycle()
    val state by model.state.collectAsStateWithLifecycle()
    val connectivity = remember(context) { context.getSystemService(ConnectivityManager::class.java) }
    var network by remember { mutableStateOf(networkFingerprint(connectivity)) }
    DisposableEffect(connectivity) {
        val callback = object : ConnectivityManager.NetworkCallback() {
            private fun changed() { network = networkFingerprint(connectivity) }
            override fun onAvailable(value: Network) { changed() }
            override fun onLost(value: Network) { changed() }
            override fun onLinkPropertiesChanged(value: Network, properties: LinkProperties) { changed() }
        }
        val registered = runCatching {
            connectivity?.registerDefaultNetworkCallback(callback, Handler(Looper.getMainLooper()))
        }.isSuccess
        onDispose { if (registered) runCatching { connectivity?.unregisterNetworkCallback(callback) } }
    }
    val vpn = connection == ConnectionState.Connected
    val request = NetworkInfoRequest(vpn, connection == ConnectionState.Connecting,
        session.takeIf { vpn }, server?.id.takeIf { vpn }, network)
    val currentRequest by rememberUpdatedState(request)
    LaunchedEffect(request) { model.load(request) }
    DisposableEffect(owner, model) {
        val observer = LifecycleEventObserver { _, event ->
            if (event == Lifecycle.Event.ON_START) model.load(currentRequest)
            if (event == Lifecycle.Event.ON_STOP) model.stop()
        }
        owner.lifecycle.addObserver(observer)
        onDispose { owner.lifecycle.removeObserver(observer); model.stop() }
    }
    NetworkInfoScreen(state, server?.name.takeIf { vpn }, onRefresh = { model.load(request, force = true) },
        onCopy = { clipboard.setText(AnnotatedString(it)) },
        onOpenLink = { uri -> runCatching { context.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(uri))) } })
}

private fun networkFingerprint(manager: ConnectivityManager?): String {
    val active = manager?.activeNetwork ?: return "offline"
    val properties = manager.getLinkProperties(active)
    return "${active.networkHandle}|${properties?.linkAddresses}|${properties?.dnsServers}"
}
