package com.lumen.core.config

import com.lumen.core.config.parser.LinkParser
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.junit.runners.Parameterized

@RunWith(Parameterized::class)
class SubscriptionCorpusRegressionTest(private val label: String, private val fixture: JSONObject) {
    companion object {
        @JvmStatic @Parameterized.Parameters(name = "{0}")
        fun fixtures(): List<Array<Any>> {
            val text = SubscriptionCorpusRegressionTest::class.java.getResourceAsStream("/subscription_regressions_1912.json")!!
                .bufferedReader().use { it.readText() }
            val cases = JSONObject(text).getJSONArray("cases")
            return (0 until cases.length()).map { val item = cases.getJSONObject(it); arrayOf<Any>(item.getString("name"), item) }
        }
    }
    @Test fun supportedSubscriptionFormatMatchesSharedCorpus() {
        val (nodes, errors) = LinkParser.parseLinksText(fixture.getString("text"))
        assertEquals("$label: $errors", fixture.getInt("count"), nodes.size)
        assertEquals("$label: $errors", fixture.getBoolean("error"), errors.isNotEmpty())
        fixture.optJSONArray("names")?.let { expected ->
            assertEquals(label, (0 until expected.length()).map { expected.getString(it) }, nodes.map { it.name })
        }
        assertTrue(label, nodes.none { it.scheme == "unknown" || it.name.contains("\"network\":") })
    }
}
