package com.lumen.ui.screens

enum class NetworkInfoText(private val en: String, private val ru: String, private val fa: String, private val zh: String) {
    TITLE("Network", "Сеть", "شبکه", "网络"),
    EXIT("VPN exit", "Выход VPN", "خروجی VPN", "VPN 出口"),
    CURRENT("Current network", "Текущая сеть", "شبکه فعلی", "当前网络"),
    PUBLIC_IP("PUBLIC IP", "ВНЕШНИЙ IP", "IP عمومی", "公网 IP"),
    LOOKING("Checking your connection…", "Проверяем подключение…", "در حال بررسی اتصال…", "正在检查连接…"),
    WAITING("Waiting for the VPN…", "Ожидаем подключения VPN…", "در انتظار اتصال VPN…", "等待 VPN 连接…"),
    FAILED("Could not check this connection. Try again.", "Не удалось проверить подключение. Попробуйте ещё раз.", "بررسی اتصال ممکن نشد. دوباره تلاش کنید.", "无法检查此连接，请重试。"),
    COUNTRY("Country", "Страна", "کشور", "国家"),
    REGION("Region", "Регион", "استان", "地区"),
    CITY("City", "Город", "شهر", "城市"),
    POSTAL("Postal code", "Почтовый индекс", "کد پستی", "邮编"),
    LOCATION("Location", "Расположение", "موقعیت", "位置"),
    TIMEZONE("Time zone", "Часовой пояс", "منطقه زمانی", "时区"),
    PROVIDER("Provider", "Провайдер", "ارائه‌دهنده", "运营商"),
    ORGANIZATION("Organization", "Организация", "سازمان", "组织"),
    MAP("Open map", "На карте", "نمایش روی نقشه", "查看地图"),
    COPY("Copy IP", "Скопировать IP", "کپی IP", "复制 IP"),
    CLASSIFICATION("IP classification", "Тип сети", "نوع شبکه", "IP 分类"),
    CLASSIFICATION_HINT("Provider database assessment. Not a test of VPN protection.", "Оценка базы провайдера, а не проверка защиты VPN.", "ارزیابی پایگاه داده؛ نه آزمایش حفاظت VPN.", "服务商数据库评估，并非 VPN 防护测试。"),
    DATACENTER("Data center", "Дата-центр", "مرکز داده", "数据中心"),
    MOBILE("Mobile", "Мобильная сеть", "شبکه همراه", "移动网络"),
    VPN("VPN", "VPN", "VPN", "VPN"),
    PROXY("Proxy", "Прокси", "پروکسی", "代理"),
    TOR("Tor", "Tor", "Tor", "Tor"),
    YES("Yes", "Да", "بله", "是"),
    NO("No", "Нет", "خیر", "否"),
    UNKNOWN("Not available", "Нет данных", "نامشخص", "暂无数据"),
    APPROXIMATE("IP location is approximate.", "Расположение по IP приблизительное.", "موقعیت IP تقریبی است.", "IP 位置仅为近似值。"),
    UPDATED("Checked", "Проверено", "بررسی‌شده", "检查时间"),
    SOURCE("Data source", "Источник данных", "منبع داده", "数据来源"),
    DETAILS("Network details", "Данные сети", "جزئیات شبکه", "网络详情");

    fun inLanguage(language: String): String = when (language.lowercase().substringBefore('-')) {
        "ru" -> ru; "fa" -> fa; "zh" -> zh; else -> en
    }
}
