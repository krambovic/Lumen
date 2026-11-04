package com.lumen.app.profiles

import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import com.lumen.core.config.crypto.ProtectedProfileCodec
import com.lumen.core.config.crypto.LumenProfileEnvelope
import com.lumen.core.config.crypto.LumenProfilePayload
import com.lumen.core.vpn.ProfilePolicyEnforcer
import com.lumen.core.config.parser.ParsedNode
import java.security.KeyPairGenerator
import java.security.KeyStore
import java.security.PrivateKey
import java.util.Base64
import android.content.Context
import java.io.File
import java.security.MessageDigest

object ProtectedProfiles {
    private const val ALIAS = "lumen-protected-profiles-v1"
    @Synchronized private fun store(): KeyStore {
        val store = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
        if (!store.containsAlias(ALIAS)) {
            fun generate(size: Int) {
                KeyPairGenerator.getInstance("RSA", "AndroidKeyStore").apply {
                    initialize(KeyGenParameterSpec.Builder(ALIAS, KeyProperties.PURPOSE_DECRYPT)
                        .setKeySize(size).setDigests(KeyProperties.DIGEST_SHA256)
                        .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_RSA_OAEP).build())
                }.generateKeyPair()
            }
            // Some older Keymaster implementations support RSA-2048 only.
            // Existing recipient keys are deliberately never rotated on update.
            try { generate(3072) } catch (_: Exception) { generate(2048) }
        }
        return store
    }
    fun recipientKey(): String = "lumen-key-v1:" + Base64.getUrlEncoder().withoutPadding()
        .encodeToString(store().getCertificate(ALIAS).publicKey.encoded)

    @Synchronized fun clearStoredProfiles(context: Context) {
        File(context.noBackupFilesDir, "protected-profiles").listFiles().orEmpty().forEach { file ->
            if (file.isFile) check(file.delete()) { "Cannot remove protected profile" }
        }
        val prefs = context.getSharedPreferences(ProfilePolicyEnforcer.PREFS_NAME, Context.MODE_PRIVATE)
        val edit = prefs.edit()
        prefs.all.keys.filter { it.startsWith("expiry_") }.forEach(edit::remove)
        edit.apply()
    }
    /** Replace password delivery with local-key encryption; never persist the password. */
    fun unlockForImport(envelope: String, password: CharArray): String {
        val profile = LumenProfileEnvelope.open(envelope, password = password)
        return LumenProfileEnvelope.seal(profile, LumenProfileEnvelope.Delivery.RECIPIENTS,
            listOf(store().getCertificate(ALIAS).publicKey))
    }

    fun parse(envelope: String, context: Context): Pair<List<ParsedNode>, List<String>> {
        val profile = LumenProfileEnvelope.open(envelope, store().getKey(ALIAS, null) as PrivateKey)
        profile.policy.requireUsable(ProfilePolicyEnforcer.effectiveNow(context))
        val nodes = LumenProfilePayload.decode(profile.config)
        if (!profile.locked) return nodes.mapIndexed { index, node ->
            node.copy(name = profile.name.ifBlank { node.name } + if (nodes.size > 1) " ${index + 1}" else "")
        } to emptyList()
        // Public delivery is not encrypted in transit; at rest every closed
        // profile is encrypted for this installation's hardware-backed key.
        val storedEnvelope = if (org.json.JSONObject(envelope).optString("delivery") == "ANYONE")
            LumenProfileEnvelope.seal(profile, LumenProfileEnvelope.Delivery.RECIPIENTS,
                listOf(store().getCertificate(ALIAS).publicKey)) else envelope
        val hash = MessageDigest.getInstance("SHA-256").digest(storedEnvelope.toByteArray()).joinToString("") { "%02x".format(it) }
        val file = File(File(context.noBackupFilesDir, "protected-profiles").apply { mkdirs() }, "$hash.lumen")
        synchronized(this) {
            if (!file.isFile) {
                val atomic = android.util.AtomicFile(file)
                val output = atomic.startWrite()
                try {
                    output.write(storedEnvelope.toByteArray(Charsets.UTF_8))
                    atomic.finishWrite(output)
                } catch (failure: Exception) {
                    atomic.failWrite(output)
                    throw failure
                }
            }
        }
        if (profile.policy.expiresAtMs > 0) context.getSharedPreferences(ProfilePolicyEnforcer.PREFS_NAME, Context.MODE_PRIVATE)
            .edit().putLong("expiry_$hash", profile.policy.expiresAtMs).apply()
        return nodes.mapIndexed { index, node -> node.copy(
            name = profile.name.ifBlank { "Protected profile" } + if (nodes.size > 1) " ${index + 1}" else "",
            link = ProtectedProfileCodec.PREFIX + "store/$hash/$index") } to emptyList()
    }
    fun resolve(reference: String, context: Context): ParsedNode {
        val (envelope, index) = if (reference.startsWith(ProtectedProfileCodec.PREFIX + "store/")) {
            val parts = reference.removePrefix(ProtectedProfileCodec.PREFIX + "store/").split('/')
            require(parts.size == 2 && parts[0].matches(Regex("[0-9a-f]{64}")))
            val file = File(File(context.noBackupFilesDir, "protected-profiles"), "${parts[0]}.lumen")
            require(file.length() in 1..8_388_608)
            file.readText() to parts[1].toInt()
        } else ProtectedProfileCodec.referenceParts(reference)
        val profile = LumenProfileEnvelope.open(envelope, store().getKey(ALIAS, null) as PrivateKey)
        profile.policy.requireUsable(ProfilePolicyEnforcer.effectiveNow(context))
        // Runtime must receive the original source, including OpenVPN directives and
        // full core configs. Storage references are only for the database/UI.
        return (LumenProfilePayload.decode(profile.config).getOrNull(index)
            ?: error("Protected profile is unavailable")).copy(
                name = profile.name.ifBlank { "Protected profile" }, isProtected = true, profilePolicy = profile.policy)
    }

    fun expiredHashes(context: Context): Set<String> {
        val prefs = context.getSharedPreferences(ProfilePolicyEnforcer.PREFS_NAME, Context.MODE_PRIVATE)
        val entries = prefs.all.filter { (key, value) ->
            key.startsWith("expiry_") && key.removePrefix("expiry_").matches(Regex("[0-9a-f]{64}")) &&
                value is Long && value > 0
        }
        if (entries.isEmpty()) return emptySet()
        val now = ProfilePolicyEnforcer.effectiveNow(context)
        return entries.filter { (_, value) -> value is Long && value <= now }
            .keys.map { it.removePrefix("expiry_") }.filter { it.matches(Regex("[0-9a-f]{64}")) }.toSet()
    }

    /** No timer at all when there are no expiring profiles. Recheck clock changes at most once a minute. */
    fun nextExpiryDelayMs(context: Context): Long? {
        val nearest = context.getSharedPreferences(ProfilePolicyEnforcer.PREFS_NAME, Context.MODE_PRIVATE)
            .all.filter { (key, value) -> key.startsWith("expiry_") &&
                key.removePrefix("expiry_").matches(Regex("[0-9a-f]{64}")) && value is Long && value > 0 }
            .values.filterIsInstance<Long>().minOrNull() ?: return null
        return (nearest - ProfilePolicyEnforcer.effectiveNow(context)).coerceIn(1L, 60_000L)
    }

    fun deleteExpiredFiles(context: Context, hashes: Set<String>) {
        val prefs = context.getSharedPreferences(ProfilePolicyEnforcer.PREFS_NAME, Context.MODE_PRIVATE)
        val edit = prefs.edit()
        hashes.filter { it.matches(Regex("[0-9a-f]{64}")) }.forEach { hash ->
            File(File(context.noBackupFilesDir, "protected-profiles"), "$hash.lumen").delete()
            edit.remove("expiry_$hash")
        }
        edit.apply()
    }
}
