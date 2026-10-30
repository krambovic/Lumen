package com.lumen.core.config

/** Diagnostic lookups must follow the selected proxy, including split-route presets. */
object NetworkInfoEndpoints {
    val urls = listOf("https://api.ipquery.io/?format=json", "https://ipwho.is/")
    val hosts = listOf("api.ipquery.io", "ipwho.is")
}
