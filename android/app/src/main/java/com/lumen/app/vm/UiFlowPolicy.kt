package com.lumen.app.vm

import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.conflate
import kotlinx.coroutines.flow.transform

/**
 * A stream of database/ping snapshots may burst faster than any UI can consume.
 * Deliver the received snapshot immediately, then coalesce pending snapshots while
 * the UI catches up. The trailing value is retained, including an empty final set.
 * This limits data processing, not the display/animation frame clock.
 */
internal fun <T> Flow<T>.coalesceUiUpdates(intervalMillis: Long = 50L): Flow<T> {
    require(intervalMillis > 0L)
    return conflate().transform { snapshot ->
        emit(snapshot)
        delay(intervalMillis)
    }
}
