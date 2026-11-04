package com.lumen.app.vm

import java.net.Socket
import java.util.concurrent.LinkedBlockingQueue
import java.util.concurrent.ThreadPoolExecutor
import java.util.concurrent.TimeUnit
import kotlinx.coroutines.suspendCancellableCoroutine

/** Blocking DNS/socket APIs must not hold a coroutine or its probe permit after cancellation. */
internal object ProbeIo {
    private val executor = ThreadPoolExecutor(32, 32, 30, TimeUnit.SECONDS, LinkedBlockingQueue()) { task ->
        Thread(task, "lumen-probe-io").apply { isDaemon = true }
    }.apply { allowCoreThreadTimeOut(true) }

    suspend fun <T> run(onCancel: () -> Unit = {}, block: () -> T): T =
        suspendCancellableCoroutine { continuation ->
            val task = executor.submit {
                if (continuation.isActive) continuation.resumeWith(runCatching(block))
            }
            continuation.invokeOnCancellation {
                runCatching(onCancel)
                task.cancel(true)
                if (task is Runnable) executor.remove(task)
            }
        }

    suspend fun <T> socket(block: (Socket) -> T): T {
        val socket = Socket()
        return try {
            run(onCancel = { socket.close() }) { socket.use(block) }
        } finally {
            socket.close()
        }
    }
}
