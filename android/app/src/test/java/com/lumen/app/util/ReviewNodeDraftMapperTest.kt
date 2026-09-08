package com.lumen.app.util

import com.lumen.core.database.model.NodeEntity
import com.lumen.ui.screens.NodeDraft
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Test

class ReviewNodeDraftMapperTest {
    @Test fun vlessCredentialsSurviveUriRoundTripWithoutTrimming() {
        val secret = " opaque+id:@/%Ж "
        val link = NodeDraftMapper.buildLink(NodeDraft(name = "test", protocol = "vless", server = "example.invalid", port = "443", secret = secret))
        val entity = NodeEntity(id = "review", name = "test", protocol = "vless", server = "example.invalid", port = 443, link = link)
        assertEquals(secret, NodeDraftMapper.draftFromEntity(entity)!!.secret)
    }

    @Test fun tlsAliasesPreserveSniAndResolveConflictsSecurely() {
        val entity = NodeEntity(id = "review", name = "test", protocol = "hysteria2", server = "example.invalid", port = 443,
            link = "hy2://token@example.invalid:443?server_name=tls.example.invalid&insecure=0&allow_insecure=1")
        val draft = NodeDraftMapper.draftFromEntity(entity)!!
        assertEquals("tls.example.invalid", draft.sni)
        assertFalse(draft.insecure)
    }
}
