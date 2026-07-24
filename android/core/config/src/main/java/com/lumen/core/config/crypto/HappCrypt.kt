package com.lumen.core.config.crypto

import java.io.ByteArrayOutputStream
import java.math.BigInteger
import java.security.KeyFactory
import java.security.PrivateKey
import java.security.interfaces.RSAPrivateKey
import java.security.spec.PKCS8EncodedKeySpec
import java.util.Base64
import java.util.concurrent.ConcurrentHashMap
import java.util.regex.Pattern
import javax.crypto.Cipher
import javax.crypto.spec.IvParameterSpec
import javax.crypto.spec.SecretKeySpec

open class HappDecryptError(message: String, cause: Throwable? = null) : Exception(message, cause)

class HappKeyUnavailableError(message: String, cause: Throwable? = null) : HappDecryptError(message, cause)

object HappCrypt {

    const val HAPP_SCHEME = "happ://"

    private val LEGACY_DIGIT_PATTERN = Pattern.compile("^\\d+")
    private val REGEX_TRAILING_EQUALS = Regex("=+$")
    private val REGEX_LEADING_EQUALS = Regex("^=+")

    private val CRYPT_PREFIXES = listOf(
        Pair("crypt5/", 4),
        Pair("crypt4/", 3),
        Pair("crypt3/", 2),
        Pair("crypt2/", 1),
        Pair("crypt/", 0)
    )

    fun isHappLink(text: String?): Boolean {
        return text?.trim()?.lowercase()?.startsWith(HAPP_SCHEME) == true
    }

    fun isHappCryptLink(text: String?): Boolean {
        return splitCryptPrefix(text) != null
    }

    fun decryptHappLink(link: String): String {
        val parsed = splitCryptPrefix(link)
            ?: throw HappDecryptError("Unsupported scheme (expected happ://crypt*)")
        val (ordinal, payload) = parsed
        if (payload.isEmpty()) {
            throw HappDecryptError("Empty body of happ://crypt link")
        }
        return try {
            if (ordinal == 4) {
                decryptCrypt5(payload)
            } else {
                decryptCrypt1To4(ordinal, payload)
            }
        } catch (e: HappDecryptError) {
            throw e
        } catch (e: Exception) {
            throw HappDecryptError("Happ link decryption error: ${e.message}", e)
        }
    }

    private fun splitCryptPrefix(text: String?): Pair<Int, String>? {
        val body = text?.trim() ?: return null
        if (body.length < HAPP_SCHEME.length || !body.substring(0, HAPP_SCHEME.length).equals(HAPP_SCHEME, ignoreCase = true)) {
            return null
        }
        val path = body.substring(HAPP_SCHEME.length)
        for ((prefix, ordinal) in CRYPT_PREFIXES) {
            if (path.length >= prefix.length && path.substring(0, prefix.length).equals(prefix, ignoreCase = true)) {
                return Pair(ordinal, path.substring(prefix.length))
            }
        }
        return null
    }

    private fun b64decode(text: String): ByteArray {
        var compact = text.replace("-", "+").replace("_", "/")
        compact = compact.filterNot { it.isWhitespace() }
        val padLen = (4 - (compact.length % 4)) % 4
        val padded = compact + "=".repeat(padLen)
        return try {
            Base64.getDecoder().decode(padded)
        } catch (e: Exception) {
            Base64.getUrlDecoder().decode(padded)
        }
    }

    private fun swapPairs(text: String): String {
        val chars = text.toCharArray()
        var i = 0
        while (i < chars.size - 1) {
            val tmp = chars[i]
            chars[i] = chars[i + 1]
            chars[i + 1] = tmp
            i += 2
        }
        return String(chars)
    }

    private fun blockPairSwap(text: String): String {
        val full = text.length - (text.length % 4)
        val sb = StringBuilder()
        var offset = 0
        while (offset < full) {
            sb.append(text.substring(offset + 2, offset + 4))
            sb.append(text.substring(offset, offset + 2))
            offset += 4
        }
        sb.append(text.substring(full))
        return sb.toString()
    }

    private fun wrapPkcs1InPkcs8(der: ByteArray): ByteArray {
        fun derLen(size: Int): ByteArray {
            if (size < 0x80) return byteArrayOf(size.toByte())
            val raw = BigInteger.valueOf(size.toLong()).toByteArray()
            val trimmed = if (raw[0] == 0.toByte() && raw.size > 1) raw.copyOfRange(1, raw.size) else raw
            return byteArrayOf((0x80 or trimmed.size).toByte()) + trimmed
        }
        val hexAlgo = "300d06092a864886f70d0101010500"
        val rsaAlgoId = ByteArray(hexAlgo.length / 2) { i ->
            hexAlgo.substring(i * 2, i * 2 + 2).toInt(16).toByte()
        }
        val octet = byteArrayOf(0x04) + derLen(der.size) + der
        val body = byteArrayOf(0x02, 0x01, 0x00) + rsaAlgoId + octet
        return byteArrayOf(0x30) + derLen(body.size) + body
    }

    private val keyCache = ConcurrentHashMap<String, PrivateKey>()

    private fun loadPrivateKey(b64Key: String): PrivateKey {
        keyCache[b64Key]?.let { return it }
        val der = b64decode(b64Key)
        val keyFactory = KeyFactory.getInstance("RSA")
        val key = try {
            keyFactory.generatePrivate(PKCS8EncodedKeySpec(der))
        } catch (e: Exception) {
            val pkcs8Der = wrapPkcs1InPkcs8(der)
            keyFactory.generatePrivate(PKCS8EncodedKeySpec(pkcs8Der))
        }
        keyCache[b64Key] = key
        return key
    }

    private fun rsaDecrypt(b64Key: String, ciphertext: ByteArray): ByteArray {
        val privateKey = loadPrivateKey(b64Key)
        val cipher = Cipher.getInstance("RSA/ECB/PKCS1Padding")
        cipher.init(Cipher.DECRYPT_MODE, privateKey)
        return cipher.doFinal(ciphertext)
    }

    private fun chachaDecrypt(key: ByteArray, nonce: ByteArray, ciphertext: ByteArray): ByteArray {
        val cipher = Cipher.getInstance("ChaCha20-Poly1305")
        val keySpec = SecretKeySpec(key, "ChaCha20")
        val paramSpec = IvParameterSpec(nonce)
        cipher.init(Cipher.DECRYPT_MODE, keySpec, paramSpec)
        return cipher.doFinal(ciphertext)
    }

    private fun decryptCrypt1To4(ordinal: Int, payload: String): String {
        val b64Key = HappCryptKeys.PKCS1_KEYS_B64.getOrNull(ordinal)
            ?: throw HappDecryptError("No RSA key for crypt mode $ordinal")
        val key = loadPrivateKey(b64Key)
        val keySize = (((key as RSAPrivateKey).modulus.bitLength() + 7) / 8)
        val cipherBytes = b64decode(payload)
        if (cipherBytes.isEmpty() || cipherBytes.size % keySize != 0) {
            throw HappDecryptError("crypt: ciphertext length is not a multiple of RSA block size")
        }
        val cipher = Cipher.getInstance("RSA/ECB/PKCS1Padding")
        cipher.init(Cipher.DECRYPT_MODE, key)

        val out = ByteArrayOutputStream()
        var offset = 0
        while (offset < cipherBytes.size) {
            val part = cipher.doFinal(cipherBytes, offset, keySize)
            out.write(part)
            offset += keySize
        }
        return String(out.toByteArray(), Charsets.UTF_8)
    }

    private fun finishCrypt5(nonce: ByteArray, urlB64: String, encStr: String, keyB64: String): String {
        val rsaPlainBytes = rsaDecrypt(keyB64, b64decode(encStr))
        val rsaPlain = String(rsaPlainBytes, Charsets.ISO_8859_1)
        val chachaKey = b64decode(swapPairs(rsaPlain))
        if (chachaKey.size != 32) {
            throw HappDecryptError("crypt5: invalid ChaCha20 key length (${chachaKey.size})")
        }
        val intermediateBytes = chachaDecrypt(chachaKey, nonce, b64decode(urlB64))
        val intermediateStr = String(intermediateBytes, Charsets.UTF_8)
        return String(b64decode(swapPairs(intermediateStr)), Charsets.UTF_8)
    }

    private fun tryDecryptCrypt5Legacy(payload: String): String? {
        val shuffled = blockPairSwap(payload)
        if (shuffled.length < 8) return null
        val marker = shuffled.substring(0, 4) + shuffled.substring(shuffled.length - 4)
        val keyB64 = HappCryptKeys.CRYPT5_KEYS_B64[marker] ?: return null
        val body = shuffled.substring(4, shuffled.length - 4)
        if (body.length < 13) return null

        val tail = body.substring(12)
        val matcher = LEGACY_DIGIT_PATTERN.matcher(tail)
        if (!matcher.find()) return null

        val segmentLenStr = matcher.group()
        val segmentLen = segmentLenStr.toIntOrNull() ?: return null
        val packed = tail.substring(segmentLenStr.length)
        if (packed.length < 1 + segmentLen) return null

        val urlB64 = packed.substring(1, 1 + segmentLen)
        val encStr = packed.substring(1 + segmentLen)
        return try {
            val nonce = body.substring(0, 12).toByteArray(Charsets.US_ASCII)
            finishCrypt5(nonce, urlB64, encStr, keyB64)
        } catch (e: Exception) {
            null
        }
    }

    private fun c51BlockPairSwap(region: String, length: Int): String {
        val sb = StringBuilder()
        for (j in 1..length) {
            val block = (j - 1) / 4
            val pos = (j - 1) % 4
            val index = 4 * block + ((pos + 2) % 4)
            if (index < region.length) {
                sb.append(region[index])
            }
        }
        return sb.toString()
    }

    private fun c51ExtractNonce(payload: String): String {
        if (payload.length < 16) return ""
        val n = payload.substring(4, 16)
        return "" + n[2] + n[3] + n[0] + n[1] + n[6] + n[7] + n[4] + n[5] + n[10] + n[11] + n[8] + n[9]
    }

    private fun c51Selector(payload: String): String {
        if (payload.length < 10) return ""
        return (payload.substring(2, 4) + payload.substring(0, 2) +
                payload.substring(payload.length - 6, payload.length - 4) +
                payload.substring(payload.length - 2)).lowercase()
    }

    private fun c51MakeCipherB64(encStr: String, splitOnInnerEquals: Boolean): String {
        val trailingStart = encStr.replace(REGEX_TRAILING_EQUALS, "").length
        val eqIdx = encStr.indexOf('=')
        val cipherB64 = if (splitOnInnerEquals && eqIdx in 0 until trailingStart) {
            encStr.substring(eqIdx + 1)
        } else {
            encStr
        }
        val cleaned = cipherB64.replace(REGEX_LEADING_EQUALS, "").replace(REGEX_TRAILING_EQUALS, "")
        val padLen = (4 - (cleaned.length % 4)) % 4
        return cleaned + "=".repeat(padLen)
    }

    private data class C51Candidate(
        val nonceStr: String,
        val urlB64: String,
        val encStr: String,
        val split: Boolean
    )

    private fun c51Candidates(payload: String): List<C51Candidate> {
        val nonceStr = c51ExtractNonce(payload)
        val candidates = mutableListOf<C51Candidate>()
        val seen = mutableSetOf<String>()

        fun push(urlB64: String, encStr: String, split: Boolean) {
            if (urlB64.isEmpty() || encStr.length < 684) return
            val key = "${urlB64.length}:${urlB64.take(16)}:${urlB64.takeLast(16)}:${encStr.take(16)}:${encStr.takeLast(16)}"
            if (seen.contains(key)) return
            seen.add(key)
            candidates.add(C51Candidate(nonceStr, urlB64, encStr, split))
        }

        val n = if (payload.length >= 20) payload.substring(18, 20).toIntOrNull() ?: 0 else 0
        if (n > 0 && payload.length >= 20 + n + 684) {
            val urlRegion = payload.substring(20, 20 + n)
            val encRegion = payload.substring(20 + n, 20 + n + 684)
            val skip = ((n - 1) / 4) * 4 + 1
            val urlB64 = payload[17] + c51BlockPairSwap(urlRegion, n - 1)
            val encStr = urlRegion[skip] + c51BlockPairSwap(encRegion, 683)
            push(urlB64, encStr, true)
        }

        for (trailerLen in 4..8) {
            val urlLen = payload.length - 20 - 684 - trailerLen
            if (urlLen <= 0) continue
            val urlRegion = payload.substring(20, 20 + urlLen)
            val encRegion = payload.substring(20 + urlLen, 20 + urlLen + 684)
            if (encRegion.length != 684) continue
            val urlB64 = c51BlockPairSwap(urlRegion, urlLen)
            val encStr = c51BlockPairSwap(encRegion, 684)
            push(urlB64, encStr, false)
            if (urlB64.endsWith("=")) {
                push(urlB64.substring(1) + "=", encStr, false)
            }
        }
        return candidates
    }

    private fun candidateKeys(selector: String): List<String> {
        val keys = mutableListOf<String>()
        val seen = mutableSetOf<String>()

        fun add(value: String?) {
            if (!value.isNullOrEmpty() && seen.add(value)) {
                keys.add(value)
            }
        }

        add(HappCryptKeys.CRYPT5_KEYS_B64[selector])
        val family = selector.take(4)
        if (family.isNotEmpty()) {
            for ((marker, value) in HappCryptKeys.CRYPT5_KEYS_B64) {
                if (marker.startsWith(family)) {
                    add(value)
                }
            }
        }
        for (value in HappCryptKeys.CRYPT5_KEYS_B64.values) {
            add(value)
        }
        return keys
    }

    private fun decryptCrypt51(payload: String): String {
        val selector = c51Selector(payload)
        val keys = candidateKeys(selector)
        val candidates = c51Candidates(payload)
        val preferredValues = HappCryptKeys.CRYPT5_KEYS_B64.filter { (marker, _) ->
            marker == selector || (selector.length >= 4 && marker.startsWith(selector.substring(0, 4)))
        }.values.toSet()

        val preferredKeys = keys.filter { preferredValues.contains(it) }
        val fallbackKeys = keys.filter { !preferredValues.contains(it) }

        fun decryptWithKeys(keyCandidates: List<String>): String? {
            for (cand in candidates) {
                val nonce = try {
                    cand.nonceStr.toByteArray(Charsets.US_ASCII)
                } catch (e: Exception) {
                    continue
                }
                val cipherB64 = try {
                    c51MakeCipherB64(cand.encStr, cand.split)
                } catch (e: Exception) {
                    continue
                }

                for (keyB64 in keyCandidates) {
                    val rsaPlain = try {
                        String(rsaDecrypt(keyB64, b64decode(cipherB64)), Charsets.ISO_8859_1)
                    } catch (e: Exception) {
                        continue
                    }
                    for (shaped in listOf(swapPairs(rsaPlain), rsaPlain)) {
                        try {
                            val chachaKey = b64decode(shaped)
                            if (chachaKey.size != 32) continue
                            val intermediateBytes = chachaDecrypt(chachaKey, nonce, b64decode(cand.urlB64))
                            val intermediateStr = String(intermediateBytes, Charsets.UTF_8)
                            return String(b64decode(swapPairs(intermediateStr)), Charsets.UTF_8)
                        } catch (e: Exception) {
                            continue
                        }
                    }
                }
            }
            return null
        }

        val decrypted = decryptWithKeys(preferredKeys)
        if (decrypted != null) return decrypted

        val fallbackDecrypted = decryptWithKeys(fallbackKeys)
        if (fallbackDecrypted != null) return fallbackDecrypted

        throw HappKeyUnavailableError(
            "Failed to decrypt happ://crypt5 link: private key unavailable in key set (format crypt5.1, marker '$selector')"
        )
    }

    private fun decryptCrypt5(payload: String): String {
        val legacy = tryDecryptCrypt5Legacy(payload)
        if (legacy != null) return legacy
        return decryptCrypt51(payload)
    }
}
