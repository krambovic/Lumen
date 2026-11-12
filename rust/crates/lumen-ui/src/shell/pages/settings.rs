//! The seven sections and dialogs of `SettingsPage.qml`.

use std::path::Path;

use egui::{
    Button, Color32, ComboBox, Context, Frame, Id, Modal, RichText, ScrollArea, Stroke, TextEdit,
    Ui,
};
use lumen_core::{i18n::tr, models::AppSettings};

use super::PageContext;
use crate::shell::{
    theme::{Theme, hex_color},
    win_native,
};
use crate::view_models::AppBridge;

const TABS: [&str; 7] = [
    "Внешний вид",
    "Сеть",
    "Запуск и тесты",
    "Подписки",
    "DNS",
    "Обновления",
    "Данные",
];
const PALETTES: &[(&str, &str)] = &[
    ("default", "Default"),
    ("absolutely", "Absolutely"),
    ("ayu", "Ayu"),
    ("catppuccin", "Catppuccin"),
    ("codex", "Codex"),
    ("dracula", "Dracula"),
    ("everforest", "Everforest"),
    ("github", "GitHub"),
    ("gruvbox", "Gruvbox"),
    ("linear", "Linear"),
    ("lobster", "Lobster"),
    ("material", "Material"),
    ("matrix", "Matrix"),
    ("midnight", "Midnight"),
    ("monokai", "Monokai"),
    ("night-owl", "Night Owl"),
    ("nord", "Nord"),
    ("notion", "Notion"),
    ("one", "One"),
    ("oscurange", "Oscurange"),
    ("raycast", "Raycast"),
    ("rose-pine", "Rose Pine"),
    ("sentry", "Sentry"),
    ("solarized", "Solarized"),
    ("temple", "Temple"),
    ("tokyo-night", "Tokyo Night"),
    ("vercel", "Vercel"),
    ("vscode-plus", "VS Code Plus"),
    ("xcode", "Xcode"),
];
const DNS_TYPES: &[(&str, &str)] = &[
    ("udp", "udp"),
    ("tcp", "tcp"),
    ("tls", "tls"),
    ("https", "https"),
];
const DNS_STRATEGIES: &[(&str, &str)] = &[
    ("prefer_ipv4", "prefer_ipv4"),
    ("prefer_ipv6", "prefer_ipv6"),
    ("ipv4_only", "ipv4_only"),
    ("ipv6_only", "ipv6_only"),
];
const SPEED_URLS: &[(&str, &str)] = &[
    (
        "https://cachefly.cachefly.net/100mb.test",
        "Cachefly 100 MB",
    ),
    ("https://cachefly.cachefly.net/50mb.test", "Cachefly 50 MB"),
    ("https://cachefly.cachefly.net/10mb.test", "Cachefly 10 MB"),
    ("https://cachefly.cachefly.net/1mb.test", "Cachefly 1 MB"),
    (
        "https://speed.cloudflare.com/__down?bytes=104857600",
        "Cloudflare 100 MB",
    ),
    (
        "https://speed.cloudflare.com/__down?bytes=52428800",
        "Cloudflare 50 MB",
    ),
    (
        "https://speed.cloudflare.com/__down?bytes=10485760",
        "Cloudflare 10 MB",
    ),
];

#[derive(Default)]
pub(super) struct SettingsPage {
    tab: usize,
    subscription: Option<SubscriptionDraft>,
    dns: Option<DnsDraft>,
    reset: bool,
    color: Option<ColorDraft>,
    password: String,
    file_error: Option<String>,
}

struct SubscriptionDraft {
    include: String,
    exclude: String,
    agent: String,
    hwid: String,
    real_hwid: bool,
    converter: bool,
    converter_url: String,
}

struct DnsDraft {
    direct: String,
    proxy: String,
    hosts: String,
}
struct ColorDraft {
    base: Color32,
    hex: String,
    mute: f32,
    tint: bool,
}

impl SettingsPage {
    pub(super) fn render(&mut self, ui: &mut Ui, page: &mut PageContext<'_>) {
        let bridge = &mut *page.bridge;
        let theme = page.theme;
        let settings = bridge.settings().clone();
        let compact = bridge.compact_mode();
        ui.label(
            RichText::new(tr("Настройки", &[]))
                .size(theme.font_title as f32)
                .strong(),
        );
        ui.horizontal_wrapped(|ui| {
            for (index, key) in TABS.iter().enumerate() {
                let key = if index == 2 && compact {
                    "Запуск"
                } else {
                    key
                };
                ui.selectable_value(&mut self.tab, index, tr(key, &[]));
            }
        });
        if let Some(error) = &self.file_error {
            ui.colored_label(theme.danger, error);
        }
        ScrollArea::vertical()
            .id_salt(("settings", self.tab))
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = theme.spacing_large as f32;
                match self.tab {
                    0 => self.appearance(ui, theme, bridge, &settings, compact),
                    1 => network(ui, theme, bridge, &settings, compact),
                    2 => startup(ui, theme, bridge, &settings, compact),
                    3 => self.subscriptions(ui, theme, bridge, &settings, compact),
                    4 => self.dns(ui, theme, bridge, compact),
                    5 => updates(ui, theme, bridge, &settings),
                    _ => page.shell.open_configs_requested = self.data(ui, theme, bridge, compact),
                }
            });
        if self.reset {
            self.reset_dialog(page.ctx, bridge);
        }
        if let Some(color) = self.color.as_mut()
            && color_dialog(page.ctx, theme, bridge, color)
        {
            self.color = None;
        }
    }

    fn appearance(
        &mut self,
        ui: &mut Ui,
        theme: &Theme,
        bridge: &mut AppBridge,
        s: &AppSettings,
        compact: bool,
    ) {
        group(ui, theme, "Тема", |ui| {
            choose(
                ui,
                "Тема",
                &s.theme,
                &[
                    ("system", "Системная"),
                    ("light", "Светлая"),
                    ("dark", "Тёмная"),
                ],
                |value| bridge.set_theme(value),
            );
            choose(
                ui,
                "Палитра (пресет)",
                &s.ui_theme_preset,
                PALETTES,
                |value| bridge.set_ui_theme_preset(value),
            );
            let codes = bridge.available_languages();
            let labels = bridge.language_labels();
            let languages = codes
                .iter()
                .zip(&labels)
                .map(|(code, label)| (code.as_str(), label.as_str()))
                .collect::<Vec<_>>();
            choose(ui, "Язык", &s.language, &languages, |value| {
                bridge.set_language(value)
            });
            ui.horizontal_wrapped(|ui| {
                ui.label(tr("Цвет акцента", &[]));
                if ui
                    .add(Button::new(&s.accent_color).fill(theme.accent))
                    .clicked()
                {
                    self.color = Some(ColorDraft::new(&s.accent_color, 0.0, false));
                }
            });
            if !compact {
                ui.horizontal_wrapped(|ui| {
                    ui.label(tr("Свой базовый тон", &[]));
                    let label = if s.ui_base_tint.is_empty() {
                        "—"
                    } else {
                        &s.ui_base_tint
                    };
                    if ui
                        .add(
                            Button::new(label)
                                .fill(hex_color(&s.ui_base_tint).unwrap_or(theme.control_fill)),
                        )
                        .clicked()
                    {
                        let (base, mute) = s
                            .ui_base_tint_src
                            .split_once('|')
                            .map(|(hex, mute)| (hex, mute.parse().unwrap_or(0.0)))
                            .unwrap_or_else(|| {
                                if s.ui_base_tint.is_empty() {
                                    ("#5B6B8C", 0.6)
                                } else {
                                    (s.ui_base_tint.as_str(), 0.0)
                                }
                            });
                        self.color = Some(ColorDraft::new(base, mute, true));
                    }
                    if !s.ui_base_tint.is_empty() && button(ui, "Сбросить") {
                        bridge.set_ui_base_tint("");
                        bridge.set_ui_base_tint_src("");
                    }
                });
            }
        });
        group(ui, theme, "Обои", |ui| {
            let wallpaper = bridge.ui_wallpaper();
            ui.horizontal_wrapped(|ui| {
                ui.label(tr("Обои окна", &[]));
                if !wallpaper.is_empty() {
                    ui.label(
                        Path::new(&wallpaper)
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy(),
                    );
                }
                if button(ui, "Выбрать…") {
                    let filter = format!(
                        "{} (*.png *.jpg *.jpeg *.bmp *.webp)\0*.png;*.jpg;*.jpeg;*.bmp;*.webp\0",
                        tr("Изображения", &[])
                    );
                    self.file_error = pick(
                        &tr("Выберите изображение", &[]),
                        &filter,
                        |path| bridge.set_ui_wallpaper(&path.to_string_lossy()),
                    );
                }
                if !wallpaper.is_empty() && button(ui, "Сбросить") {
                    bridge.set_ui_wallpaper("");
                }
            });
            if !compact && !wallpaper.is_empty() {
                percent(
                    ui,
                    "Непрозрачность обоев",
                    s.ui_wallpaper_opacity,
                    |value| bridge.set_ui_wallpaper_opacity(value),
                );
                percent(
                    ui,
                    "Размытие обоев",
                    s.ui_wallpaper_blur,
                    |value| bridge.set_ui_wallpaper_blur(value),
                );
                percent(
                    ui,
                    "Яркость обоев",
                    s.ui_wallpaper_brightness,
                    |value| bridge.set_ui_wallpaper_brightness(value),
                );
            }
        });
        group(ui, theme, "Интерфейс", |ui| {
            choose(
                ui,
                "Плотность",
                &s.ui_density,
                &[
                    ("compact", "Компактная"),
                    ("comfortable", "Обычная"),
                    ("spacious", "Свободная"),
                ],
                |value| bridge.set_ui_density(value),
            );
            number(
                ui,
                "Масштаб интерфейса",
                s.ui_font_scale,
                80..=140,
                5.0,
                |value| bridge.set_ui_font_scale(value),
            );
            if !compact {
                number(
                    ui,
                    "Скругление углов",
                    s.ui_corner_radius,
                    0..=20,
                    1.0,
                    |value| bridge.set_ui_corner_radius(value),
                );
            }
            toggles(
                ui,
                bridge,
                &[("Анимации", s.ui_animations, AppBridge::set_ui_animations)],
            );
            toggle(ui, "Компактный режим", compact, |value| {
                bridge.set_interface_mode(if value { "compact" } else { "full" })
            });
        });
        group(ui, theme, "Прозрачность", |ui| {
            let active = s.ui_backdrop != "solid";
            ui.add_enabled_ui(bridge.ui_backdrop_available() || active, |ui| {
                toggle(ui, "Прозрачность", active, |value| {
                    bridge.set_ui_backdrop(if value { "mica" } else { "solid" })
                });
            });
            if !compact {
                ui.add_enabled_ui(active, |ui| {
                    percent(
                        ui,
                        "Сила прозрачности",
                        s.ui_transparency_strength,
                        |value| bridge.set_ui_transparency_strength(value),
                    )
                });
            }
        });
    }

    fn subscriptions(
        &mut self,
        ui: &mut Ui,
        theme: &Theme,
        bridge: &mut AppBridge,
        s: &AppSettings,
        compact: bool,
    ) {
        group(ui, theme, "Подписки", |ui| {
            choose(
                ui,
                "Автообновление подписок",
                &s.subscription_auto_update_minutes.to_string(),
                &[
                    ("0", "Выкл"),
                    ("30", "30 мин"),
                    ("60", "1 час"),
                    ("240", "4 часа"),
                    ("720", "12 часов"),
                    ("1440", "24 часа"),
                ],
                |value| bridge.set_subscription_auto_update_minutes(value.parse().unwrap_or(0)),
            );
            toggle(
                ui,
                "Загружать подписки через прокси/TUN",
                s.subscription_use_proxy_tun,
                |value| bridge.set_subscription_use_proxy_tun(value),
            );
        });
        if compact {
            return;
        }
        let draft = self.subscription.get_or_insert_with(|| SubscriptionDraft {
            include: s.subscription_include_regex.clone(),
            exclude: s.subscription_exclude_regex.clone(),
            agent: s.subscription_user_agent.clone(),
            hwid: s.subscription_hwid.clone(),
            real_hwid: s.subscription_use_real_hwid,
            converter: s.subscription_converter_enabled,
            converter_url: s.subscription_converter_url.clone(),
        });
        group(ui, theme, "Фильтры regex", |ui| {
            ui.label(tr("Каждая строка — отдельное выражение. Фильтр применяется к имени, адресу, протоколу и исходной ссылке сервера.", &[]));
            area(ui, "Включать", &mut draft.include, "NL|DE|Reality");
            area(ui, "Исключать", &mut draft.exclude, "expired|traffic limit");
        });
        group(ui, theme, "Параметры запроса", |ui| {
            field(
                ui,
                "User-Agent",
                &mut draft.agent,
                "Happ/2.18.3/Windows/2606241603601",
                false,
            );
            ui.checkbox(&mut draft.real_hwid, tr("Отправлять настоящий HWID", &[]));
            ui.add_enabled_ui(!draft.real_hwid, |ui| {
                field(
                    ui,
                    "HWID",
                    &mut draft.hwid,
                    "00000000-0000-4000-8000-000000000000",
                    false,
                )
            });
            ui.checkbox(&mut draft.converter, tr("Конвертация подписки", &[]));
            field(
                ui,
                "URL",
                &mut draft.converter_url,
                "https://converter.example/sub?url={url}",
                false,
            );
            if button(ui, "Сохранить настройки подписок") {
                bridge.set_subscription_user_agent(&draft.agent);
                bridge.set_subscription_include_regex(&draft.include);
                bridge.set_subscription_exclude_regex(&draft.exclude);
                bridge.set_subscription_use_real_hwid(draft.real_hwid);
                bridge.set_subscription_hwid(&draft.hwid);
                bridge.set_subscription_converter_enabled(draft.converter);
                bridge.set_subscription_converter_url(&draft.converter_url);
            }
        });
    }

    fn dns(&mut self, ui: &mut Ui, theme: &Theme, bridge: &mut AppBridge, compact: bool) {
        let mode = bridge.dns_mode();
        group(ui, theme, "DNS", |ui| {
            choose(
                ui,
                "Режим DNS",
                &mode,
                &[("system", "Системный"), ("builtin", "Встроенный")],
                |value| bridge.set_dns_mode(value),
            );
        });
        if compact {
            return;
        }
        let draft = self.dns.get_or_insert_with(|| DnsDraft {
            direct: bridge.dns_bootstrap_servers_text(),
            proxy: bridge.dns_proxy_servers_text(),
            hosts: bridge.dns_hosts_text(),
        });
        let enabled = mode == "builtin";
        ui.add_enabled_ui(enabled, |ui| {
            group(ui, theme, "DNS-серверы", |ui| {
                area(ui, "Прямые DNS-серверы", &mut draft.direct, "1.1.1.1");
                ui.label(tr("Необязательно; пусто — DNS только через прокси", &[]));
                area(
                    ui,
                    "DNS через прокси",
                    &mut draft.proxy,
                    "cloudflare-dns.com",
                );
                ui.label(tr("По одному адресу в строке", &[]));
                if !compact {
                    choose(
                        ui,
                        "Тип прямого DNS",
                        &bridge.dns_bootstrap_type(),
                        DNS_TYPES,
                        |value| bridge.set_bootstrap_dns("", value),
                    );
                    choose(
                        ui,
                        "Тип proxy DNS",
                        &bridge.dns_proxy_type(),
                        DNS_TYPES,
                        |value| bridge.set_proxy_dns("", value),
                    );
                    choose(
                        ui,
                        "Стратегия прямого DNS",
                        &bridge.dns_bootstrap_strategy(),
                        DNS_STRATEGIES,
                        |value| bridge.set_dns_bootstrap_strategy(value),
                    );
                    choose(
                        ui,
                        "Стратегия proxy DNS",
                        &bridge.dns_proxy_strategy(),
                        DNS_STRATEGIES,
                        |value| bridge.set_dns_proxy_strategy(value),
                    );
                    let flags: [Toggle; 5] = [
                        (
                            "Параллельные DNS-запросы",
                            bridge.dns_parallel_query(),
                            AppBridge::set_dns_parallel_query,
                        ),
                        (
                            "Оптимистичный кэш",
                            bridge.dns_optimistic_cache(),
                            AppBridge::set_dns_optimistic_cache,
                        ),
                        (
                            "Geo-проверка DNS",
                            bridge.dns_geo_check(),
                            AppBridge::set_dns_geo_check,
                        ),
                        (
                            "Перехватывать DNS в TUN",
                            bridge.dns_hijack_enabled(),
                            AppBridge::set_dns_hijack_enabled,
                        ),
                        (
                            "Fake DNS",
                            bridge.dns_fake_enabled(),
                            AppBridge::set_dns_fake_enabled,
                        ),
                    ];
                    toggles(ui, bridge, &flags);
                    area(ui, "DNS Hosts", &mut draft.hosts, "example.com=1.2.3.4");
                    ui.label(tr(
                        "Формат: domain=address. Несколько адресов разделяются запятыми.",
                        &[],
                    ));
                }
                if button(ui, "Применить DNS") {
                    bridge.apply_dns_settings(&draft.direct, &draft.proxy, &draft.hosts);
                }
            });
        });
    }

    fn data(&mut self, ui: &mut Ui, theme: &Theme, bridge: &mut AppBridge, compact: bool) -> bool {
        let mut configs = false;
        group(ui, theme, "Данные", |ui| {
            let loaded = bridge.profile_loaded();
            ui.label(tr("Пароль профиля", &[]));
            ui.label(tr(
                if loaded {
                    "Защитить настройки и серверы паролем"
                } else {
                    "Профиль не открыт. Введите пароль; изменения и подключение заблокированы."
                },
                &[],
            ));
            let edit = ui.add(
                TextEdit::singleline(&mut self.password)
                    .password(true)
                    .hint_text(tr("Пароль", &[])),
            );
            if button(
                ui,
                if loaded {
                    "Задать пароль"
                } else {
                    "Открыть профиль"
                },
            ) || (edit.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter)))
            {
                bridge.set_encryption_password(&self.password);
                self.password.clear();
            }
            ui.horizontal_wrapped(|ui| {
                ui.label(tr("Резервная копия", &[]));
                if button(ui, "Экспорт") {
                    self.file_error = file_result(
                        win_native::save_json_file(
                            &tr("Сохранить резервную копию", &[]),
                            "lumen-backup.json",
                        ),
                        |path| bridge.export_backup(&path, ""),
                    );
                }
                if button(ui, "Импорт") {
                    let filter = format!(
                        "{}\0*.json\0{}\0*.*\0",
                        tr("Резервная копия (*.json)", &[]),
                        tr("Все файлы (*)", &[])
                    );
                    self.file_error = pick(
                        &tr("Импорт резервной копии", &[]),
                        &filter,
                        |path| bridge.import_backup(&path, ""),
                    );
                    self.subscription = None;
                    self.dns = None;
                }
            });
            toggle(
                ui,
                "Телеметрия",
                bridge.diagnostics_upload_enabled(),
                |value| bridge.set_diagnostics_upload(value),
            );
            ui.horizontal_wrapped(|ui| {
                ui.label(tr("Конфиги", &[]));
                configs = button(ui, "Открыть");
            });
        });
        if !compact {
            group(ui, theme, "Пути к ядрам", |ui| {
                for (key, label, value) in [
                    ("xray", "Ядро Xray", bridge.xray_path_display()),
                    (
                        "singbox",
                        "Ядро sing-box-extended",
                        bridge.singbox_path_display(),
                    ),
                ] {
                    let mut next = committed_text(ui, label, &value, false);
                    if ui.push_id(key, |ui| button(ui, "Обзор")).inner {
                        self.file_error = pick(
                            &tr("Обзор", &[]),
                            "Executable files (*.exe)\0*.exe\0",
                            |path| next = Some(path.to_string_lossy().into_owned()),
                        );
                    }
                    if let Some(path) = next {
                        if key == "xray" {
                            bridge.set_xray_path(&path);
                        } else {
                            bridge.set_singbox_path(&path);
                        }
                    }
                }
            });
        }
        group(ui, theme, "Сброс настроек", |ui| {
            if button(ui, "Сбросить настройки") {
                self.reset = true;
            }
        });
        configs
    }

    fn reset_dialog(&mut self, ctx: &Context, bridge: &mut AppBridge) {
        let modal = Modal::new(Id::unique("settings-reset")).show(ctx, |ui| {
            ui.set_width(420.0_f32.min((ctx.content_rect().width() - 48.0).max(0.0)));
            ui.heading(tr("Сбросить настройки?", &[]));
            ui.label(tr("Настройки приложения и маршрутизации вернутся к значениям по умолчанию. Серверы и подписки не будут удалены.", &[]));
            ui.horizontal_wrapped(|ui| (button(ui, "Отмена"), button(ui, "Сбросить"))).inner
        });
        let (cancel, confirm) = modal.inner;
        if confirm {
            bridge.reset_settings_to_defaults();
            self.subscription = None;
            self.dns = None;
        }
        if cancel || confirm || modal.should_close() {
            self.reset = false;
        }
    }
}

fn network(ui: &mut Ui, theme: &Theme, bridge: &mut AppBridge, s: &AppSettings, compact: bool) {
    group(ui, theme, "Сеть", |ui| {
        let region = bridge.regional_preset();
        ui.add_enabled_ui(!bridge.regional_preset_busy(), |ui| {
            choose(
                ui,
                "Региональный профиль",
                &region,
                &[("russia", "Россия"), ("china", "Китай"), ("iran", "Иран")],
                |value| bridge.set_regional_preset(value),
            );
        });
        if bridge.regional_preset_busy() {
            ui.spinner();
        }
        choose(
            ui,
            "Основное ядро",
            &s.core_preference,
            &[
                ("auto", "Auto"),
                ("xray", "Xray"),
                ("singbox", "sing-box extended"),
            ],
            |value| bridge.set_core_preference(value),
        );
        toggles(
            ui,
            bridge,
            &[
                (
                    "Прокси в обход локальной сети",
                    s.system_proxy_bypass_lan,
                    AppBridge::set_proxy_bypass_lan,
                ),
                (
                    "Настраивать прокси Firefox",
                    s.firefox_proxy_integration,
                    AppBridge::set_firefox_proxy_integration,
                ),
                (
                    "Переподключение при смене сети",
                    s.reconnect_on_network_change,
                    AppBridge::set_reconnect_on_network_change,
                ),
                (
                    "Предпочитать IPv6",
                    s.prefer_ipv6,
                    AppBridge::set_prefer_ipv6,
                ),
                ("Kill-switch", s.kill_switch, AppBridge::set_kill_switch),
            ],
        );
        if !compact {
            toggle(
                ui,
                "Фрагментирование",
                bridge.fragmentation_enabled(),
                |value| bridge.set_fragmentation(value),
            );
            if bridge.fragmentation_enabled() {
                let mut packets = bridge.fragment_packets();
                let mut length = bridge.fragment_length();
                let mut delay = bridge.fragment_delay();
                let a = committed_text(ui, "Пакеты", &packets, false);
                let b = committed_text(ui, "Размер фрагментации", &length, false);
                let c = committed_text(ui, "Интервал фрагментации", &delay, false);
                if a.is_some() || b.is_some() || c.is_some() {
                    if let Some(value) = a {
                        packets = value;
                    }
                    if let Some(value) = b {
                        length = value;
                    }
                    if let Some(value) = c {
                        delay = value;
                    }
                    bridge.set_fragment_settings(&packets, &length, &delay);
                }
            }
            toggle(
                ui,
                "Мультиплексирование",
                s.multiplex_enabled,
                |value| bridge.set_multiplexing(value),
            );
            if s.multiplex_enabled {
                number(
                    ui,
                    "Настройки мультиплексирования",
                    s.multiplex_concurrency,
                    1..=32,
                    1.0,
                    |value| bridge.set_multiplex_concurrency(value),
                );
            }
        }
    });
    group(ui, theme, "Цепочка прокси", |ui| {
        toggle(
            ui,
            "Использовать цепочку",
            s.proxy_chain_enabled,
            |value| bridge.set_proxy_chain_enabled(value),
        );
        ui.add_enabled_ui(s.proxy_chain_enabled, |ui| {
            let options = bridge.proxy_chain_node_options();
            let options = options
                .iter()
                .map(|(id, label)| (id.as_str(), label.as_str()))
                .collect::<Vec<_>>();
            choose(
                ui,
                "Предыдущий прокси",
                &s.proxy_chain_front_node_id,
                &options,
                |value| bridge.set_proxy_chain_front(value),
            );
            choose(
                ui,
                "Выходной прокси",
                &s.proxy_chain_exit_node_id,
                &options,
                |value| bridge.set_proxy_chain_exit(value),
            );
        });
        if s.proxy_chain_enabled {
            let preview = bridge.proxy_chain_preview();
            let hops = preview["hops"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|hop| hop["label"].as_str())
                .collect::<Vec<_>>();
            if hops.len() > 1 {
                ui.label(hops.join(" → "));
            }
            for note in preview["notes"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(serde_json::Value::as_str)
            {
                ui.colored_label(theme.warning, note);
            }
        }
    });
    if !compact {
        group(ui, theme, "Авто-переключение", |ui| {
            toggle(
                ui,
                "Включить авто-переключение",
                s.auto_switch_enabled,
                |value| bridge.set_auto_switch(value),
            );
            number(
                ui,
                "Порог скорости",
                s.auto_switch_threshold_kbps,
                1..=10000,
                1.0,
                |value| bridge.set_auto_switch_threshold(value),
            );
            number(
                ui,
                "Задержка",
                s.auto_switch_delay_sec,
                5..=300,
                1.0,
                |value| bridge.set_auto_switch_delay(value),
            );
            number(
                ui,
                "Пауза",
                s.auto_switch_cooldown_sec,
                10..=600,
                1.0,
                |value| bridge.set_auto_switch_cooldown(value),
            );
        });
        group(ui, theme, "Прокси", |ui| {
            number(
                ui,
                "SOCKS-порт",
                s.local_socks_port,
                1025..=65535,
                1.0,
                |value| bridge.set_local_socks_port(value),
            );
            number(
                ui,
                "HTTP-порт",
                s.local_http_port,
                1025..=65535,
                1.0,
                |value| bridge.set_local_http_port(value),
            );
            toggle(
                ui,
                "Авторизация прокси",
                s.proxy_auth_enabled,
                |value| {
                    bridge.set_proxy_auth_enabled(value);
                },
            );
            let user = committed_text(ui, "Логин прокси", &s.proxy_auth_username, false);
            let password = committed_text(ui, "Пароль прокси", &s.proxy_auth_password, true);
            if user.is_some() || password.is_some() {
                bridge.set_proxy_auth_credentials(
                    user.as_deref().unwrap_or(&s.proxy_auth_username),
                    password.as_deref().unwrap_or(&s.proxy_auth_password),
                );
            }
            toggles(
                ui,
                bridge,
                &[
                    (
                        "Разрешить подключения из локальной сети",
                        s.proxy_allow_lan,
                        AppBridge::set_proxy_allow_lan,
                    ),
                    (
                        "Sniffing только для маршрутизации",
                        s.sniff_route_only,
                        AppBridge::set_sniff_route_only,
                    ),
                ],
            );
        });
        group(ui, theme, "TUN", |ui| {
            toggle(ui, "Strict Route", s.tun_strict_route, |value| {
                bridge.set_tun_strict_route(value)
            });
            choose(
                ui,
                "Сетевой стек TUN",
                &s.tun_stack,
                &[
                    ("mixed", "mixed"),
                    ("system", "system"),
                    ("gvisor", "gvisor"),
                ],
                |value| bridge.set_tun_stack(value),
            );
            number(ui, "MTU", s.tun_mtu, 1280..=65535, 1.0, |value| {
                bridge.set_tun_mtu(value)
            });
            toggles(
                ui,
                bridge,
                &[
                    (
                        "Блокировать QUIC/HTTP3 в TUN",
                        s.tun_block_quic,
                        AppBridge::set_tun_block_quic,
                    ),
                    (
                        "Endpoint-Independent NAT",
                        s.tun_endpoint_independent_nat,
                        AppBridge::set_tun_endpoint_independent_nat,
                    ),
                ],
            );
        });
    }
}

fn startup(ui: &mut Ui, theme: &Theme, bridge: &mut AppBridge, s: &AppSettings, compact: bool) {
    group(ui, theme, "Запуск", |ui| {
        toggle(
            ui,
            "Запускать при входе",
            s.launch_on_startup,
            |value| bridge.set_launch_on_startup(value),
        );
        ui.add_enabled_ui(s.launch_on_startup, |ui| {
            toggle(
                ui,
                "Запускать автозапуск в трей",
                s.launch_in_tray_on_startup,
                |value| bridge.set_launch_in_tray_on_startup(value),
            )
        });
        toggles(
            ui,
            bridge,
            &[
                (
                    "Всегда запускать от администратора",
                    s.always_run_as_admin,
                    AppBridge::set_always_run_as_admin,
                ),
                (
                    "Автоподключение после запуска",
                    s.auto_connect_last,
                    AppBridge::set_auto_connect_last,
                ),
                (
                    "Восстанавливать VPN после сна",
                    s.reconnect_after_sleep,
                    AppBridge::set_reconnect_after_sleep,
                ),
                (
                    "Проверка VPN",
                    s.block_vpn_conflicts,
                    AppBridge::set_block_vpn_conflicts,
                ),
                (
                    "Автоподключение при импорте",
                    s.auto_connect_on_import,
                    AppBridge::set_auto_connect_on_import,
                ),
            ],
        );
        if !compact {
            toggle(
                ui,
                "Автозапуск Zapret",
                s.zapret_autostart,
                |value| bridge.set_zapret_autostart(value),
            );
        }
    });
    if !compact {
        group(
            ui,
            theme,
            "Тестирование серверов",
            |ui| {
                choose(
                    ui,
                    "Способ измерения ping",
                    &s.ping_method,
                    &[
                        ("tcping", "TCP ping"),
                        ("icmp", "ICMP"),
                        ("http", "HTTP GET"),
                        ("real", "Реальная задержка"),
                    ],
                    |value| bridge.set_ping_method(value),
                );
                if let Some(url) =
                    committed_text(ui, "URL теста скорости", &s.speed_test_url, false)
                {
                    bridge.set_speed_test_url(&url);
                }
                ui.menu_button(tr("Пресеты", &[]), |ui| {
                    for (url, label) in SPEED_URLS {
                        if ui.button(*label).clicked() {
                            bridge.set_speed_test_url(url);
                            ui.close();
                        }
                    }
                });
                number(
                    ui,
                    "Параллельных проверок",
                    s.speed_test_concurrency,
                    0..=32,
                    1.0,
                    |value| bridge.set_speed_test_concurrency(value),
                );
            },
        );
    }
}

fn updates(ui: &mut Ui, theme: &Theme, bridge: &mut AppBridge, s: &AppSettings) {
    group(ui, theme, "Обновления", |ui| {
        toggles(
            ui,
            bridge,
            &[
                (
                    "Проверять обновления",
                    s.check_updates,
                    AppBridge::set_check_updates,
                ),
                (
                    "Проверять обновления ядра и geoip/geosite",
                    s.resource_update_check,
                    AppBridge::set_resource_update_check,
                ),
                (
                    "Автообновление приложения",
                    s.app_auto_update,
                    AppBridge::set_app_auto_update,
                ),
            ],
        )
    });
}

impl ColorDraft {
    fn new(hex: &str, mute: f32, tint: bool) -> Self {
        let base = hex_color(hex).unwrap_or(Color32::from_rgb(0, 120, 212));
        Self {
            base,
            hex: color_hex(base),
            mute: mute.clamp(0.0, 1.0),
            tint,
        }
    }
}

fn color_dialog(
    ctx: &Context,
    theme: &Theme,
    bridge: &mut AppBridge,
    draft: &mut ColorDraft,
) -> bool {
    let modal = Modal::new(Id::unique("settings-color")).show(ctx, |ui| {
        ui.set_width(350.0_f32.min((ctx.content_rect().width() - 48.0).max(0.0)));
        ui.heading(tr(
            if draft.tint {
                "Свой базовый тон"
            } else {
                "Цвет акцента"
            },
            &[],
        ));
        ui.horizontal_wrapped(|ui| {
            if ui.color_edit_button_srgba(&mut draft.base).changed() {
                draft.hex = color_hex(draft.base);
            }
            if ui.text_edit_singleline(&mut draft.hex).changed()
                && let Some(color) = hex_color(&draft.hex)
            {
                draft.base = color;
            }
            if !draft.tint && button(ui, "Системный цвет") {
                let system = bridge.system_accent();
                if let Some(color) = hex_color(&system) {
                    draft.base = color;
                    draft.hex = color_hex(color);
                    draft.mute = 0.0;
                }
            }
        });
        ui.add(egui::Slider::new(&mut draft.mute, 0.0..=1.0).text(tr("Затемнение", &[])));
        let result = muted_color(draft.base, draft.mute);
        ui.colored_label(result, color_hex(result));
        ui.horizontal_wrapped(|ui| {
            let cancel = button(ui, "Отмена");
            let accept = ui
                .add(
                    Button::new(RichText::new(tr("Применить", &[])).color(theme.accent_text))
                        .fill(theme.accent),
                )
                .clicked();
            (cancel, accept, result)
        })
        .inner
    });
    let (cancel, accept, result) = modal.inner;
    if accept {
        if draft.tint {
            bridge.set_ui_base_tint(&color_hex(result));
            bridge.set_ui_base_tint_src(&format!("{}|{}", color_hex(draft.base), draft.mute));
        } else {
            bridge.set_accent(&color_hex(result));
        }
    }
    cancel || accept || modal.should_close()
}

fn muted_color(color: Color32, mute: f32) -> Color32 {
    let mut hsv = egui::ecolor::HsvaGamma::from(color);
    hsv.s *= 1.0 - 0.80 * mute;
    hsv.v *= 1.0 - 0.55 * mute;
    Color32::from(hsv)
}

fn color_hex(color: Color32) -> String {
    format!("#{:02X}{:02X}{:02X}", color.r(), color.g(), color.b())
}

fn group(ui: &mut Ui, theme: &Theme, title: &str, content: impl FnOnce(&mut Ui)) {
    Frame::new()
        .fill(theme.card)
        .stroke(Stroke::new(1.0, theme.border_solid))
        .corner_radius(theme.radius as u8)
        .inner_margin(14.0)
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 8.0;
            ui.label(
                RichText::new(tr(title, &[]))
                    .size(theme.font_strong as f32)
                    .strong(),
            );
            content(ui);
        });
}

type Toggle = (&'static str, bool, fn(&mut AppBridge, bool));

fn toggles(ui: &mut Ui, bridge: &mut AppBridge, items: &[Toggle]) {
    for &(label, value, set) in items {
        toggle(ui, label, value, |value| set(bridge, value));
    }
}
fn toggle(ui: &mut Ui, label: &str, mut value: bool, set: impl FnOnce(bool)) {
    if ui.checkbox(&mut value, tr(label, &[])).changed() {
        set(value);
    }
}
fn choose(
    ui: &mut Ui,
    label: &str,
    active: &str,
    options: &[(&str, &str)],
    set: impl FnOnce(&str),
) {
    ui.horizontal_wrapped(|ui| {
        ui.label(tr(label, &[]));
        let mut selected = active;
        let caption = options
            .iter()
            .find(|option| option.0 == active)
            .or_else(|| options.first())
            .map_or(active, |option| option.1);
        ComboBox::from_id_salt(label)
            .width(185.0)
            .selected_text(tr(caption, &[]))
            .truncate()
            .show_ui(ui, |ui| {
                for &(key, caption) in options {
                    if ui
                        .selectable_label(key == active, tr(caption, &[]))
                        .clicked()
                    {
                        selected = key;
                        ui.close();
                    }
                }
            });
        if selected != active {
            set(selected);
        }
    });
}
fn number(
    ui: &mut Ui,
    label: &str,
    mut value: i64,
    range: std::ops::RangeInclusive<i64>,
    step: f64,
    set: impl FnOnce(i64),
) {
    ui.horizontal_wrapped(|ui| {
        ui.label(tr(label, &[]));
        if ui
            .add(egui::DragValue::new(&mut value).range(range).speed(step))
            .changed()
        {
            set(value);
        }
    });
}
fn percent(ui: &mut Ui, label: &str, value: i64, set: impl FnOnce(i64)) {
    let id = ui.make_persistent_id(("setting-slider", label));
    let mut edited = ui
        .data_mut(|data| data.get_temp::<i64>(id))
        .unwrap_or(value);
    let response = ui.add(egui::Slider::new(&mut edited, 0..=100).text(tr(label, &[])));
    if response.dragged() {
        ui.data_mut(|data| data.insert_temp(id, edited));
    } else if response.changed() || response.drag_stopped() {
        ui.data_mut(|data| data.remove::<i64>(id));
        set(edited);
    }
}
fn field(ui: &mut Ui, label: &str, value: &mut String, hint: &str, password: bool) {
    ui.label(tr(label, &[]));
    ui.add(
        TextEdit::singleline(value)
            .desired_width(f32::INFINITY)
            .hint_text(hint)
            .password(password),
    );
}
fn area(ui: &mut Ui, label: &str, value: &mut String, hint: &str) {
    ui.label(tr(label, &[]));
    ScrollArea::vertical()
        .id_salt(label)
        .max_height(145.0)
        .show(ui, |ui| {
            ui.add(
                TextEdit::multiline(value)
                    .desired_width(f32::INFINITY)
                    .desired_rows(3)
                    .hint_text(hint),
            );
        });
}
fn committed_text(ui: &mut Ui, label: &str, current: &str, password: bool) -> Option<String> {
    ui.label(tr(label, &[]));
    let id = ui.make_persistent_id(("settings-text", label));
    let mut value = ui
        .data_mut(|data| data.get_temp::<String>(id))
        .unwrap_or_else(|| current.into());
    let response = ui.add(
        TextEdit::singleline(&mut value)
            .id(id)
            .password(password)
            .desired_width(f32::INFINITY),
    );
    if response.lost_focus() {
        ui.data_mut(|data| data.remove::<String>(id));
        (value != current).then_some(value)
    } else {
        if response.has_focus() {
            ui.data_mut(|data| data.insert_temp(id, value));
        }
        None
    }
}
fn button(ui: &mut Ui, key: &str) -> bool {
    ui.button(tr(key, &[])).clicked()
}
fn pick(title: &str, filter: &str, action: impl FnOnce(std::path::PathBuf)) -> Option<String> {
    file_result(win_native::pick_file(Path::new(""), title, filter), action)
}
fn file_result(
    result: anyhow::Result<Option<std::path::PathBuf>>,
    action: impl FnOnce(std::path::PathBuf),
) -> Option<String> {
    match result {
        Ok(Some(path)) => {
            action(path);
            None
        }
        Ok(None) => None,
        Err(error) => Some(error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_tint_darkening_keeps_source_color_and_reduces_value() {
        let draft = ColorDraft::new("#90A0BF", 0.6, true);
        assert_eq!(draft.hex, "#90A0BF");
        assert!(muted_color(draft.base, draft.mute).b() < draft.base.b());
        assert_eq!(muted_color(draft.base, 0.0), draft.base);
    }
}
