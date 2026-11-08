//! Curated "What's new" notes shown by the welcome flow after an update
//! (`release_notes.py`). Titles/texts are Russian source strings translated
//! via i18n keys in `locales/*.json`.

use serde_json::{json, Value};

/// Saved profiles written before Lumen tracked the welcome flow have no
/// `onboarding_version`; they are upgrades, never fresh installs.
pub const LEGACY_PROFILE_VERSION: &str = "0";

pub struct Note {
    pub glyph: &'static str,
    pub title: &'static str,
    pub text: &'static str,
}

pub struct VersionNotes {
    pub version: &'static str,
    pub items: &'static [Note],
}

pub static RELEASE_NOTES: &[VersionNotes] = &[
    VersionNotes {
        version: "2.1.4",
        items: &[
            Note {
                glyph: "\u{EC05}",
                title: "Кодировка подписок",
                text: "Исправлена кириллица в заголовках и баннерах подписок.",
            },
            Note {
                glyph: "\u{E943}",
                title: "Полезная диагностика",
                text: "В архив возвращены настройки; добавлены версии системы и ядер, проверка сети по выбору.",
            },
            Note {
                glyph: "\u{E895}",
                title: "Отправка отчётов",
                text: "Восстановлен ID установки и добавлена отправка через прокси при недоступности сервера напрямую.",
            },
        ],
    },
    VersionNotes {
        version: "2.1.3",
        items: &[
            Note {
                glyph: "\u{EC05}",
                title: "Подписки и автовыбор",
                text: "Импорт из буфера распознаёт подписки; исправлены группы автовыбора и переключение серверов.",
            },
            Note {
                glyph: "\u{E9D9}",
                title: "Задержка без хэндшейков",
                text: "HTTP GET по умолчанию; задержка на панели измеряется на прогретом соединении.",
            },
            Note {
                glyph: "\u{E895}",
                title: "Обновлённые компоненты",
                text: "sing-box extended 2.7.3 с исправлениями Lumen и droute 2.2.1 в комплекте и обновлениях.",
            },
            Note {
                glyph: "\u{E790}",
                title: "Стабильность и оформление",
                text: "Исправлены ложные конфликты ядер и подсказки; новая светлая иконка и обновления только для Desktop.",
            },
        ],
    },
    VersionNotes {
        version: "2.1.2",
        items: &[Note {
            glyph: "\u{E8A5}",
            title: "Смена ядра в TUN",
            text: "TUN снова передаёт данные после переключения между Xray и sing-box extended.",
        }],
    },
    VersionNotes {
        version: "2.1.1",
        items: &[
            Note {
                glyph: "\u{E943}",
                title: "Выбор основного ядра",
                text: "Auto, Xray или sing-box extended в Настройках → Сеть; на Xray работает и TUN.",
            },
            Note {
                glyph: "\u{E768}",
                title: "Быстрый запуск TUN",
                text: "TUN включается за секунду, а не за 10–20 секунд.",
            },
            Note {
                glyph: "\u{E823}",
                title: "Плавный интерфейс",
                text: "Без подвисаний при запуске и остановке Zapret и TUN, изменении размера окна и раскрытии боковой панели.",
            },
        ],
    },
    VersionNotes {
        version: "2.1.0",
        items: &[
            Note {
                glyph: "\u{EC05}",
                title: "Новая вкладка «Серверы»",
                text: "Выбор подписки и группы в одном месте, инспектор сервера и подписки, все действия — в меню по правому клику.",
            },
            Note {
                glyph: "\u{E80F}",
                title: "Новая панель управления",
                text: "Подключение, серверы, задержка и трафик приложений — на одном экране.",
            },
            Note {
                glyph: "\u{E790}",
                title: "Новые иконки",
                text: "Обновлены иконки Маршрутов, Zapret и Логов; Логи переехали вниз.",
            },
            Note {
                glyph: "\u{E943}",
                title: "Конфиги — в настройках",
                text: "Редактор конфигов теперь открывается кнопкой в Настройках → Данные.",
            },
        ],
    },
    VersionNotes {
        version: "2.0.0",
        items: &[
            Note {
                glyph: "\u{E753}",
                title: "Бесплатный WARP в один клик",
                text: "Lumen сам получает профили Cloudflare WARP — AmneziaWG и MASQUE — и добавляет их в группу «WARP». Без ботов и ручного копирования ключей.",
            },
            Note {
                glyph: "\u{E945}",
                title: "Zapret для отдельного сервера",
                text: "Обход DPI можно включить для конкретного сервера: стратегия применяется только к его адресу и не мешает остальному трафику.",
            },
            Note {
                glyph: "\u{E71B}",
                title: "Цепочки прокси",
                text: "Подключайтесь к серверу через промежуточный прокси или выпускайте трафик через дополнительный выходной узел.",
            },
            Note {
                glyph: "\u{E8BC}",
                title: "Быстрая настройка при первом запуске",
                text: "Новый мастер: язык, тема, регион, маршрутизация, серверы, обновления, автозапуск и диагностика — за пару минут. После обновлений Lumen коротко расскажет, что изменилось.",
            },
        ],
    },
];

pub const FALLBACK_NOTE: Note = Note {
    glyph: "\u{E895}",
    title: "Исправления и улучшения",
    text: "Lumen стал стабильнее и быстрее. Подробный список изменений — на странице релиза на GitHub.",
};

/// Numeric release key (major, minor, patch[, build]); empty for invalid values.
pub fn version_key(value: &str) -> Vec<i64> {
    let text = value.trim().strip_prefix('v').unwrap_or(value.trim());
    let text = text.split(['-', '+']).next().unwrap_or(text);
    let parts: Vec<&str> = text.split('.').collect();
    if parts.is_empty() || parts.len() > 4 || parts.iter().any(|p| p.is_empty() || !p.chars().all(|c| c.is_ascii_digit())) {
        return Vec::new();
    }
    let mut parsed: Vec<i64> = parts.iter().filter_map(|p| p.parse::<i64>().ok()).collect();
    if parsed.len() != parts.len() {
        return Vec::new();
    }
    while parsed.len() < 3 {
        parsed.push(0);
    }
    parsed
}

pub fn normalize_seen_version(value: &serde_json::Value) -> String {
    let text = match value {
        serde_json::Value::String(s) => s.trim().trim_start_matches('v').to_string(),
        serde_json::Value::Number(n) => n.to_string(),
        _ => String::new(),
    };
    if text.is_empty() {
        return String::new();
    }
    if version_key(&text).is_empty() {
        LEGACY_PROFILE_VERSION.into()
    } else {
        text
    }
}

pub fn is_upgrade(seen_version: &str, app_version: &str) -> bool {
    let seen = seen_version.trim();
    if seen.is_empty() {
        return false; // fresh install: nothing to compare with
    }
    let current = version_key(app_version);
    let previous = version_key(seen);
    if current.is_empty() {
        return false;
    }
    let seen_base = seen.split('+').next().unwrap_or(seen);
    let app_base = app_version.split('+').next().unwrap_or(app_version);
    let promoted_to_stable =
        previous == current && seen_base.contains('-') && !app_base.contains('-');
    previous.is_empty() || previous < current || promoted_to_stable
}

/// Release groups newer than `seen_version` up to `app_version` (newest first).
pub fn notes_between(seen_version: &str, app_version: &str, limit: usize) -> Vec<Value> {
    let current_text = app_version.trim().trim_start_matches('v').to_string();
    let current = version_key(&current_text);
    let previous = {
        let k = version_key(seen_version);
        if k.is_empty() {
            vec![0, 0, 0]
        } else {
            k
        }
    };
    let mut versions: Vec<&VersionNotes> = RELEASE_NOTES.iter().collect();
    versions.sort_by(|a, b| version_key(b.version).cmp(&version_key(a.version)));
    let mut groups: Vec<Value> = Vec::new();
    for notes in versions {
        let key = version_key(notes.version);
        if !key.is_empty()
            && key <= current
            && (previous < key || is_upgrade(seen_version, notes.version))
        {
            groups.push(json!({
                "version": notes.version,
                "items": notes.items.iter().map(|n| json!({
                    "glyph": n.glyph,
                    "title": n.title,
                    "text": n.text,
                })).collect::<Vec<_>>(),
            }));
        }
    }
    if !current.is_empty() && (groups.is_empty() || version_key(groups[0]["version"].as_str().unwrap_or("")) != current) {
        groups.insert(
            0,
            json!({
                "version": current_text,
                "items": [{
                    "glyph": FALLBACK_NOTE.glyph,
                    "title": FALLBACK_NOTE.title,
                    "text": FALLBACK_NOTE.text,
                }],
            }),
        );
    }
    groups.truncate(limit.max(1));
    groups
}

pub fn legacy_profile_version() -> String {
    LEGACY_PROFILE_VERSION.into()
}
