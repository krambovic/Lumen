package com.lumen.app.network

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.lumen.core.vpn.ActiveProxySession
import com.lumen.ui.screens.NetworkInfoUiState
import com.lumen.ui.screens.NetworkInfoSnapshot
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

internal data class NetworkInfoRequest(val vpn: Boolean, val waiting: Boolean,
    val session: ActiveProxySession?, val serverId: String?, val network: String)

internal class NetworkInfoViewModel(
    private val lookup: suspend (ActiveProxySession?, () -> Boolean) -> NetworkInfoSnapshot = { session, cancelled ->
        NetworkInfoRepository().lookup(session, cancelled)
    },
    private val io: CoroutineDispatcher = Dispatchers.IO
) : ViewModel() {
    private val mutableState = MutableStateFlow(NetworkInfoUiState())
    val state = mutableState.asStateFlow()
    private var lastRequest: NetworkInfoRequest? = null
    private var job: Job? = null

    fun stop() {
        job?.cancel()
        if (mutableState.value.loading) mutableState.value = mutableState.value.copy(loading = false)
    }

    fun load(request: NetworkInfoRequest, force: Boolean = false) {
        val same = request == lastRequest
        if (same && job?.isActive == true) return
        if (same && !force && mutableState.value.updatedAt?.let { System.currentTimeMillis() - it < 60_000 } == true) return
        job?.cancel()
        lastRequest = request
        val previous = mutableState.value
        mutableState.value = if (same) previous.copy(loading = true, failed = false)
            else NetworkInfoUiState(throughVpn = request.vpn, waiting = request.waiting)
        if (request.waiting || (request.vpn && request.session == null)) {
            mutableState.value = mutableState.value.copy(loading = true, waiting = true)
            return
        }
        job = viewModelScope.launch {
            try {
                val result = withContext(io) {
                    val coroutine = currentCoroutineContext()
                    lookup(if (request.vpn) request.session else null) { !coroutine.isActive }
                }
                ensureActive()
                mutableState.value = NetworkInfoUiState(data = result, loading = false,
                    throughVpn = request.vpn, updatedAt = System.currentTimeMillis())
            } catch (e: CancellationException) { throw e }
            catch (_: Exception) {
                ensureActive()
                mutableState.value = mutableState.value.copy(loading = false, failed = true)
            }
        }
    }
}
