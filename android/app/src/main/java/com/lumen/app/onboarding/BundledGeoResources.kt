package com.lumen.app.onboarding

import java.io.File
import java.io.InputStream

/** Only restore the selected region. Presence on disk never enables a route. */
internal fun bundledGeoResourceNames(region: String): List<String> = buildList {
    add("geosite-category-ads-all.srs")
    when (region) {
        "ru" -> addAll(listOf("geosite-category-ru.srs", "geoip-ru.srs"))
        "ir" -> addAll(listOf("geosite-ir.srs", "geoip-ir.srs"))
    }
}

/** Installs offline defaults without replacing lists already updated by the user. */
internal fun restoreBundledGeoResources(
    region: String,
    directory: File,
    openAsset: (String) -> InputStream,
    replace: (File, File) -> Unit
) {
    directory.mkdirs()
    for (name in bundledGeoResourceNames(region)) {
        val target = File(directory, name)
        if (target.isFile && target.length() > 0) continue
        val temporary = File.createTempFile("bundled-", ".srs", directory)
        try {
            openAsset("georesources/$name").use { input ->
                temporary.outputStream().use { output -> input.copyTo(output) }
            }
            check(temporary.length() > 0) { "Bundled geo resource is empty: $name" }
            replace(temporary, target)
        } finally {
            temporary.delete()
        }
    }
}
