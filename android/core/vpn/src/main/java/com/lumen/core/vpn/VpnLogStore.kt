package com.lumen.core.vpn

import java.io.File
import java.io.FileOutputStream
import java.io.RandomAccessFile
import kotlin.math.min

/** How much of the log is kept on disk. Both limits are enforced at once. */
data class VpnLogRetention(
    val maxBytes: Long = DEFAULT_MAX_BYTES,
    val maxEntries: Int = DEFAULT_MAX_ENTRIES
) {
    fun sanitized(): VpnLogRetention = VpnLogRetention(
        maxBytes = maxBytes.coerceIn(MIN_MAX_BYTES, MAX_MAX_BYTES),
        maxEntries = maxEntries.coerceIn(MIN_MAX_ENTRIES, MAX_MAX_ENTRIES)
    )

    companion object {
        const val DEFAULT_MAX_BYTES = 2L * 1024 * 1024
        const val DEFAULT_MAX_ENTRIES = 5_000
        const val MIN_MAX_BYTES = 64L * 1024
        const val MAX_MAX_BYTES = 64L * 1024 * 1024
        const val MIN_MAX_ENTRIES = 100
        const val MAX_MAX_ENTRIES = 200_000
    }
}

/**
 * The on-disk half of [VpnLogBus]: an append-only log that survives the process.
 *
 * Two segments, an active one and a single archive, so the total never exceeds the
 * configured budget and nothing is ever rewritten: every entry is appended and
 * flushed as it arrives, which is what keeps the tail after a crash or a kill.
 * Rotation is a rename, so it costs the same whatever the segment holds.
 *
 * Reads walk the file backwards in chunks and stop as soon as the requested number
 * of entries is found: a megabyte-sized log is never pulled into memory to show the
 * last screenful.
 *
 * Every method blocks on IO and must stay off the main thread. [VpnLogBus] calls
 * them from its single writer thread; the methods are synchronized because reads
 * and exports are served on the caller's thread while that writer keeps appending.
 */
class VpnLogStore(
    private val directory: File,
    retention: VpnLogRetention = VpnLogRetention()
) {
    private val activeFile = File(directory, ACTIVE_NAME)
    private val archiveFile = File(directory, ARCHIVE_NAME)

    private var limits: VpnLogRetention = retention.sanitized()
    private var stream: FileOutputStream? = null
    // -1 until the segment left behind by the previous run has been measured, so a
    // restart continues that segment's budget instead of starting a fresh one.
    private var activeBytes = -1L
    private var activeEntries = 0

    @Synchronized
    fun setRetention(value: VpnLogRetention) {
        limits = value.sanitized()
        // A budget lowered in the settings has to take effect now, not once the
        // old, larger segments happen to fill up again.
        if (stream != null) rotateIfNeeded(0)
        enforceArchive()
    }

    @Synchronized
    fun append(entry: VpnLogEntry) {
        val bytes = (encode(entry) + "\n").toByteArray(Charsets.UTF_8)
        openSegment()
        rotateIfNeeded(bytes.size)
        val out = stream ?: return
        out.write(bytes)
        // Flush per entry: buffered lines are exactly the ones a crash loses.
        out.flush()
        activeBytes += bytes.size
        activeEntries++
    }

    /** Newest entries first written, oldest first in the returned list. */
    @Synchronized
    fun readTail(limit: Int, skipFromEnd: Int = 0): List<VpnLogEntry> {
        val skip = skipFromEnd.coerceAtLeast(0)
        val want = limit.coerceAtLeast(0) + skip
        if (limit <= 0 || want <= 0) return emptyList()
        runCatching { stream?.flush() }
        val lines = tailLines(activeFile, want).toMutableList()
        if (lines.size < want) lines.addAll(0, tailLines(archiveFile, want - lines.size))
        val decoded = lines.mapNotNull { decode(it) }
        val end = decoded.size - skip
        if (end <= 0) return emptyList()
        return decoded.subList(maxOf(0, end - limit), end).toList()
    }

    /** Bytes currently held on disk by both segments. */
    fun size(): Long = archiveFile.length() + activeFile.length()

    /** Releases the append handle; the next write reopens the segment. */
    @Synchronized
    fun close() {
        runCatching { stream?.close() }
        stream = null
    }

    @Synchronized
    fun clear() {
        runCatching { stream?.close() }
        stream = null
        runCatching { activeFile.delete() }
        runCatching { archiveFile.delete() }
        activeBytes = -1L
        activeEntries = 0
    }

    /**
     * Writes the whole persisted log to [target] in the human readable form the log
     * screen shows, one entry at a time. Returns how many entries were written.
     */
    @Synchronized
    fun exportTo(target: File): Long {
        runCatching { stream?.flush() }
        target.parentFile?.let { if (!it.isDirectory) it.mkdirs() }
        var written = 0L
        target.bufferedWriter().use { writer ->
            listOf(archiveFile, activeFile).forEach { source ->
                if (!source.isFile) return@forEach
                source.bufferedReader().useLines { lines ->
                    lines.forEach { line ->
                        val entry = decode(line)
                        if (entry != null) {
                            writer.append(format(entry)).append('\n')
                            written++
                        }
                    }
                }
            }
        }
        restrictToOwner(target)
        return written
    }

    private fun openSegment() {
        if (stream != null) return
        if (!directory.isDirectory) directory.mkdirs()
        restrictToOwner(directory)
        activeBytes = if (activeFile.isFile) activeFile.length() else 0L
        activeEntries = if (activeBytes > 0L) countEntries(activeFile) else 0
        stream = FileOutputStream(activeFile, true)
        restrictToOwner(activeFile)
    }

    /**
     * Half the budget per segment: with the archive kept alongside, the pair stays
     * inside the configured size and entry count.
     */
    private fun rotateIfNeeded(incomingBytes: Int) {
        val maxSegmentBytes = (limits.maxBytes / 2).coerceAtLeast(MIN_SEGMENT_BYTES)
        val maxSegmentEntries = (limits.maxEntries / 2).coerceAtLeast(1)
        val bytes = activeBytes.coerceAtLeast(0L) + incomingBytes
        val entries = activeEntries + if (incomingBytes > 0) 1 else 0
        if (bytes <= maxSegmentBytes && entries <= maxSegmentEntries) return
        // An entry larger than the whole budget still gets written; rotating an
        // empty segment would only throw the archive away for nothing.
        if (activeBytes <= 0L) return
        rotate()
    }

    /** Drops an archive that a newly lowered budget no longer has room for. */
    private fun enforceArchive() {
        if (!archiveFile.isFile) return
        val maxSegmentBytes = (limits.maxBytes / 2).coerceAtLeast(MIN_SEGMENT_BYTES)
        val maxSegmentEntries = (limits.maxEntries / 2).coerceAtLeast(1)
        if (archiveFile.length() > maxSegmentBytes || countEntries(archiveFile) > maxSegmentEntries) {
            runCatching { archiveFile.delete() }
        }
    }

    private fun rotate() {
        runCatching { stream?.close() }
        stream = null
        runCatching { archiveFile.delete() }
        if (!activeFile.renameTo(archiveFile)) runCatching { activeFile.delete() }
        openSegment()
    }

    private fun countEntries(file: File): Int {
        var count = 0
        runCatching {
            file.inputStream().buffered().use { input ->
                val buffer = ByteArray(COPY_BUFFER)
                while (true) {
                    val read = input.read(buffer)
                    if (read <= 0) break
                    for (i in 0 until read) if (buffer[i] == NEW_LINE) count++
                }
            }
        }
        return count
    }

    /**
     * Reads the last [limit] lines by seeking backwards from the end, so the amount
     * read is proportional to what is shown and not to the size of the file.
     */
    private fun tailLines(file: File, limit: Int): List<String> {
        if (limit <= 0 || !file.isFile) return emptyList()
        val lines = ArrayDeque<String>()
        runCatching {
            RandomAccessFile(file, "r").use { raf ->
                var end = raf.length()
                var carry = ByteArray(0)
                while (end > 0 && lines.size < limit) {
                    val size = min(CHUNK_BYTES.toLong(), end).toInt()
                    val start = end - size
                    val chunk = ByteArray(size)
                    raf.seek(start)
                    raf.readFully(chunk)
                    // '\n' cannot appear inside a UTF-8 sequence, so splitting the
                    // raw bytes on it can never cut a character in half.
                    val combined = if (carry.isEmpty()) chunk else chunk + carry
                    var lineEnd = combined.size
                    var i = combined.size - 1
                    while (i >= 0 && lines.size < limit) {
                        if (combined[i] == NEW_LINE) {
                            if (lineEnd > i + 1) {
                                lines.addFirst(String(combined, i + 1, lineEnd - i - 1, Charsets.UTF_8))
                            }
                            lineEnd = i
                        }
                        i--
                    }
                    carry = combined.copyOfRange(0, lineEnd)
                    end = start
                    // Only a corrupted file has a single line this long; stop
                    // instead of buffering it whole.
                    if (carry.size > MAX_LINE_BYTES) return@use
                }
                if (end == 0L && carry.isNotEmpty() && lines.size < limit) {
                    lines.addFirst(String(carry, Charsets.UTF_8))
                }
            }
        }
        return lines.toList()
    }

    /** Nothing outside our UID may read the log; it names every server used. */
    private fun restrictToOwner(file: File) {
        runCatching {
            file.setReadable(false, false)
            file.setWritable(false, false)
            file.setExecutable(false, false)
            file.setReadable(true, true)
            file.setWritable(true, true)
            if (file.isDirectory) file.setExecutable(true, true)
        }
    }

    companion object {
        const val ACTIVE_NAME = "vpn-log.txt"
        const val ARCHIVE_NAME = "vpn-log.1.txt"

        private const val MIN_SEGMENT_BYTES = 4L * 1024
        private const val MAX_LINE_BYTES = 64 * 1024
        private const val CHUNK_BYTES = 32 * 1024
        private const val COPY_BUFFER = 8 * 1024
        private val NEW_LINE = '\n'.code.toByte()

        /** The one rendering used by the log screen, the export and the share text. */
        fun format(entry: VpnLogEntry): String =
            "[${entry.formattedTime}] [${entry.level.name}] [${entry.component}] ${entry.message}"

        private fun encode(entry: VpnLogEntry): String = buildString {
            append(entry.timestamp)
            append('\t')
            append(entry.level.name)
            append('\t')
            append(escape(entry.component))
            append('\t')
            append(escape(entry.message))
        }

        private fun decode(line: String): VpnLogEntry? {
            if (line.isBlank()) return null
            val parts = line.split('\t')
            if (parts.size < 4) return null
            val timestamp = parts[0].toLongOrNull() ?: return null
            val level = runCatching { VpnLogLevel.valueOf(parts[1]) }.getOrNull() ?: return null
            return VpnLogEntry(
                timestamp = timestamp,
                formattedTime = VpnLogBus.formatTime(timestamp),
                level = level,
                component = unescape(parts[2]),
                // A stack trace is one entry, so it must survive as one line.
                message = unescape(parts.drop(3).joinToString("\t"))
            )
        }

        private fun escape(value: String): String = value
            .replace("\\", "\\\\")
            .replace("\r", "")
            .replace("\n", "\\n")
            .replace("\t", "\\t")

        private fun unescape(value: String): String {
            if (!value.contains('\\')) return value
            val out = StringBuilder(value.length)
            var i = 0
            while (i < value.length) {
                val current = value[i]
                val next = if (i + 1 < value.length) value[i + 1] else ' '
                if (current == '\\' && (next == 'n' || next == 't' || next == '\\')) {
                    out.append(if (next == 'n') '\n' else if (next == 't') '\t' else '\\')
                    i += 2
                } else {
                    out.append(current)
                    i++
                }
            }
            return out.toString()
        }
    }
}
