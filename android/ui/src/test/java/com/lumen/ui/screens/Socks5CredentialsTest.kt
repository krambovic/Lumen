package com.lumen.ui.screens

import org.junit.Assert.*
import org.junit.Test

class Socks5CredentialsTest {
    @Test fun preservesSpacesAndUnicode() {
        assertTrue(isValidSocks5Credential(" username "))
        assertTrue(isValidSocks5Credential(" пароль 🚀 "))
        assertTrue(isValidSocks5Credential(" "))
    }
    @Test fun countsUtf8BytesRatherThanCharacters() {
        assertTrue(isValidSocks5Credential("a".repeat(255)))
        assertFalse(isValidSocks5Credential("a".repeat(256)))
        assertTrue(isValidSocks5Credential("я".repeat(127)))
        assertFalse(isValidSocks5Credential("я".repeat(128)))
    }
    @Test fun rejectsEmptyAndControlCharacters() {
        assertFalse(isValidSocks5Credential(""))
        assertFalse(isValidSocks5Credential("user\nname"))
        assertFalse(isValidSocks5Credential("secret" + 0.toChar()))
    }
}
