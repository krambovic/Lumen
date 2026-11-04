package com.lumen.ui.screens

/** Translates the legacy import status messages at the presentation boundary. */
private enum class ImportText(val source: String, private val ru: String, private val fa: String, private val zh: String) {
    NOTHING("Nothing to import", "Нечего импортировать", "چیزی برای وارد کردن نیست", "没有可导入内容"),
    SUBSCRIPTION("Import subscription?", "Импортировать подписку?", "اشتراک وارد شود؟", "导入订阅？"),
    CONFIG("Import server config?", "Импортировать конфигурацию?", "پیکربندی سرور وارد شود؟", "导入服务器配置？"),
    SUBSCRIPTION_HELP("The link will be downloaded and added as a subscription.", "По ссылке будут загружены серверы и добавлена подписка.", "سرورها از پیوند دانلود و به‌عنوان اشتراک اضافه می‌شوند.", "将从链接下载内容并添加为订阅。"),
    CONFIG_HELP("Supported servers will be added to Default.", "Совместимые серверы будут добавлены в группу «Вручную».", "سرورهای سازگار به گروه دستی اضافه می‌شوند.", "支持的服务器将添加到手动分组。"),
    IMPORTING("Importing…", "Импортируем…", "در حال وارد کردن…", "正在导入…"),
    SUBSCRIPTION_DONE("Subscription imported", "Подписка импортирована", "اشتراک وارد شد", "订阅已导入"),
    DONE("Import complete", "Импорт завершён", "وارد کردن تمام شد", "导入完成"),
    FAILED("Import failed", "Не удалось импортировать", "وارد کردن ناموفق بود", "导入失败"),
    EMPTY("The file is empty", "Файл пуст", "فایل خالی است", "文件为空"),
    TOO_LARGE("File too large", "Файл слишком большой", "فایل بیش از حد بزرگ است", "文件过大"),
    PARSE_ERROR("File parse error", "Ошибка чтения конфигурации", "خطا در خواندن پیکربندی", "配置解析错误"),
    NO_SERVERS("No supported servers", "Нет совместимых серверов", "سرور سازگاری پیدا نشد", "没有支持的服务器"),
    FILE("Import from file?", "Импортировать из файла?", "از فایل وارد شود؟", "从文件导入？"),
    CLIPBOARD_EMPTY("Clipboard is empty", "Буфер обмена пуст", "حافظه موقت خالی است", "剪贴板为空"),
    CLIPBOARD_INVALID("Clipboard does not contain a subscription or supported config", "В буфере нет подписки или совместимой конфигурации", "حافظه موقت شامل اشتراک یا پیکربندی سازگار نیست", "剪贴板中没有订阅或支持的配置"),
    CLIPBOARD_LARGE("Import data is larger than 1 MiB", "Данные для импорта превышают 1 МиБ", "حجم داده واردشده بیش از ۱ مگابایت است", "导入数据超过 1 MiB"),
    PROFILE_LARGE("Profile exceeds the 8 MiB import limit", "Профиль превышает ограничение 8 МиБ", "حجم پروفایل بیش از محدودیت ۸ مگابایت است", "配置超过 8 MiB 导入限制"),
    SUBSCRIPTION_EMPTY("Subscription contains no supported servers", "В подписке нет совместимых серверов", "اشتراک شامل سرور سازگار نیست", "订阅中没有支持的服务器"),
    CONFIG_EMPTY("No supported server configs found", "Совместимые конфигурации не найдены", "پیکربندی سازگاری پیدا نشد", "未找到支持的配置"),
    FILE_EMPTY("No recognised server configs found in file", "В файле нет совместимых конфигураций", "در فایل پیکربندی سازگاری پیدا نشد", "文件中未找到支持的配置"),
    EXPIRED("Import request expired", "Запрос импорта устарел. Повторите импорт.", "درخواست وارد کردن منقضی شد. دوباره تلاش کنید.", "导入请求已过期，请重试。"),
    UNKNOWN("Unknown import error", "Неизвестная ошибка импорта", "خطای ناشناخته در وارد کردن", "未知导入错误"),
    UNKNOWN_ERROR("Unknown error", "Неизвестная ошибка", "خطای ناشناخته", "未知错误"),
    ADDED("{count} server(s) added", "Добавлено серверов: {count}", "سرورهای اضافه‌شده: {count}", "已添加 {count} 个服务器"),
    ADDED_TO("{count} server(s) added to {group}", "Добавлено серверов в {group}: {count}", "سرورهای اضافه‌شده به {group}: {count}", "已向 {group} 添加 {count} 个服务器"),
    FOUND("{count} server(s) found. They will be added to Default.", "Найдено серверов: {count}. Они будут добавлены в группу «Вручную».", "{count} سرور پیدا شد و به گروه دستی اضافه می‌شوند.", "找到 {count} 个服务器，将添加到手动分组。"),
    LIMIT("File exceeds the {limit} MiB import limit", "Файл превышает ограничение импорта {limit} МиБ", "حجم فایل بیش از محدودیت {limit} مگابایت است", "文件超过 {limit} MiB 导入限制"),
    INVALID_FILE("The file is empty, unreadable or exceeds the {limit} MiB import limit", "Файл пуст, недоступен или превышает ограничение {limit} МиБ", "فایل خالی، ناخوانا یا بزرگ‌تر از {limit} مگابایت است", "文件为空、无法读取或超过 {limit} MiB 限制"),
    HTTPS("{service} subscription URL must use HTTPS", "Ссылка подписки {service} должна использовать HTTPS", "پیوند اشتراک {service} باید از HTTPS استفاده کند", "{service} 订阅链接必须使用 HTTPS");

    fun localized(language: String): String = when (normalizedLanguageCode(language)) {
        "ru" -> ru; "fa" -> fa; "zh" -> zh; else -> source
    }
}

fun translatedImportText(value: String, strings: LumenStrings): String {
    val language = strings.localeCode
    ImportText.entries.firstOrNull { it.source == value }?.let { return it.localized(language) }
    val templates = listOf(
        Regex("^(\\d+) server\\(s\\) added$") to (ImportText.ADDED to listOf("count")),
        Regex("^(\\d+) server\\(s\\) added to (.+)$") to (ImportText.ADDED_TO to listOf("count", "group")),
        Regex("^(\\d+) server\\(s\\) found\\. They will be added to Default\\.$") to (ImportText.FOUND to listOf("count")),
        Regex("^File exceeds the (\\d+) MiB import limit$") to (ImportText.LIMIT to listOf("limit")),
        Regex("^The file is empty, unreadable or exceeds the (\\d+) MiB import limit$") to (ImportText.INVALID_FILE to listOf("limit")),
        Regex("^(Happ|Incy) subscription URL must use HTTPS$") to (ImportText.HTTPS to listOf("service"))
    )
    for ((pattern, template) in templates) {
        val match = pattern.matchEntire(value) ?: continue
        var result = template.first.localized(language)
        template.second.forEachIndexed { index, key -> result = result.replace("{$key}", match.groupValues[index + 1]) }
        return result
    }
    // Core diagnostics and provider-authored text retain their original detail.
    return value
}
