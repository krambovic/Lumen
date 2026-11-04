package com.lumen.core.config

import com.lumen.core.config.crypto.*
import java.security.KeyPairGenerator
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test

class LumenProfilePolicyTest {
    @Test fun expiresAtAnAbsoluteDeadlineRatherThanTwentyFourHoursAfterImport() {
        val created = 1_000_000L
        val policy = LumenProfilePolicy(created, created + 24 * 3_600_000L)
        policy.requireUsable(policy.expiresAtMs - 1)
        assertTrue(runCatching { policy.requireUsable(policy.expiresAtMs) }.isFailure)
        assertTrue(runCatching { policy.requireUsable(created - 300_001) }.isFailure)
        assertEquals(policy, LumenProfilePolicy.fromJson(policy.toJson()))
    }

    @Test fun connectionPoliciesDenyWrongTransportsAndMeteredWifi() {
        val policy = LumenProfilePolicy(wifiOnly = true, unmeteredOnly = true)
        assertTrue(runCatching { policy.requireUsable(1, wifi = false, unmetered = true) }.isFailure)
        assertTrue(runCatching { policy.requireUsable(1, wifi = true, unmetered = false) }.isFailure)
        policy.requireUsable(1, wifi = true, unmetered = true)
    }

    @Test fun encryptedRestrictionsCannotBeOverriddenOutsideThePayloadOrDowngradedToV2() {
        val key = KeyPairGenerator.getInstance("RSA").apply { initialize(2048) }.generateKeyPair()
        val policy = LumenProfilePolicy(1000, 1000 + 3_600_000, wifiOnly = true)
        val profile = LumenProfileEnvelope.Profile("Fixture", "socks://vpn.example:1080", policy = policy)
        val root = JSONObject(LumenProfileEnvelope.seal(profile, LumenProfileEnvelope.Delivery.RECIPIENTS, listOf(key.public)))
        assertEquals(3, root.getInt("version"))
        root.put("policy", LumenProfilePolicy().toJson())
        assertEquals(policy, LumenProfileEnvelope.open(root.toString(), key.private).policy)
        root.put("version", 2)
        assertTrue(runCatching { LumenProfileEnvelope.open(root.toString(), key.private) }.isFailure)
        assertTrue(runCatching { LumenProfileEnvelope.seal(profile.copy(locked = false), LumenProfileEnvelope.Delivery.ANYONE) }.isFailure)
    }
}
