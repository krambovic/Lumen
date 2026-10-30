package com.lumen.ui.screens

private fun LumenStrings.refined(en: String, ru: String, fa: String, zh: String) = when (localeCode) {
    "ru" -> ru; "fa" -> fa; "zh" -> zh; else -> en
}
val LumenStrings.community get() = refined("Community", "Сообщество", "انجمن", "社区")
val LumenStrings.communitySummary get() = refined("GitHub · Support · Telegram", "GitHub · Поддержка · Telegram", "گیت‌هاب · حمایت · تلگرام", "GitHub · 支持 · Telegram")
val LumenStrings.settingsTileFull get() = refined("Full · 4 × 4", "Полная · 4 × 4", "کامل · ۴ × ۴", "完整 · 4 × 4")
val LumenStrings.serverLayout get() = refined("Server layout", "Вид серверов", "چیدمان سرورها", "服务器布局")
val LumenStrings.serverLayoutList get() = refined("List", "Список", "فهرست", "列表")
val LumenStrings.serverLayoutGrid get() = refined("Compact grid", "Компактная сетка", "شبکهٔ فشرده", "紧凑网格")
val LumenStrings.cryptoSupport get() = refined("Cryptocurrency", "Криптовалюта", "رمزارز", "加密货币")
val LumenStrings.walletAddress get() = refined("Wallet address", "Адрес кошелька", "آدرس کیف پول", "钱包地址")
