package com.lumen.ui.screens

/** Shared labels for recently added controls, following the app's selected language. */
enum class InterfaceText(private val en: String, private val ru: String, private val fa: String, private val zh: String) {
    DOCK_SIZE("Dock size", "Размер дока", "اندازه نوار پیمایش", "导航栏大小"),
    SMALL("Small", "Маленький", "کوچک", "小"),
    DEFAULT("Default", "Обычный", "پیش‌فرض", "默认"),
    LARGE("Large", "Большой", "بزرگ", "大"),
    HYBRID("Hybrid", "Гибридный", "ترکیبی", "混合"),
    PROTECTED("Protected", "Закрытый", "محافظت‌شده", "受保护"),
    WAIT_IMPORT("Wait for the import to finish", "Дождитесь завершения импорта", "منتظر پایان وارد کردن بمانید", "请等待导入完成"),
    RESET_FAILED("Reset failed", "Не удалось завершить сброс", "بازنشانی ناموفق بود", "重置失败"),
    HEADER_KEY("Header protection key", "Ключ защиты заголовка", "کلید حفاظت سربرگ", "头部保护密钥"),
    PADDING("Additional padding", "Дополнительное заполнение", "داده پوششی اضافی", "额外填充"),
    REKEY_AFTER("Rekey interval", "Интервал смены ключа", "فاصله تعویض کلید", "密钥更换间隔"),
    REKEY_TIMEOUT("Rekey timeout", "Тайм-аут смены ключа", "مهلت تعویض کلید", "密钥更换超时"),
    REJECT_AFTER("Key lifetime", "Срок действия ключа", "مدت اعتبار کلید", "密钥有效期"),
    KEEPALIVE_TIMEOUT("Keepalive timeout", "Тайм-аут поддержания связи", "مهلت حفظ اتصال", "保活超时"),
    HANDSHAKE_ATTEMPTS("Handshake attempts", "Попытки рукопожатия", "تعداد تلاش دست‌دهی", "握手尝试次数"),
    SETUP_HINT("Appearance, region and connection", "Оформление, регион и подключение", "ظاهر، منطقه و اتصال", "外观、地区和连接"),
    PROTECTED_PROFILES("Protected profiles", "Закрытые профили", "پروفایل‌های محافظت‌شده", "受保护配置"),
    RECIPIENT_HINT("Receive profiles encrypted for your device", "Получайте профили, зашифрованные для вашего устройства", "دریافت پروفایل‌های رمزگذاری‌شده برای دستگاه شما", "接收为此设备加密的配置"),
    REMOTES("Additional servers (host and port per line)", "Дополнительные серверы (адрес и порт в строке)", "سرورهای بیشتر (نشانی و پورت در هر خط)", "其他服务器（每行地址和端口）"),
    ENCRYPTION("Encryption", "Шифрование", "رمزنگاری", "加密"),
    CIPHER("Cipher", "Шифр", "الگوریتم رمزنگاری", "加密算法"),
    DIGEST("Authentication digest", "Хеш аутентификации", "هش احراز هویت", "认证摘要"),
    CREDENTIALS("Credentials", "Данные входа", "اطلاعات ورود", "登录凭据"),
    CERTIFICATES("Certificates", "Сертификаты", "گواهی‌ها", "证书"),
    CA("CA certificate", "Сертификат CA", "گواهی CA", "CA 证书"),
    CLIENT_CERT("Client certificate", "Сертификат клиента", "گواهی کاربر", "客户端证书"),
    TLS_CRYPT("tls-crypt key", "Ключ tls-crypt", "کلید tls-crypt", "tls-crypt 密钥"),
    TLS_AUTH("tls-auth key", "Ключ tls-auth", "کلید tls-auth", "tls-auth 密钥"),
    KEY_DIRECTION("Key direction", "Направление ключа", "جهت کلید", "密钥方向"),
    TLS_SUITES("TLS cipher suites", "Наборы шифров TLS", "مجموعه رمزهای TLS", "TLS 密码套件"),
    X509_NAME("Certificate name", "Имя сертификата", "نام گواهی", "证书名称"),
    X509_MODE("Certificate name verification", "Проверка имени сертификата", "بررسی نام گواهی", "证书名称验证"),
    X509_EXACT("Exact name", "Точное имя", "نام دقیق", "精确名称"),
    X509_PREFIX("Name prefix", "Префикс имени", "پیشوند نام", "名称前缀"),
    X509_SUFFIX("Name suffix", "Суффикс имени", "پسوند نام", "名称后缀"),
    RECONNECT("Reconnect, s", "Повтор, с", "اتصال مجدد، ثانیه", "重连，秒"),
    PING_INTERVAL("Ping, s", "Пинг, с", "پینگ، ثانیه", "探测，秒"),
    PING_RESTART("Ping restart, s", "Перезапуск, с", "راه‌اندازی مجدد، ثانیه", "重启，秒"),
    DNS_SERVERS("DNS servers (one per line)", "DNS-серверы (по одному в строке)", "سرورهای DNS (هر کدام در یک خط)", "DNS 服务器（每行一个）"),
    DRACULA("Dark violet tones", "Тёмные фиолетовые тона", "رنگ‌های بنفش تیره", "深紫色调"),
    CATPPUCCIN("Soft pastel accents", "Мягкие пастельные акценты", "رنگ‌های پاستلی ملایم", "柔和粉彩强调色"),
    NORD("Cool arctic blue", "Холодный арктический синий", "آبی سرد قطبی", "冷调极地蓝"),
    GITHUB("Graphite and blue accents", "Графит и синие акценты", "گرافیتی با تأکید آبی", "石墨灰与蓝色强调色"),
    GRUVBOX("Warm retro contrast", "Тёплый ретро-контраст", "کنتراست گرم و کلاسیک", "暖调复古对比"),
    TOKYO("Neon night accents", "Ночные неоновые акценты", "رنگ‌های نئونی شب", "夜间霓虹强调色"),
    MONOKAI("Classic dark contrast", "Классический тёмный контраст", "کنتراست تیره کلاسیک", "经典深色对比"),
    MATERIAL("Calm deep tones", "Спокойные глубокие тона", "رنگ‌های عمیق و آرام", "沉静深色调"),
    SOLARIZED("Deep cyan tones", "Глубокие бирюзовые тона", "رنگ‌های فیروزه‌ای عمیق", "深青色调");

    fun inLanguage(language: String): String = when (normalizedLanguageCode(language)) {
        "ru" -> ru; "fa" -> fa; "zh" -> zh; else -> en
    }
}

fun NodeUiModel.localizedProtocol(strings: LumenStrings): String =
    if (isProtected) InterfaceText.PROTECTED.inLanguage(strings.localeCode) else displayProtocol
