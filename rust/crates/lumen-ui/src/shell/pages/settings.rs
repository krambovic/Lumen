//! The seven sections and dialogs of `SettingsPage.qml`.
//!
//! Mirrors the QML structure: an icon tab bar with an accent underline, then
//! cards of `SettingRow`s — glyph + title/subtitle on the left, controls
//! pinned to the right.

#[allow(unused_imports)]
use crate::shell::wheel::GlideRoom;
use crate::shell::color_picker::{ColorPicker, PickerResult};
use crate::shell::widgets::FluentTip;
use std::path::Path;

use egui::{
    Align, Color32, Context, FontId, Id, Layout, RichText,
    ScrollArea, Sense, Stroke, TextEdit, TextFormat, Ui, text::LayoutJob, vec2,
};
use lumen_core::{constants::APP_VERSION, i18n::tr, models::AppSettings};

use super::{PageContext, fluent_button};
use crate::shell::{
    motion,
    theme::{Theme, hex_color, semibold},
    widgets, win_native,
};
use crate::view_models::AppBridge;

/// QML `tabModel` — glyphs with the labels shown from a 1220px page width.
const TABS: [(&str, &str); 7] = [
    ("\u{e790}", "Внешний вид"),
    ("\u{e774}", "Сеть"),
    ("\u{e7e8}", "Запуск и тесты"),
    ("\u{e8c8}", "Подписки"),
    ("\u{e774}", "DNS"),
    ("\u{e895}", "Обновления"),
    ("\u{e74e}", "Данные"),
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
    ("https://cachefly.cachefly.net/100mb.test", "Cachefly 100 MB"),
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
    /// `displayedTab`: lags `tab` while the exit animation runs.
    displayed: usize,
    transition: TabTransition,
    subscription: Option<SubscriptionDraft>,
    dns: Option<DnsDraft>,
    reset: bool,
    /// `ColorPickerDialog` and whether it edits the base tint (else the accent).
    color: Option<(ColorPicker, bool)>,
    password: String,
    file_error: Option<String>,
}

/// `tabExit` (70/90 ms, InCubic) followed by `tabEnter` (120 ms OutCubic and
/// 220 ms OutQuint) of SettingsPage.qml, with `activateTab`'s rules: a click
/// during the exit only changes the destination, a click during the enter
/// starts a new exit from the current opacity and offset, and a click on the
/// tab that is still displayed cancels the exit.
#[derive(Default)]
struct TabTransition {
    initialized: bool,
    /// `currentTab` as of the previous frame; a change is one `activateTab`.
    target: usize,
    /// `tabTransitionDirection`.
    direction: f32,
    phase: TabPhase,
    opacity: f32,
    x: f32,
}

#[derive(Default, Clone, Copy)]
enum TabPhase {
    #[default]
    Idle,
    Exit { start: f64, opacity: f32, x: f32, to_x: f32 },
    Enter { start: f64, from_x: f32 },
}

impl TabTransition {
    /// `tabTransitionOpacity`.
    const DIM: f32 = 0.86;
    const EXIT: f32 = 0.09;
    const ENTER: f32 = 0.22;

    fn running(&self) -> bool {
        !matches!(self.phase, TabPhase::Idle)
    }

    /// `resetTabTransition`.
    fn reset(&mut self) {
        self.phase = TabPhase::Idle;
        self.opacity = 1.0;
        self.x = 0.0;
    }

    fn advance(
        &mut self,
        now: f64,
        tab: usize,
        displayed: &mut usize,
        animated: bool,
        width: f32,
    ) -> (f32, f32) {
        // `tabEnterDistance` / `tabExitDistance`.
        let enter = (width * 0.035).clamp(24.0, 40.0).round();
        let exit = (enter * 0.35).round();
        if !self.initialized {
            self.initialized = true;
            self.target = tab;
            self.direction = 1.0;
            self.reset();
        }
        if tab != self.target {
            self.target = tab;
            self.direction = if tab >= *displayed { 1.0 } else { -1.0 };
            let exiting = matches!(self.phase, TabPhase::Exit { .. });
            if *displayed == tab {
                if exiting {
                    self.reset();
                }
            } else if !exiting && animated {
                self.phase = TabPhase::Exit {
                    start: now,
                    opacity: self.opacity,
                    x: self.x,
                    to_x: -self.direction * exit,
                };
            }
        }
        if !animated {
            self.reset();
            *displayed = self.target;
            return (1.0, 0.0);
        }
        if let TabPhase::Exit { start, opacity, x, to_x } = self.phase {
            let elapsed = (now - start) as f32;
            if elapsed < Self::EXIT {
                let fade = motion::ease_in_cubic(elapsed / 0.07);
                let slide = motion::ease_in_cubic(elapsed / Self::EXIT);
                self.opacity = opacity + (Self::DIM - opacity) * fade;
                self.x = x + (to_x - x) * slide;
            } else {
                // `tabExit.onFinished`: only now does the page change.
                *displayed = self.target;
                self.phase = TabPhase::Enter {
                    start: start + f64::from(Self::EXIT),
                    from_x: self.direction * enter,
                };
            }
        }
        if let TabPhase::Enter { start, from_x } = self.phase {
            let elapsed = (now - start) as f32;
            if elapsed >= Self::ENTER {
                self.reset();
            } else {
                self.opacity = Self::DIM + (1.0 - Self::DIM) * motion::ease_out_cubic(elapsed / 0.12);
                self.x = from_x * (1.0 - motion::ease_out_quint(elapsed / Self::ENTER));
            }
        }
        (self.opacity, self.x)
    }
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

impl SettingsPage {
    pub(super) fn render(&mut self, ui: &mut Ui, page: &mut PageContext<'_>) {
        let bridge = &mut *page.bridge;
        let theme = page.theme;
        let settings = bridge.settings().clone();
        let compact = bridge.compact_mode();
        // Root ColumnLayout: 20 px above, 12 px right, 8 px below, `spacingLarge` gaps.
        let mut column = ui.available_rect_before_wrap();
        column.min.y += 20.0;
        column.max.x -= 12.0;
        column.max.y -= 8.0;
        let mut column_ui = ui.new_child(egui::UiBuilder::new().max_rect(column));
        let ui = &mut column_ui;
        ui.spacing_mut().item_spacing.y = theme.spacing_large as f32;
        let title = widgets::galley(
            ui,
            &tr("Настройки", &[]),
            semibold(theme.font_title as f32),
            theme.text,
        );
        let (title_rect, _) = ui.allocate_exact_size(title.size(), Sense::hover());
        ui.painter().galley(title_rect.min, title, theme.text);
        self.tab_bar(ui, theme, compact);
        if let Some(error) = &self.file_error {
            ui.colored_label(theme.danger, error);
        }
        let (opacity, offset) = self.tab_transition(page.ctx, theme, ui.available_width());
        let viewport = ui.available_rect_before_wrap();
        let shown = self.displayed;
        let mut stack =
            ui.new_child(egui::UiBuilder::new().max_rect(viewport.translate(vec2(offset, 0.0))));
        stack.set_clip_rect(viewport.intersect(ui.clip_rect()));
        stack.multiply_opacity(opacity);
        let ui = &mut stack;
        ScrollArea::vertical()
            .id_salt(("settings", shown))
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = theme.spacing_large as f32;
                match shown {
                    0 => self.appearance(ui, theme, bridge, &settings, compact),
                    1 => network(ui, theme, bridge, &settings, compact),
                    2 => startup(ui, theme, bridge, &settings, compact),
                    3 => self.subscriptions(ui, theme, bridge, &settings, compact),
                    4 => self.dns(ui, theme, bridge, compact),
                    5 => updates(ui, theme, bridge, &settings),
                    _ => page.shell.open_configs_requested = self.data(ui, theme, bridge, compact),
                }
            }).glide_room(ui);
        if self.reset {
            self.reset_dialog(page.ctx, theme, bridge);
        }
        if let Some((picker, tint)) = self.color.as_mut() {
            match picker.show(page.ctx, theme, || bridge.system_accent()) {
                PickerResult::Open => {}
                PickerResult::Closed => self.color = None,
                PickerResult::Accepted { hex, base_hex, mute } => {
                    if *tint {
                        bridge.set_ui_base_tint(&hex);
                        bridge.set_ui_base_tint_src(&format!("{base_hex}|{mute}"));
                    } else {
                        bridge.set_accent(&hex);
                    }
                    self.color = None;
                }
            }
        }
    }

    /// Stack opacity and horizontal offset of the tab transition; switches
    /// `displayed` when the exit animation ends.
    fn tab_transition(&mut self, ctx: &Context, theme: &Theme, width: f32) -> (f32, f32) {
        let now = ctx.input(|input| input.time);
        let result = self.transition.advance(
            now,
            self.tab,
            &mut self.displayed,
            theme.anim_fast > 0,
            width,
        );
        if self.transition.running() {
            ctx.request_repaint();
        }
        result
    }

    /// Icon tab strip with the half-width accent underline (`tabRow` +
    /// `tabIndicator` in SettingsPage.qml).
    fn tab_bar(&mut self, ui: &mut Ui, theme: &Theme, compact: bool) {
        let scale = if ui.available_width() < 760.0 { 0.82 } else { 1.0 };
        let height = (40.0 * theme.font_scale as f32).max(32.0);
        let font_size = (theme.font_normal as f32 * scale).round();
        let show_labels = ui.available_width() >= 1220.0;
        let tab_width =
            (ui.available_width() - 4.0 * (TABS.len() - 1) as f32) / TABS.len() as f32;
        ui.allocate_ui_with_layout(
            vec2(ui.available_width(), height),
            Layout::left_to_right(Align::Center),
            |ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                for (index, (glyph, label)) in TABS.iter().enumerate() {
                    let label = if index == 2 && compact {
                        tr("Запуск", &[])
                    } else {
                        tr(label, &[])
                    };
                    let (rect, response) =
                        ui.allocate_exact_size(vec2(tab_width, height), Sense::click());
                    let current = self.tab == index;
                    let highlight = motion::toggle(
                        ui.ctx(),
                        theme,
                        Id::unique(("settings-tab-fill", index)),
                        current || response.hovered(),
                        140.0,
                        motion::linear,
                    );
                    let fill = if current {
                        theme.accent_soft
                    } else {
                        theme.nav_hover
                    };
                    ui.painter().rect_filled(
                        rect,
                        theme.radius_small as u8,
                        fill.gamma_multiply(highlight),
                    );
                    // Painted directly so the click response keeps the hit.
                    let galley = ui.painter().layout_job(tab_job(
                        theme,
                        glyph,
                        &label,
                        current,
                        show_labels,
                        font_size,
                    ));
                    ui.painter().galley(
                        egui::pos2(
                            rect.center().x - galley.size().x / 2.0,
                            rect.center().y - galley.size().y / 2.0,
                        ),
                        galley,
                        theme.text,
                    );
                    if current {
                        let underline_width = tab_width * 0.5;
                        let underline = egui::Rect::from_min_size(
                            egui::pos2(
                                motion::follow(
                                    ui.ctx(),
                                    theme,
                                    Id::unique("settings-tab-indicator"),
                                    rect.left() + (tab_width - underline_width) * 0.5,
                                    240.0,
                                    motion::ease_out_quint,
                                ),
                                rect.bottom() - 2.5,
                            ),
                            vec2(underline_width, 2.5),
                        );
                        ui.painter().rect_filled(underline, 2.0, theme.accent);
                    }
                    if response.clicked() {
                        self.tab = index;
                    }
                }
            },
        );
        hline(ui, theme, 0.0);
    }

    fn appearance(
        &mut self,
        ui: &mut Ui,
        theme: &Theme,
        bridge: &mut AppBridge,
        s: &AppSettings,
        compact: bool,
    ) {
        card(ui, theme, |ui| {
            ui.spacing_mut().item_spacing.y = 14.0;
            ui.label(
                RichText::new(tr("Тема", &[]))
                    .size(theme.font_strong as f32)
                    .family(crate::shell::theme::semibold_family()),
            );
            setting_row(
                ui,
                theme,
                "\u{e790}",
                "Тема",
                "Выберите светлую, тёмную или системную тему",
                |ui| {
                    styled_combo(
                        ui,
                        "theme",
                        &s.theme,
                        &[
                            ("system", "Авто"),
                            ("light", "Светлая"),
                            ("dark", "Тёмная"),
                        ],
                        210.0,
                        |value| bridge.set_theme(value),
                    );
                },
            );
            setting_row(
                ui,
                theme,
                "\u{e771}",
                "Палитра (пресет)",
                "Готовая цветовая тема поверх светлой/тёмной",
                |ui| {
                    styled_combo(ui, "palette", &s.ui_theme_preset, PALETTES, 210.0, |value| {
                        bridge.set_ui_theme_preset(value)
                    });
                },
            );
            let codes = bridge.available_languages();
            let labels = bridge.language_labels();
            let languages = codes
                .iter()
                .zip(&labels)
                .map(|(code, label)| (code.as_str(), label.as_str()))
                .collect::<Vec<_>>();
            setting_row(
                ui,
                theme,
                "\u{e774}",
                "Язык",
                "При первом запуске выбирается по языку системы",
                |ui| {
                    styled_combo(ui, "language", &s.language, &languages, 210.0, |value| {
                        bridge.set_language(value)
                    });
                },
            );
            setting_row(
                ui,
                theme,
                "\u{e2b1}",
                "Цвет акцента",
                "Цвет акцента для элементов интерфейса",
                |ui| {
                    if accent_picker(ui, theme, theme.accent).clicked() {
                        self.color = Some((
                            ColorPicker::open("settings-accent", tr("Цвет акцента", &[]), &s.accent_color, 0.0, true, true),
                            false,
                        ));
                    }
                },
            );
            if !compact {
                setting_row(
                    ui,
                    theme,
                    "\u{e7e6}",
                    "Свой базовый тон",
                    "Кастомный цвет подложки окна (поверх пресета)",
                    |ui| {
                        if !s.ui_base_tint.is_empty() && ghost(ui, theme, "Сбросить") {
                            bridge.set_ui_base_tint("");
                            bridge.set_ui_base_tint_src("");
                        }
                        let tint = hex_color(&s.ui_base_tint);
                        if base_tint_picker(ui, theme, tint).clicked() {
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
                            self.color = Some((
                                ColorPicker::open("settings-tint", tr("Свой базовый тон", &[]), base, mute, true, false),
                                true,
                            ));
                        }
                    },
                );
            }
        });
        card(ui, theme, |ui| {
            ui.spacing_mut().item_spacing.y = 14.0;
            ui.label(
                RichText::new(tr("Обои", &[]))
                    .size(theme.font_strong as f32)
                    .family(crate::shell::theme::semibold_family()),
            );
            let wallpaper = bridge.ui_wallpaper();
            setting_row(
                ui,
                theme,
                "\u{eb9f}",
                "Обои окна",
                "Фоновое изображение под интерфейсом",
                |ui| {
                    if !wallpaper.is_empty() && ghost(ui, theme, "Сбросить") {
                        bridge.set_ui_wallpaper("");
                    }
                    if accent_button(ui, theme, "Выбрать…") {
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
                },
            );
            if !compact && !wallpaper.is_empty() {
                setting_row(
                    ui,
                    theme,
                    "\u{e890}",
                    "Непрозрачность обоев",
                    "Насколько ярко видно изображение, %",
                    |ui| {
                        percent_slider(
                            ui,
                            theme,
                            "wallpaper-opacity",
                            s.ui_wallpaper_opacity,
                            |v| bridge.set_ui_wallpaper_opacity(v),
                        );
                    },
                );
                setting_row(
                    ui,
                    theme,
                    "\u{e7b3}",
                    "Размытие обоев",
                    "Сила размытия фона, 0–100",
                    |ui| {
                        percent_slider(ui, theme, "wallpaper-blur", s.ui_wallpaper_blur, |v| {
                            bridge.set_ui_wallpaper_blur(v)
                        });
                    },
                );
                setting_row(
                    ui,
                    theme,
                    "\u{e706}",
                    "Яркость обоев",
                    "100 = оригинал, меньше = темнее",
                    |ui| {
                        percent_slider(
                            ui,
                            theme,
                            "wallpaper-brightness",
                            s.ui_wallpaper_brightness,
                            |v| bridge.set_ui_wallpaper_brightness(v),
                        );
                    },
                );
            }
        });
        card(ui, theme, |ui| {
            ui.spacing_mut().item_spacing.y = 14.0;
            ui.label(
                RichText::new(tr("Интерфейс", &[]))
                    .size(theme.font_strong as f32)
                    .family(crate::shell::theme::semibold_family()),
            );
            setting_row(
                ui,
                theme,
                "\u{e799}",
                "Плотность",
                "Насколько компактно расположены элементы",
                |ui| {
                    styled_combo(
                        ui,
                        "density",
                        &s.ui_density,
                        &[
                            ("compact", "Компактная"),
                            ("comfortable", "Обычная"),
                            ("spacious", "Свободная"),
                        ],
                        210.0,
                        |value| bridge.set_ui_density(value),
                    );
                },
            );
            setting_row(
                ui,
                theme,
                "\u{e8e9}",
                "Масштаб интерфейса",
                "Размер шрифта и всего интерфейса, % (80–140)",
                |ui| {
                    spin(ui, theme, s.ui_font_scale, 80..=140, 5.0, |value| {
                        bridge.set_ui_font_scale(value)
                    });
                },
            );
            if !compact {
                setting_row(
                    ui,
                    theme,
                    "\u{e8b3}",
                    "Скругление углов",
                    "Радиус скругления карточек и кнопок, px",
                    |ui| {
                        spin(ui, theme, s.ui_corner_radius, 0..=20, 1.0, |value| {
                            bridge.set_ui_corner_radius(value)
                        });
                    },
                );
            }
            setting_row(
                ui,
                theme,
                "\u{e945}",
                "Анимации",
                "Плавные переходы и движение интерфейса",
                |ui| {
                    switch_setting(ui, theme, s.ui_animations, true, |value| {
                        bridge.set_ui_animations(value)
                    });
                },
            );
            setting_row(
                ui,
                theme,
                "\u{e890}",
                "Компактный режим",
                "Скрыть сложные и профессиональные настройки",
                |ui| {
                    switch_setting(ui, theme, compact, true, |value| {
                        bridge.set_interface_mode(if value { "compact" } else { "full" })
                    });
                },
            );
        });
        card(ui, theme, |ui| {
            ui.spacing_mut().item_spacing.y = 14.0;
            ui.label(
                RichText::new(tr("Прозрачность", &[]))
                    .size(theme.font_strong as f32)
                    .family(crate::shell::theme::semibold_family()),
            );
            let active = s.ui_backdrop != "solid";
            let available = bridge.ui_backdrop_available() || active;
            setting_row(
                ui,
                theme,
                "\u{e790}",
                "Прозрачность",
                "Использовать системные эффекты фона Windows 11",
                |ui| {
                    switch_setting(ui, theme, active, available, |value| {
                        bridge.set_ui_backdrop(if value { "mica" } else { "solid" })
                    });
                },
            );
            if !compact {
                setting_row(
                    ui,
                    theme,
                    "\u{e9e9}",
                    "Сила прозрачности",
                    "0: Mica почти выключена, 100: максимум Mica",
                    |ui| {
                        ui.add_enabled_ui(active, |ui| {
                            percent_slider(
                                ui,
                                theme,
                                "transparency-strength",
                                s.ui_transparency_strength,
                                |v| bridge.set_ui_transparency_strength(v),
                            );
                        });
                    },
                );
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
        card(ui, theme, |ui| {
            ui.spacing_mut().item_spacing.y = 14.0;
            ui.label(
                RichText::new(tr("Подписки", &[]))
                    .size(theme.font_strong as f32)
                    .family(crate::shell::theme::semibold_family()),
            );
            setting_row(
                ui,
                theme,
                "\u{e895}",
                "Автообновление подписок",
                "Как часто фоново обновлять подписки (прокси/VPN не отключается)",
                |ui| {
                    styled_combo(
                        ui,
                        "sub-auto",
                        &s.subscription_auto_update_minutes.to_string(),
                        &[
                            ("0", "Выкл"),
                            ("30", "30 мин"),
                            ("60", "1 час"),
                            ("240", "4 часа"),
                            ("720", "12 часов"),
                            ("1440", "24 часа"),
                        ],
                        200.0,
                        |value| {
                            bridge.set_subscription_auto_update_minutes(value.parse().unwrap_or(0))
                        },
                    );
                },
            );
            setting_row(
                ui,
                theme,
                "\u{e8a7}",
                "Загружать подписки через прокси/TUN",
                "Импорт и обновление используют активное подключение Lumen; без подключения — системную сеть",
                |ui| {
                    switch_setting(ui, theme, s.subscription_use_proxy_tun, true, |value| {
                        bridge.set_subscription_use_proxy_tun(value)
                    });
                },
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
        card(ui, theme, |ui| {
            ui.spacing_mut().item_spacing.y = 14.0;
            ui.label(
                RichText::new(tr("Фильтры regex", &[]))
                    .size(theme.font_normal as f32)
                    .family(crate::shell::theme::semibold_family()),
            );
            ui.label(
                RichText::new(tr("Каждая строка — отдельное выражение. Фильтр применяется к имени, адресу, протоколу и исходной ссылке сервера.", &[]))
                    .size(theme.font_small as f32)
                    .color(theme.text_faint),
            );
            let small = egui::FontId::proportional(theme.font_small as f32);
            let normal = egui::FontId::proportional(theme.font_normal as f32);
            let preferred = [
                column_preferred_width(ui, &[(&tr("Включать", &[]), &small)], &[(&draft.include, "NL|DE|Reality")], &normal),
                column_preferred_width(ui, &[(&tr("Исключать", &[]), &small)], &[(&draft.exclude, "expired|traffic limit")], &normal),
            ];
            layout_columns(
                ui,
                ui.available_width() >= 900.0,
                preferred,
                |ui| {
                    caption(ui, theme, "Включать");
                    styled_area(ui, theme, &mut draft.include, "NL|DE|Reality", 86.0);
                },
                |ui| {
                    caption(ui, theme, "Исключать");
                    styled_area(ui, theme, &mut draft.exclude, "expired|traffic limit", 86.0);
                },
            );
        });
        card(ui, theme, |ui| {
            ui.spacing_mut().item_spacing.y = 14.0;
            ui.label(
                RichText::new(tr("Параметры запроса", &[]))
                    .size(theme.font_normal as f32)
                    .family(crate::shell::theme::semibold_family()),
            );
            setting_row(
                ui,
                theme,
                "\u{e8d4}",
                "User-Agent",
                "Пустое поле использует полный профиль запросов Happ для Windows",
                |ui| {
                    plain_field(ui, theme, &mut draft.agent, "Happ/2.18.3/Windows/2606241603601", 360.0);
                },
            );
            setting_row(
                ui,
                theme,
                "\u{e72e}",
                "Отправлять настоящий HWID",
                "Использовать MachineGuid текущей установки Windows вместо указанного вручную",
                |ui| {
                    switch_setting(ui, theme, draft.real_hwid, true, |value| {
                        draft.real_hwid = value;
                    });
                },
            );
            if !draft.real_hwid {
                setting_row(
                    ui,
                    theme,
                    "\u{e950}",
                    "HWID",
                    "Идентификатор устройства для заголовка X-Hwid; пустое поле использует стандартное значение Happ",
                    |ui| {
                        plain_field(
                            ui,
                            theme,
                            &mut draft.hwid,
                            "00000000-0000-4000-8000-000000000000",
                            360.0,
                        );
                    },
                );
            }
            setting_row(
                ui,
                theme,
                "\u{e8ab}",
                "Конвертация подписки",
                "Перед загрузкой передавать URL сервису-конвертеру",
                |ui| {
                    switch_setting(ui, theme, draft.converter, true, |value| {
                        draft.converter = value;
                    });
                },
            );
            ui.add_enabled_ui(draft.converter, |ui| {
                plain_field(
                    ui,
                    theme,
                    &mut draft.converter_url,
                    "https://converter.example/sub?url={url}",
                    ui.available_width().max(120.0),
                );
            });
            right_row(ui, theme.control_height as f32, |ui| {
                if accent_button_glyph(ui, theme, "\u{e74e}", "Сохранить настройки подписок") {
                    bridge.set_subscription_user_agent(&draft.agent);
                    bridge.set_subscription_include_regex(&draft.include);
                    bridge.set_subscription_exclude_regex(&draft.exclude);
                    bridge.set_subscription_use_real_hwid(draft.real_hwid);
                    bridge.set_subscription_hwid(&draft.hwid);
                    bridge.set_subscription_converter_enabled(draft.converter);
                    bridge.set_subscription_converter_url(&draft.converter_url);
                }
            });
        });
    }

    fn dns(&mut self, ui: &mut Ui, theme: &Theme, bridge: &mut AppBridge, compact: bool) {
        let mode = bridge.dns_mode();
        card(ui, theme, |ui| {
            ui.spacing_mut().item_spacing.y = 14.0;
            ui.label(
                RichText::new("DNS")
                    .size(theme.font_strong as f32)
                    .family(crate::shell::theme::semibold_family()),
            );
            setting_row(
                ui,
                theme,
                "\u{e774}",
                "Режим DNS",
                "Системный DNS или встроенный DNS выбранного ядра",
                |ui| {
                    styled_combo(
                        ui,
                        "dns-mode",
                        &mode,
                        &[("system", "Системный"), ("builtin", "Встроенный")],
                        210.0,
                        |value| bridge.set_dns_mode(value),
                    );
                },
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
            card(ui, theme, |ui| {
                ui.spacing_mut().item_spacing.y = 14.0;
                ui.label(
                    RichText::new(tr("DNS-серверы", &[]))
                        .size(theme.font_normal as f32)
                        .family(crate::shell::theme::semibold_family()),
                );
                let wide = ui.available_width() >= 900.0;
                let small = egui::FontId::proportional(theme.font_small as f32);
                let normal = egui::FontId::proportional(theme.font_normal as f32);
                let preferred = [
                    column_preferred_width(
                        ui,
                        &[
                            (&tr("Прямые DNS-серверы", &[]), &normal),
                            (&tr("Необязательно; пусто — DNS только через прокси", &[]), &small),
                        ],
                        &[(&draft.direct, "1.1.1.1")],
                        &normal,
                    ),
                    column_preferred_width(
                        ui,
                        &[
                            (&tr("DNS через прокси", &[]), &normal),
                            (&tr("По одному адресу в строке", &[]), &small),
                        ],
                        &[(&draft.proxy, "cloudflare-dns.com")],
                        &normal,
                    ),
                ];
                layout_columns(
                    ui,
                    wide,
                    preferred,
                    |ui| {
                        ui.label(text("Прямые DNS-серверы", theme));
                        caption(ui, theme, "Необязательно; пусто — DNS только через прокси");
                        styled_area(ui, theme, &mut draft.direct, "1.1.1.1", 86.0);
                    },
                    |ui| {
                        ui.label(text("DNS через прокси", theme));
                        caption(ui, theme, "По одному адресу в строке");
                        styled_area(ui, theme, &mut draft.proxy, "cloudflare-dns.com", 86.0);
                    },
                );
                let bootstrap_type = bridge.dns_bootstrap_type();
                let proxy_type = bridge.dns_proxy_type();
                let bootstrap_strategy = bridge.dns_bootstrap_strategy();
                let proxy_strategy = bridge.dns_proxy_strategy();
                if wide {
                    // GridLayout `columnSpacing: 12`.
                    ui.spacing_mut().item_spacing.x = 12.0;
                    ui.columns(2, |columns| {
                        setting_row(&mut columns[0], theme, "\u{e8a5}", "Тип прямого DNS", "", |ui| {
                            styled_combo(
                                ui,
                                "dns-bootstrap-type",
                                &bootstrap_type,
                                DNS_TYPES,
                                210.0,
                                |value| bridge.set_bootstrap_dns("", value),
                            );
                        });
                        setting_row(&mut columns[1], theme, "\u{e8a5}", "Тип proxy DNS", "", |ui| {
                            styled_combo(ui, "dns-proxy-type", &proxy_type, DNS_TYPES, 210.0, |value| {
                                bridge.set_proxy_dns("", value)
                            });
                        });
                    });
                    ui.columns(2, |columns| {
                        setting_row(&mut columns[0], theme, "\u{e8ef}", "Стратегия прямого DNS", "", |ui| {
                            styled_combo(
                                ui,
                                "dns-bootstrap-strategy",
                                &bootstrap_strategy,
                                DNS_STRATEGIES,
                                210.0,
                                |value| bridge.set_dns_bootstrap_strategy(value),
                            );
                        });
                        setting_row(&mut columns[1], theme, "\u{e8ef}", "Стратегия proxy DNS", "", |ui| {
                            styled_combo(
                                ui,
                                "dns-proxy-strategy",
                                &proxy_strategy,
                                DNS_STRATEGIES,
                                210.0,
                                |value| bridge.set_dns_proxy_strategy(value),
                            );
                        });
                    });
                } else {
                    setting_row(ui, theme, "\u{e8a5}", "Тип прямого DNS", "", |ui| {
                        styled_combo(ui, "dns-bootstrap-type", &bootstrap_type, DNS_TYPES, 210.0, |value| {
                            bridge.set_bootstrap_dns("", value)
                        });
                    });
                    setting_row(ui, theme, "\u{e8a5}", "Тип proxy DNS", "", |ui| {
                        styled_combo(ui, "dns-proxy-type", &proxy_type, DNS_TYPES, 210.0, |value| {
                            bridge.set_proxy_dns("", value)
                        });
                    });
                    setting_row(ui, theme, "\u{e8ef}", "Стратегия прямого DNS", "", |ui| {
                        styled_combo(
                            ui,
                            "dns-bootstrap-strategy",
                            &bootstrap_strategy,
                            DNS_STRATEGIES,
                            210.0,
                            |value| bridge.set_dns_bootstrap_strategy(value),
                        );
                    });
                    setting_row(ui, theme, "\u{e8ef}", "Стратегия proxy DNS", "", |ui| {
                        styled_combo(
                            ui,
                            "dns-proxy-strategy",
                            &proxy_strategy,
                            DNS_STRATEGIES,
                            210.0,
                            |value| bridge.set_dns_proxy_strategy(value),
                        );
                    });
                }
                setting_row(
                    ui,
                    theme,
                    "\u{e8c9}",
                    "Параллельные DNS-запросы",
                    "Запрашивать несколько DNS одновременно, когда ядро это поддерживает",
                    |ui| {
                        switch_setting(ui, theme, bridge.dns_parallel_query(), true, |value| {
                            bridge.set_dns_parallel_query(value)
                        });
                    },
                );
                setting_row(
                    ui,
                    theme,
                    "\u{e823}",
                    "Оптимистичный кэш",
                    "Отдавать устаревший ответ и обновлять его в фоне, когда ядро это поддерживает",
                    |ui| {
                        switch_setting(ui, theme, bridge.dns_optimistic_cache(), true, |value| {
                            bridge.set_dns_optimistic_cache(value)
                        });
                    },
                );
                setting_row(
                    ui,
                    theme,
                    "\u{e81e}",
                    "Geo-проверка DNS",
                    "Выбирать прямой или proxy DNS по правилам GeoSite пресета",
                    |ui| {
                        switch_setting(ui, theme, bridge.dns_geo_check(), true, |value| {
                            bridge.set_dns_geo_check(value)
                        });
                    },
                );
                setting_row(
                    ui,
                    theme,
                    "\u{e8d7}",
                    "Перехватывать DNS в TUN",
                    "Направлять DNS-запросы приложений во встроенный DNS",
                    |ui| {
                        switch_setting(ui, theme, bridge.dns_hijack_enabled(), true, |value| {
                            bridge.set_dns_hijack_enabled(value)
                        });
                    },
                );
                setting_row(
                    ui,
                    theme,
                    "\u{e8f1}",
                    "Fake DNS",
                    "Использовать FakeIP для DNS в TUN",
                    |ui| {
                        switch_setting(ui, theme, bridge.dns_fake_enabled(), true, |value| {
                            bridge.set_dns_fake_enabled(value)
                        });
                    },
                );
            });
            card(ui, theme, |ui| {
                ui.spacing_mut().item_spacing.y = 14.0;
                ui.label(
                    RichText::new("Hosts")
                        .size(theme.font_normal as f32)
                        .family(crate::shell::theme::semibold_family()),
                );
                ui.label(
                    RichText::new(tr("Формат: domain=address. Несколько адресов разделяются запятыми.", &[]))
                        .size(theme.font_small as f32)
                        .color(theme.text_faint),
                );
                styled_area(ui, theme, &mut draft.hosts, "example.com=1.1.1.1\nlocal.test=127.0.0.1,::1", 110.0);
                right_row(ui, theme.control_height as f32, |ui| {
                    if accent_button_glyph(ui, theme, "\u{e74e}", "Применить DNS") {
                        bridge.apply_dns_settings(&draft.direct, &draft.proxy, &draft.hosts);
                    }
                });
            });
        });
    }

    fn data(&mut self, ui: &mut Ui, theme: &Theme, bridge: &mut AppBridge, compact: bool) -> bool {
        let mut configs = false;
        card(ui, theme, |ui| {
            ui.spacing_mut().item_spacing.y = 14.0;
            ui.label(
                RichText::new(tr("Данные", &[]))
                    .size(theme.font_strong as f32)
                    .family(crate::shell::theme::semibold_family()),
            );
            let loaded = bridge.profile_loaded();
            setting_row(
                ui,
                theme,
                "\u{e72e}",
                "Пароль профиля",
                if loaded {
                    "Защитить настройки и серверы паролем"
                } else {
                    "Профиль не открыт. Введите пароль; изменения и подключение заблокированы."
                },
                |ui| {
                    // Right-to-left row: the button is added first so it
                    // ends up after the field, as in QML.
                    let clicked = accent_button(
                        ui,
                        theme,
                        if loaded { "Задать пароль" } else { "Открыть профиль" },
                    );
                    let edit = ui.add(styled_field(
                        theme,
                        TextEdit::singleline(&mut self.password).password(true),
                        &tr("Пароль", &[]),
                        200.0,
                    ));
                    if clicked
                        || (edit.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter)))
                    {
                        bridge.set_encryption_password(&self.password);
                        self.password.clear();
                    }
                },
            );
            setting_row(
                ui,
                theme,
                "\u{e74e}",
                "Резервная копия",
                "Экспорт и импорт настроек и серверов",
                |ui| {
                    // Right-to-left row: QML order is Экспорт, Импорт.
                    if ghost_glyph(ui, theme, "\u{e8b7}", "Импорт") {
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
                    if ghost_glyph(ui, theme, "\u{e74e}", "Экспорт") {
                        self.file_error = file_result(
                            win_native::save_json_file(
                                &tr("Сохранить резервную копию", &[]),
                                "lumen-backup.json",
                            ),
                            |path| bridge.export_backup(&path, ""),
                        );
                    }
                },
            );
            setting_row(
                ui,
                theme,
                "\u{e9d9}",
                "Телеметрия",
                "Отправлять диагностические ошибки на сервер Lumen",
                |ui| {
                    switch_setting(
                        ui,
                        theme,
                        bridge.diagnostics_upload_enabled(),
                        true,
                        |value| bridge.set_diagnostics_upload(value),
                    );
                },
            );
            setting_row(
                ui,
                theme,
                "\u{e943}",
                "Конфиги",
                "Шаблоны конфигураций Xray и sing-box",
                |ui| {
                    configs = ghost_glyph(ui, theme, "\u{e943}", "Открыть");
                },
            );
        });
        if !compact {
            card(ui, theme, |ui| {
                ui.spacing_mut().item_spacing.y = 14.0;
                ui.label(
                    RichText::new(tr("Пути к ядрам", &[]))
                        .size(theme.font_strong as f32)
                        .family(crate::shell::theme::semibold_family()),
                );
                let xray = bridge.xray_path_display();
                let singbox = bridge.singbox_path_display();
                for (key, title, subtitle, value, hint) in [
                    (
                        "xray",
                        "Ядро Xray",
                        "Путь к исполняемому файлу xray",
                        xray,
                        "core\\xray.exe",
                    ),
                    (
                        "singbox",
                        "Ядро sing-box-extended",
                        "Путь к исполняемому файлу sing-box-extended",
                        singbox,
                        "core\\sing-box-extended\\sing-box.exe",
                    ),
                ] {
                    setting_row(ui, theme, "\u{e756}", title, subtitle, |ui| {
                        // Right-to-left row: QML order is the field, then Обзор.
                        if ghost_glyph(ui, theme, "\u{e8b7}", "Обзор") {
                            self.file_error = pick(
                                &tr("Обзор", &[]),
                                "Executable files (*.exe)\0*.exe\0",
                                |path| {
                                    if key == "xray" {
                                        bridge.set_xray_path(&path.to_string_lossy());
                                    } else {
                                        bridge.set_singbox_path(&path.to_string_lossy());
                                    }
                                },
                            );
                        }
                        if let Some(path) = committed_field(
                            ui,
                            theme,
                            ("settings-path", key),
                            &value,
                            hint,
                            240.0,
                            false,
                        ) {
                            if key == "xray" {
                                bridge.set_xray_path(&path);
                            } else {
                                bridge.set_singbox_path(&path);
                            }
                        }
                    });
                }
            });
        }
        card(ui, theme, |ui| {
            ui.spacing_mut().item_spacing.y = 14.0;
            setting_row(
                ui,
                theme,
                "\u{e777}",
                "Сброс настроек",
                "Вернуть настройки приложения и маршрутизации по умолчанию. Серверы и подписки останутся.",
                |ui| {
                    if ui.add(danger_button(theme, "Сбросить настройки")).clicked() {
                        self.reset = true;
                    }
                },
            );
        });
        ui.add_space(4.0);
        ui.label(
            RichText::new(format!(
                "{} · v{APP_VERSION}",
                lumen_core::constants::APP_NAME
            ))
            .size(theme.font_small as f32)
            .color(theme.text_faint),
        );
        configs
    }

    fn reset_dialog(&mut self, ctx: &Context, theme: &Theme, bridge: &mut AppBridge) {
        use crate::shell::fluent_dialog::{DialogAction, FluentDialog, body_text};
        // resetSettingsDialog: FluentDialog, 480 px.
        let action = FluentDialog::new("settings-reset", tr("Сбросить настройки?", &[]), 480.0)
            .ok(tr("Сбросить", &[]))
            .show(ctx, theme, |ui| {
                body_text(
                    ui,
                    &tr("Настройки приложения и маршрутизации вернутся к значениям по умолчанию. Серверы и подписки не будут удалены.", &[]),
                    FontId::proportional(theme.font_normal as f32),
                    theme.text_muted,
                );
            });
        if action == DialogAction::Accept {
            bridge.reset_settings_to_defaults();
            self.subscription = None;
            self.dns = None;
        }
        if action != DialogAction::None {
            self.reset = false;
        }
    }

    // __CHUNK3__
}

fn network(ui: &mut Ui, theme: &Theme, bridge: &mut AppBridge, s: &AppSettings, compact: bool) {
    card(ui, theme, |ui| {
        ui.spacing_mut().item_spacing.y = 14.0;
        ui.label(
            RichText::new(tr("Сеть", &[]))
                .size(theme.font_strong as f32)
                .family(crate::shell::theme::semibold_family()),
        );
        let region = bridge.regional_preset();
        setting_row(
            ui,
            theme,
            "\u{e909}",
            "Региональный профиль",
            "Меняет быстрые пресеты и источники GeoIP/GeoSite",
            |ui| {
                ui.add_enabled_ui(!bridge.regional_preset_busy(), |ui| {
                    styled_combo(
                        ui,
                        "region",
                        &region,
                        &[("russia", "Россия"), ("china", "Китай"), ("iran", "Иран")],
                        220.0,
                        |value| bridge.set_regional_preset(value),
                    );
                });
            },
        );
        setting_row(
            ui,
            theme,
            "\u{e950}",
            "Основное ядро",
            "Auto — подходящее ядро для каждого сервера",
            |ui| {
                styled_combo(
                    ui,
                    "core-preference",
                    &s.core_preference,
                    &[
                        ("auto", "Auto"),
                        ("xray", "Xray"),
                        ("singbox", "sing-box extended"),
                    ],
                    220.0,
                    |value| bridge.set_core_preference(value),
                );
                info_icon(ui, theme, "Auto сам выбирает ядро для каждого сервера и может работать гибридом sing-box + Xray. Xray — всё, включая TUN, только через Xray; серверы sing-box (Hysteria2, TUIC, AWG, WARP) недоступны. sing-box extended — всё только через sing-box; серверы, которым нужен Xray, недоступны.");
            },
        );
        setting_row(
            ui,
            theme,
            "\u{e80f}",
            "Прокси в обход локальной сети",
            "Не проксировать адреса локальной сети",
            |ui| {
                switch_setting(ui, theme, s.system_proxy_bypass_lan, true, |value| {
                    bridge.set_proxy_bypass_lan(value)
                });
            },
        );
        setting_row(
            ui,
            theme,
            "\u{e774}",
            "Настраивать прокси Firefox",
            "Принудительно записывать прокси в профили браузеров Firefox",
            |ui| {
                switch_setting(ui, theme, s.firefox_proxy_integration, true, |value| {
                    bridge.set_firefox_proxy_integration(value)
                });
                info_icon(ui, theme, "Обычно Firefox использует системный прокси автоматически. Включайте эту настройку только если Firefox его игнорирует.");
            },
        );
        setting_row(
            ui,
            theme,
            "\u{e895}",
            "Переподключение при смене сети",
            "Автоматически переподключаться при изменении сети",
            |ui| {
                switch_setting(ui, theme, s.reconnect_on_network_change, true, |value| {
                    bridge.set_reconnect_on_network_change(value)
                });
            },
        );
        if !compact {
            setting_row(
                ui,
                theme,
                "\u{e968}",
                "Предпочитать IPv6",
                "Использовать IPv6-адреса раньше IPv4, если сервер и сеть это поддерживают",
                |ui| {
                    switch_setting(ui, theme, s.prefer_ipv6, true, |value| {
                        bridge.set_prefer_ipv6(value)
                    });
                },
            );
        }
        setting_row(
            ui,
            theme,
            "\u{e72e}",
            "Kill-switch",
            "При обрыве VPN блокировать трафик вместо выхода напрямую",
            |ui| {
                switch_setting(ui, theme, s.kill_switch, true, |value| {
                    bridge.set_kill_switch(value)
                });
            },
        );
        if !compact {
            setting_row(
                ui,
                theme,
                "\u{e8a6}",
                "Фрагментирование",
                "Разделять TLS ClientHello на части для обхода DPI; иногда снижает стабильность",
                |ui| {
                    switch_setting(ui, theme, bridge.fragmentation_enabled(), true, |value| {
                        bridge.set_fragmentation(value)
                    });
                    info_icon(ui, theme, "Фрагментирование делит TLS-рукопожатие на части. Может помочь обходу DPI, но иногда снижает стабильность.");
                },
            );
            if bridge.fragmentation_enabled() {
                setting_row(
                    ui,
                    theme,
                    "\u{e9d9}",
                    "Настройки фрагментирования",
                    "Fragment Settings",
                    |ui| {
                        let mut packets = bridge.fragment_packets();
                        let mut length = bridge.fragment_length();
                        let mut delay = bridge.fragment_delay();
                        // Right-to-left row: QML order is the info icon,
                        // packets, length, delay.
                        let c = committed_field(ui, theme, "frag-delay", &delay, "10-20", 100.0, false);
                        let b = committed_field(ui, theme, "frag-length", &length, "50-100", 100.0, false);
                        let a = committed_field(ui, theme, "frag-packets", &packets, "tlshello", 100.0, false);
                        info_icon(ui, theme, "Packets выбирает часть трафика, Length размер фрагментов, Delay задержку между ними.");
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
                    },
                );
            }
            setting_row(
                ui,
                theme,
                "\u{e7c3}",
                "Мультиплексирование",
                "Объединять несколько соединений в один канал; зависит от поддержки сервера",
                |ui| {
                    switch_setting(ui, theme, s.multiplex_enabled, true, |value| {
                        bridge.set_multiplexing(value)
                    });
                    info_icon(ui, theme, "Мультиплексирование объединяет несколько запросов в один канал. Может ускорить много мелких соединений, но зависит от поддержки сервера.");
                },
            );
            if s.multiplex_enabled {
                setting_row(
                    ui,
                    theme,
                    "\u{e9d9}",
                    "Настройки мультиплексирования",
                    "Multiplex Settings",
                    |ui| {
                        spin(ui, theme, s.multiplex_concurrency, 1..=32, 1.0, |value| {
                            bridge.set_multiplex_concurrency(value)
                        });
                        info_icon(ui, theme, "Потоки задают, сколько запросов можно вести внутри одного мультиплексированного соединения.");
                    },
                );
            }
        }
    });
    card(ui, theme, |ui| {
        ui.spacing_mut().item_spacing.y = 14.0;
        ui.label(
            RichText::new(tr("Цепочка прокси", &[]))
                .size(theme.font_strong as f32)
                .family(crate::shell::theme::semibold_family()),
        );
        setting_row(
            ui,
            theme,
            "\u{e71b}",
            "Использовать цепочку",
            "Приложение → предыдущий прокси → выбранный сервер → выходной прокси → сайт. Работает в Xray и sing-box.",
            |ui| {
                switch_setting(ui, theme, s.proxy_chain_enabled, true, |value| {
                    bridge.set_proxy_chain_enabled(value)
                });
            },
        );
        let options = bridge.proxy_chain_node_options();
        let options = options
            .iter()
            .map(|(id, label)| (id.as_str(), label.as_str()))
            .collect::<Vec<_>>();
        ui.add_enabled_ui(s.proxy_chain_enabled, |ui| {
            setting_row(
                ui,
                theme,
                "\u{e8ab}",
                "Предыдущий прокси",
                "Через него идёт подключение к выбранному серверу",
                |ui| {
                    if let Some(value) = chain_combo(ui, theme, "chain-front", &s.proxy_chain_front_node_id, &options) {
                        bridge.set_proxy_chain_front(value);
                    }
                },
            );
            setting_row(
                ui,
                theme,
                "\u{e774}",
                "Выходной прокси",
                "Сайты видят IP этого сервера",
                |ui| {
                    if let Some(value) = chain_combo(ui, theme, "chain-exit", &s.proxy_chain_exit_node_id, &options) {
                        bridge.set_proxy_chain_exit(value);
                    }
                },
            );
        });
        if s.proxy_chain_enabled {
            let preview = bridge.proxy_chain_preview();
            let hops = preview["hops"].as_array().cloned().unwrap_or_default();
            if hops.len() > 1 {
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    for (index, hop) in hops.iter().enumerate() {
                        if index > 0 {
                            ui.label(
                                RichText::new("\u{e76c}")
                                    .size(10.0)
                                    .color(theme.text_faint),
                            );
                        }
                        let label = hop["label"].as_str().unwrap_or_default();
                        let main = hop["role"].as_str() == Some("main");
                        let fill = if main {
                            theme.accent_soft
                        } else {
                            theme.control_fill
                        };
                        chip(ui, theme, label, fill);
                    }
                });
            }
            for note in preview["notes"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(serde_json::Value::as_str)
            {
                ui.colored_label(theme.warning, format!("⚠ {note}"));
            }
        }
    });
    if !compact {
        card(ui, theme, |ui| {
            ui.spacing_mut().item_spacing.y = 14.0;
            ui.label(
                RichText::new(tr("Авто-переключение", &[]))
                    .size(theme.font_strong as f32)
                    .family(crate::shell::theme::semibold_family()),
            );
            setting_row(
                ui,
                theme,
                "\u{e895}",
                "Включить авто-переключение",
                "Переключать сервер при падении скорости",
                |ui| {
                    switch_setting(ui, theme, s.auto_switch_enabled, true, |value| {
                        bridge.set_auto_switch(value)
                    });
                },
            );
            setting_row(
                ui,
                theme,
                "\u{ec4a}",
                "Порог скорости",
                "КБ/с, ниже которого срабатывает переключение",
                |ui| {
                    spin(ui, theme, s.auto_switch_threshold_kbps, 1..=10000, 1.0, |value| {
                        bridge.set_auto_switch_threshold(value)
                    });
                },
            );
            setting_row(
                ui,
                theme,
                "\u{e916}",
                "Задержка",
                "Секунд низкой скорости перед переключением",
                |ui| {
                    spin(ui, theme, s.auto_switch_delay_sec, 5..=300, 1.0, |value| {
                        bridge.set_auto_switch_delay(value)
                    });
                },
            );
            setting_row(
                ui,
                theme,
                "\u{e81c}",
                "Пауза",
                "Секунд между переключениями",
                |ui| {
                    spin(ui, theme, s.auto_switch_cooldown_sec, 10..=600, 1.0, |value| {
                        bridge.set_auto_switch_cooldown(value)
                    });
                },
            );
        });
        card(ui, theme, |ui| {
            ui.spacing_mut().item_spacing.y = 14.0;
            ui.label(
                RichText::new(tr("Прокси", &[]))
                    .size(theme.font_strong as f32)
                    .family(crate::shell::theme::semibold_family()),
            );
            setting_row(
                ui,
                theme,
                "\u{e8a6}",
                "SOCKS-порт",
                "Локальный порт SOCKS/mixed прокси (рекомендуется 10808)",
                |ui| {
                    spin(ui, theme, s.local_socks_port, 1025..=65535, 1.0, |value| {
                        bridge.set_local_socks_port(value)
                    });
                    info_icon(ui, theme, "Локальный порт для SOCKS и mixed прокси. Порт 10818 зарезервирован под Discord Voice и будет сброшен на рекомендованный. Применяется с переподключением.");
                },
            );
            setting_row(
                ui,
                theme,
                "\u{e8a6}",
                "HTTP-порт",
                "Локальный порт HTTP прокси (рекомендуется 10809)",
                |ui| {
                    spin(ui, theme, s.local_http_port, 1025..=65535, 1.0, |value| {
                        bridge.set_local_http_port(value)
                    });
                    info_icon(ui, theme, "Локальный порт для HTTP прокси. Должен отличаться от SOCKS-порта; порт 10818 зарезервирован под Discord Voice. Применяется с переподключением.");
                },
            );
            setting_row(
                ui,
                theme,
                "\u{e72e}",
                "Авторизация прокси",
                "Требовать логин и пароль для SOCKS/mixed и HTTP прокси",
                |ui| {
                    switch_setting(ui, theme, s.proxy_auth_enabled, true, |value| {
                        bridge.set_proxy_auth_enabled(value);
                    });
                    info_icon(ui, theme, "По умолчанию выключено. После изменения активное подключение переподключится. Клиенты системного прокси должны поддерживать HTTP-аутентификацию.");
                },
            );
            let mut next_user = None;
            let mut next_password = None;
            setting_row(
                ui,
                theme,
                "\u{e77b}",
                "Логин прокси",
                "Имя пользователя для локального прокси",
                |ui| {
                    next_user = committed_field(
                        ui,
                        theme,
                        "proxy-auth-user",
                        &s.proxy_auth_username,
                        "Логин",
                        240.0,
                        false,
                    );
                },
            );
            setting_row(
                ui,
                theme,
                "\u{e8d7}",
                "Пароль прокси",
                "Пароль не показывается в интерфейсе",
                |ui| {
                    next_password = committed_field(
                        ui,
                        theme,
                        "proxy-auth-pass",
                        &s.proxy_auth_password,
                        "Пароль",
                        240.0,
                        true,
                    );
                },
            );
            if next_user.is_some() || next_password.is_some() {
                bridge.set_proxy_auth_credentials(
                    next_user.as_deref().unwrap_or(&s.proxy_auth_username),
                    next_password.as_deref().unwrap_or(&s.proxy_auth_password),
                );
            }
            setting_row(
                ui,
                theme,
                "\u{e774}",
                "Разрешить подключения из локальной сети",
                "Открыть SOCKS/HTTP порты для других устройств вашей сети",
                |ui| {
                    switch_setting(ui, theme, s.proxy_allow_lan, true, |value| {
                        bridge.set_proxy_allow_lan(value)
                    });
                    info_icon(ui, theme, "Другие устройства вашей сети смогут использовать этот ПК как прокси (адрес — IP этого ПК и указанные выше порты). Работает в режиме прокси. Windows может запросить разрешение брандмауэра.");
                },
            );
            setting_row(
                ui,
                theme,
                "\u{e895}",
                "Sniffing только для маршрутизации",
                "Не подменять адрес назначения доменом из sniffing (рекомендуется: выключено)",
                |ui| {
                    switch_setting(ui, theme, s.sniff_route_only, true, |value| {
                        bridge.set_sniff_route_only(value)
                    });
                    info_icon(ui, theme, "Домен из sniffing используется только для правил маршрутизации, а соединение идёт на исходный IP. Может помочь, если отдельные приложения некорректно работают с подменой адреса.");
                },
            );
        });
        let limited = bridge.limited_mode();
        ui.add_enabled_ui(!limited, |ui| {
            card(ui, theme, |ui| {
                ui.spacing_mut().item_spacing.y = 14.0;
                ui.label(
                    RichText::new(tr("TUN", &[]))
                        .size(theme.font_strong as f32)
                        .family(crate::shell::theme::semibold_family()),
                );
                setting_row(
                    ui,
                    theme,
                    "\u{e72e}",
                    "Strict Route",
                    "Ограничивать обход TUN для защиты от утечек (рекомендуется: включено)",
                    |ui| {
                        switch_setting(ui, theme, s.tun_strict_route, true, |value| {
                            bridge.set_tun_strict_route(value)
                        });
                        info_icon(ui, theme, "Защищает от DNS-утечек Windows при нескольких сетевых адаптерах, но также может ограничить трафик в обход TUN. Может нарушить Discord Voice (ICE fallback), работу WinDivert, VirtualBox и других приложений с нестандартной маршрутизацией. Применяется при следующем включении TUN.");
                    },
                );
                setting_row(
                    ui,
                    theme,
                    "\u{e968}",
                    "Сетевой стек TUN",
                    "mixed — рекомендуется",
                    |ui| {
                        styled_combo(
                            ui,
                            "tun-stack",
                            &s.tun_stack,
                            &[
                                ("mixed", "mixed"),
                                ("system", "system"),
                                ("gvisor", "gvisor"),
                            ],
                            210.0,
                            |value| bridge.set_tun_stack(value),
                        );
                        info_icon(ui, theme, "system — быстрее, gvisor — совместимее, mixed — system для TCP и gVisor для UDP.");
                    },
                );
                setting_row(
                    ui,
                    theme,
                    "\u{e9d9}",
                    "MTU",
                    "Размер пакета TUN-адаптера (рекомендуется 9000)",
                    |ui| {
                        spin(ui, theme, s.tun_mtu, 1280..=65535, 1.0, |value| {
                            bridge.set_tun_mtu(value)
                        });
                    },
                );
                setting_row(
                    ui,
                    theme,
                    "\u{e72c}",
                    "Блокировать QUIC/HTTP3 в TUN",
                    "Браузеры переходят на HTTP/2 через прокси (рекомендуется: включено)",
                    |ui| {
                        switch_setting(ui, theme, s.tun_block_quic, true, |value| {
                            bridge.set_tun_block_quic(value)
                        });
                        info_icon(ui, theme, "QUIC/HTTP3 из TUN отклоняется, чтобы браузеры использовали TCP HTTP/2 — так стабильнее через прокси. Отключайте, только если нужен HTTP/3. Применяется при следующем включении TUN.");
                    },
                );
                setting_row(
                    ui,
                    theme,
                    "\u{e774}",
                    "Endpoint-Independent NAT",
                    "Full-cone NAT для игр и P2P (рекомендуется: выключено)",
                    |ui| {
                        switch_setting(
                            ui,
                            theme,
                            s.tun_endpoint_independent_nat,
                            true,
                            |value| bridge.set_tun_endpoint_independent_nat(value),
                        );
                        info_icon(ui, theme, "Улучшает совместимость с играми и P2P-приложениями, но потребляет больше памяти. Работает со стеками gvisor и mixed. Применяется при следующем включении TUN.");
                    },
                );
            });
        });
    }
}

fn startup(ui: &mut Ui, theme: &Theme, bridge: &mut AppBridge, s: &AppSettings, compact: bool) {
    card(ui, theme, |ui| {
        ui.spacing_mut().item_spacing.y = 14.0;
        ui.label(
            RichText::new(tr("Запуск", &[]))
                .size(theme.font_strong as f32)
                .family(crate::shell::theme::semibold_family()),
        );
        setting_row(
            ui,
            theme,
            "\u{e7e8}",
            "Запускать при входе",
            "Автозапуск вместе с Windows",
            |ui| {
                switch_setting(ui, theme, s.launch_on_startup, true, |value| {
                    bridge.set_launch_on_startup(value)
                });
            },
        );
        // `enabled: App.launchOnStartup; opacity: enabled ? 1.0 : 0.55`.
        ui.scope(|ui| {
            if !s.launch_on_startup {
                ui.multiply_opacity(0.55);
            }
            setting_row(
                ui,
                theme,
                "\u{e7e8}",
                "Запускать автозапуск в трей",
                "При входе в Windows не показывать окно, только иконку в трее",
                |ui| {
                    switch_setting(
                        ui,
                        theme,
                        s.launch_in_tray_on_startup,
                        s.launch_on_startup,
                        |value| bridge.set_launch_in_tray_on_startup(value),
                    );
                },
            );
        });
        setting_row(
            ui,
            theme,
            "\u{e72e}",
            "Всегда запускать от администратора",
            "При каждом запуске Windows будет запрашивать права администратора",
            |ui| {
                switch_setting(ui, theme, s.always_run_as_admin, true, |value| {
                    bridge.set_always_run_as_admin(value)
                });
            },
        );
        setting_row(
            ui,
            theme,
            "\u{e768}",
            "Автоподключение после запуска",
            "Подключаться к последнему использованному серверу при старте",
            |ui| {
                switch_setting(ui, theme, s.auto_connect_last, true, |value| {
                    bridge.set_auto_connect_last(value)
                });
            },
        );
        setting_row(
            ui,
            theme,
            "\u{e7e8}",
            "Восстанавливать VPN после сна",
            "Автоматически переподключаться после выхода Windows из сна или гибернации",
            |ui| {
                switch_setting(ui, theme, s.reconnect_after_sleep, true, |value| {
                    bridge.set_reconnect_after_sleep(value)
                });
            },
        );
        setting_row(
            ui,
            theme,
            "\u{e72e}",
            "Проверка VPN",
            "Не запускать Lumen при работающем другом VPN или прокси",
            |ui| {
                switch_setting(ui, theme, s.block_vpn_conflicts, true, |value| {
                    bridge.set_block_vpn_conflicts(value)
                });
            },
        );
        setting_row(
            ui,
            theme,
            "\u{e896}",
            "Автоподключение при импорте",
            "Сразу подключаться к импортированному серверу",
            |ui| {
                switch_setting(ui, theme, s.auto_connect_on_import, true, |value| {
                    bridge.set_auto_connect_on_import(value)
                });
            },
        );
        if !compact {
            setting_row(
                ui,
                theme,
                "\u{e945}",
                "Автозапуск Zapret",
                "Запускать Zapret при старте приложения",
                |ui| {
                    switch_setting(ui, theme, s.zapret_autostart, true, |value| {
                        bridge.set_zapret_autostart(value)
                    });
                },
            );
        }
    });
    if !compact {
        card(ui, theme, |ui| {
            ui.spacing_mut().item_spacing.y = 14.0;
            ui.label(
                RichText::new(tr("Тестирование серверов", &[]))
                    .size(theme.font_strong as f32)
                    .family(crate::shell::theme::semibold_family()),
            );
            setting_row(
                ui,
                theme,
                "\u{e724}",
                "Способ измерения ping",
                "TCP ping быстрее всего; ICMP — системный ping; реальная задержка — HTTP через сервер",
                |ui| {
                    styled_combo(
                        ui,
                        "ping-method",
                        &s.ping_method,
                        &[
                            ("tcping", "TCP ping (быстро)"),
                            ("icmp", "ICMP (системный)"),
                            ("http", "HTTP GET"),
                            ("real", "Реальная задержка (HTTP)"),
                        ],
                        210.0,
                        |value| bridge.set_ping_method(value),
                    );
                },
            );
            setting_row(
                ui,
                theme,
                "\u{ec4a}",
                "URL теста скорости",
                "Пусто — стандартный (cachefly 50MB)",
                |ui| {
                    // Right-to-left row: QML order is the field, then the presets.
                    let labels = SPEED_URLS.iter().map(|(_, label)| *label).collect::<Vec<_>>();
                    let selected = SPEED_URLS.iter().position(|(url, _)| *url == s.speed_test_url);
                    if let Some(index) = super::search_combo::preset_menu(ui, theme, "speed-url", &labels, selected) {
                        bridge.set_speed_test_url(SPEED_URLS[index].0);
                    }
                    if let Some(url) = committed_field(
                        ui,
                        theme,
                        "speed-url",
                        &s.speed_test_url,
                        "https://cachefly.cachefly.net/50mb.test",
                        300.0,
                        false,
                    ) {
                        bridge.set_speed_test_url(&url);
                    }
                },
            );
            setting_row(
                ui,
                theme,
                "\u{e8c9}",
                "Параллельных проверок",
                "Сколько серверов тестировать одновременно (0 — авто)",
                |ui| {
                    spin(ui, theme, s.speed_test_concurrency, 0..=32, 1.0, |value| {
                        bridge.set_speed_test_concurrency(value)
                    });
                },
            );
        });
    }
}

fn updates(ui: &mut Ui, theme: &Theme, bridge: &mut AppBridge, s: &AppSettings) {
    card(ui, theme, |ui| {
        ui.spacing_mut().item_spacing.y = 14.0;
        ui.label(
            RichText::new(tr("Обновления", &[]))
                .size(theme.font_strong as f32)
                .family(crate::shell::theme::semibold_family()),
        );
        for (glyph, title, subtitle, value, set) in [
            (
                "\u{e777}",
                "Проверять обновления",
                "Автоматически проверять наличие обновлений",
                s.check_updates,
                AppBridge::set_check_updates as fn(&mut AppBridge, bool),
            ),
            (
                "\u{e72c}",
                "Проверять обновления ядра и geoip/geosite",
                "При запуске проверять обновления и уведомлять",
                s.resource_update_check,
                AppBridge::set_resource_update_check,
            ),
            (
                "\u{e895}",
                "Автообновление приложения",
                "Автоматически скачивать и устанавливать новые версии",
                s.app_auto_update,
                AppBridge::set_app_auto_update,
            ),
        ] {
            setting_row(ui, theme, glyph, title, subtitle, |ui| {
                switch_setting(ui, theme, value, true, |next| set(bridge, next));
            });
        }
    });
}

/// Height-constrained right-aligned row (`QML RowLayout`); egui rows must pin
/// their height or they inherit the scroll viewport's tall rect.
fn right_row<R>(ui: &mut Ui, height: f32, content: impl FnOnce(&mut Ui) -> R) -> R {
    ui.allocate_ui_with_layout(
        vec2(ui.available_width(), height),
        Layout::right_to_left(Align::Center),
        content,
    )
    .inner
}

/// `SettingRow`: glyph + title/subtitle block on the left, controls pinned to
/// the right edge (SettingsPage.qml component).
fn setting_row<R>(
    ui: &mut Ui,
    theme: &Theme,
    glyph: &str,
    title: &str,
    subtitle: &str,
    right: impl FnOnce(&mut Ui) -> R,
) -> R {
    // Title (19) + 2 + subtitle (16) at 100% font scale; a lone title still
    // clears a 32 px control.
    let height = (if subtitle.is_empty() { 32.0 } else { 37.0 }) * (theme.font_scale as f32).max(1.0);
    right_row(ui, height, |ui| {
        let result = right(ui);
        ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 12.0;
            if !glyph.is_empty() {
                ui.allocate_ui_with_layout(
                    vec2(22.0, height),
                    Layout::left_to_right(Align::Center),
                    |ui| {
                        // Keep the full 22 px glyph slot; egui shrinks it to the glyph.
                        ui.set_min_width(22.0);
                        ui.label(
                            RichText::new(glyph)
                                .size(16.0)
                                .color(theme.text_muted),
                        );
                    },
                );
            }
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                if subtitle.is_empty() {
                    // The RowLayout centres a lone title on the row; egui
                    // starts a vertical block at the top.
                    let font = egui::FontId::proportional(theme.font_normal as f32);
                    let title_height = ui.fonts_mut(|fonts| fonts.row_height(&font));
                    ui.add_space(((height - title_height) / 2.0).max(0.0).round());
                }
                ui.label(RichText::new(tr(title, &[])).size(theme.font_normal as f32));
                if !subtitle.is_empty() {
                    ui.label(
                        RichText::new(tr(subtitle, &[]))
                            .size(theme.font_small as f32)
                            .color(theme.text_faint),
                    );
                }
            });
        });
        result
    })
}

fn card(ui: &mut Ui, theme: &Theme, content: impl FnOnce(&mut Ui)) {
    widgets::card_frame(theme, theme.spacing_large as f32)
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            // Card ColumnLayout `spacing: 14`.
            ui.spacing_mut().item_spacing.y = 14.0;
            content(ui);
        });
}

/// 1px `Theme.divider` line.
fn hline(ui: &mut Ui, theme: &Theme, inset: f32) {
    // Allocated directly: a `horizontal` row would pad the line to a full
    // interact height. The inset applies to both ends, as in QML.
    let (rect, _) =
        ui.allocate_exact_size(vec2(ui.available_width().max(0.0), 1.0), Sense::hover());
    ui.painter()
        .rect_filled(rect.shrink2(vec2(inset, 0.0)), 0.0, theme.divider);
}

fn caption(ui: &mut Ui, theme: &Theme, text: &str) {
    ui.label(
        RichText::new(tr(text, &[]))
            .size(theme.font_small as f32)
            .color(theme.text_muted),
    );
}

fn text(text: &str, theme: &Theme) -> RichText {
    RichText::new(tr(text, &[])).size(theme.font_normal as f32)
}

fn tab_job(
    theme: &Theme,
    glyph: &str,
    label: &str,
    current: bool,
    show_label: bool,
    size: f32,
) -> LayoutJob {
    let mut job = LayoutJob::default();
    let glyph_color = if current { theme.accent } else { theme.text_muted };
    job.append(glyph, 0.0, TextFormat::simple(FontId::proportional(size), glyph_color));
    if show_label {
        let color = if current { theme.text } else { theme.text_muted };
        let font = if current { semibold(size) } else { FontId::proportional(size) };
        job.append(label, 8.0, TextFormat::simple(font, color));
    }
    job
}

/// `FluentCombo` bound to a key list; QML combos with an out-of-list value
/// still display their first item, so the fallback mirrors that.
/// Proxy chain `SearchCombo` (260 px); returns the picked node id.
fn chain_combo<'a>(
    ui: &mut Ui,
    theme: &Theme,
    id: &str,
    current: &str,
    options: &[(&'a str, &'a str)],
) -> Option<&'a str> {
    let labels = options.iter().map(|(_, label)| *label).collect::<Vec<_>>();
    let index = options.iter().position(|(key, _)| *key == current);
    super::search_combo::SearchCombo::new(ui, id, labels, index)
        .width(260.0)
        .show(ui, theme)
        .map(|index| options[index].0)
}

fn styled_combo(
    ui: &mut Ui,
    id: impl std::hash::Hash + std::fmt::Debug,
    active: &str,
    options: &[(&str, &str)],
    width: f32,
    set: impl FnOnce(&str),
) {
    // `StyledCombo: FluentCombo`, whose fill fades on hover.
    let Some(theme) = crate::shell::fluent_menu::published_theme(ui.ctx()) else {
        return;
    };
    let labels: Vec<String> = options.iter().map(|(_, caption)| tr(caption, &[])).collect();
    // `boundIndex: Math.max(0, model.indexOf(...))`.
    let current = Some(options.iter().position(|(key, _)| *key == active).unwrap_or(0));
    let enabled = ui.is_enabled();
    if let Some(index) = crate::shell::widgets::combo(ui, &theme, id, width, &labels, current, enabled)
        && let Some(&(key, _)) = options.get(index)
        && key != active
    {
        set(key);
    }
}

/// `FluentSpin`: a 130 x controlHeight editable SpinBox with ‹ / › steppers
/// (26 px wide, 4 px in from each side) and the value centred between them.
/// The field fill follows hover (not while editing) and the border turns
/// accent while editing, both over 110 ms. Steppers, Up / Down while editing
/// and Enter / focus loss commit (`valueModified`).
fn spin(
    ui: &mut Ui,
    theme: &Theme,
    value: i64,
    range: std::ops::RangeInclusive<i64>,
    step: f64,
    set: impl FnOnce(i64),
) {
    let step = (step as i64).max(1);
    let height = theme.control_height as f32;
    let (rect, response) = ui.allocate_exact_size(vec2(130.0, height), Sense::hover());
    let id = response.id;
    let enabled = ui.is_enabled();
    let edit_id = id.with("edit");
    let focused = ui.memory(|memory| memory.has_focus(edit_id));
    let ctx = ui.ctx().clone();
    let hover = ctx.animate_bool_with_time(id.with("hover"), enabled && response.hovered() && !focused, 0.11);
    let focus = ctx.animate_bool_with_time(id.with("focus"), focused, 0.11);
    let radius = theme.radius_small as f32;
    let background = ui.painter().add(egui::Shape::Noop);

    let indicator = |ui: &mut Ui, left: f32, glyph: &str, salt: &str| {
        let button = egui::Rect::from_min_size(egui::pos2(left, rect.center().y - (height - 8.0) / 2.0), vec2(26.0, height - 8.0));
        let response = ui.interact(button, id.with(salt), Sense::click());
        let fill = if !enabled {
            Color32::TRANSPARENT
        } else if response.is_pointer_button_down_on() {
            theme.control_fill_pressed
        } else if response.hovered() {
            theme.control_fill_hover
        } else {
            Color32::TRANSPARENT
        };
        ui.painter().rect_filled(button, radius, fill);
        ui.painter().text(
            button.center(),
            egui::Align2::CENTER_CENTER,
            glyph,
            FontId::proportional(11.0),
            if enabled { theme.text_muted } else { theme.text_faint },
        );
        enabled && response.clicked()
    };
    let down = indicator(ui, rect.left() + 4.0, "\u{e76b}", "down");
    let up = indicator(ui, rect.right() - 30.0, "\u{e76c}", "up");

    let mut text = ui
        .data_mut(|data| data.get_temp::<String>(edit_id))
        .filter(|_| focused)
        .unwrap_or_else(|| value.to_string());
    let edit = ui.place(
        egui::Rect::from_min_max(egui::pos2(rect.left() + 34.0, rect.top()), egui::pos2(rect.right() - 34.0, rect.bottom())),
        TextEdit::singleline(&mut text)
            .id(edit_id)
            .font(FontId::proportional(theme.font_normal as f32))
            .text_color(theme.text)
            .frame(egui::Frame::NONE)
            .margin(egui::Margin::ZERO)
            .align(egui::Align2::CENTER_CENTER),
    );
    let mut next = None;
    if edit.has_focus() {
        let (key_up, key_down) = ui.input_mut(|input| {
            (
                input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp),
                input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown),
            )
        });
        let typed = text.trim().parse::<i64>().unwrap_or(value);
        if key_up {
            next = Some(typed + step);
        } else if key_down {
            next = Some(typed - step);
        }
        if let Some(stepped) = next {
            text = stepped.clamp(*range.start(), *range.end()).to_string();
        }
        ui.data_mut(|data| data.insert_temp(edit_id, text.clone()));
    }
    let committed = edit.lost_focus() || (edit.has_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter)));
    if committed {
        next = Some(text.trim().parse::<i64>().unwrap_or(value));
        ui.data_mut(|data| data.remove::<String>(edit_id));
    }
    if down {
        next = Some(value - step);
    } else if up {
        next = Some(value + step);
    }

    let fill = theme.control_fill.lerp_to_gamma(theme.control_fill_hover, hover);
    let border = theme.border_solid.lerp_to_gamma(theme.accent, focus);
    ui.painter().set(background, egui::Shape::rect_filled(rect, radius, fill));
    ui.painter().rect_stroke(rect, radius, Stroke::new(1.0, border), egui::StrokeKind::Inside);

    if let Some(next) = next.map(|next| next.clamp(*range.start(), *range.end()))
        && next != value
    {
        set(next);
    }
}

/// `PercentSlider`: 170pt slider + right-aligned "N%" label, committing on
/// drag release.
fn percent_slider(
    ui: &mut Ui,
    theme: &Theme,
    id_salt: &str,
    value: i64,
    set: impl FnOnce(i64),
) {
    // RowLayout spacing 8: Slider (170) | "N%" (42, right aligned).
    let id = ui.make_persistent_id(("setting-slider", id_salt));
    let (row, _) = ui.allocate_exact_size(vec2(220.0, 36.0), Sense::hover());
    let slider = egui::Rect::from_min_size(row.min, vec2(170.0, 36.0));
    let response = ui.interact(slider, id, Sense::click_and_drag());
    let enabled = ui.is_enabled();
    // Universal Slider: padding 6, a 2 px track and an 8 x 24 handle that
    // travels over (availableWidth - 8).
    let inner = slider.shrink(6.0);
    let travel = inner.width() - 8.0;
    let mut current = value.clamp(0, 100);
    let pressed = enabled && (response.is_pointer_button_down_on() || response.dragged());
    if pressed && let Some(pointer) = response.interact_pointer_pos() {
        let position = ((pointer.x - inner.left() - 4.0) / travel).clamp(0.0, 1.0);
        // stepSize 5; `onMoved` reports every change.
        current = ((position * 100.0 / 5.0).round() * 5.0) as i64;
    }
    let position = current as f32 / 100.0;
    let hovered = enabled && response.hovered();
    let universal = |dark: u8, light: u8| {
        if theme.dark { Color32::from_gray(dark) } else { Color32::from_gray(light) }
    };
    let track_y = inner.center().y;
    let track = egui::Rect::from_min_max(egui::pos2(inner.left(), track_y - 1.0), egui::pos2(inner.right(), track_y + 1.0));
    let rest = if !enabled {
        universal(0x33, 0xCC)
    } else if hovered && !pressed {
        theme.text.gamma_multiply(0.6)
    } else {
        theme.text.gamma_multiply(0.4)
    };
    ui.painter().rect_filled(track, 0.0, rest);
    let filled = egui::Rect::from_min_size(track.min, vec2(position * inner.width(), 2.0));
    ui.painter().rect_filled(filled, 0.0, if enabled { theme.accent } else { universal(0x33, 0xCC) });
    let handle = egui::Rect::from_min_size(
        egui::pos2(inner.left() + position * travel, inner.center().y - 12.0),
        vec2(8.0, 24.0),
    );
    let handle_fill = if !enabled {
        universal(0x33, 0xCC)
    } else if pressed {
        // chromeHighColor.
        universal(0x76, 0xCC)
    } else if hovered {
        // chromeAltLowColor.
        universal(0xF2, 0x17)
    } else {
        theme.accent
    };
    ui.painter().rect_filled(handle, 4.0, handle_fill);
    ui.painter().text(
        egui::pos2(row.right(), row.center().y),
        egui::Align2::RIGHT_CENTER,
        format!("{current}%"),
        FontId::proportional(theme.font_small as f32),
        theme.text_muted,
    );
    if current != value {
        set(current);
    }
}

/// QML `Switch` as a 44×20 Fluent pill.
/// Universal `Switch` (impl/SwitchIndicator.qml): 44x20 track, 10 px knob.
/// Colours are Universal base colours, i.e. the text colour at 20 % (baseLow),
/// 60 % (baseMedium), 80 % (baseMediumHigh) and 100 % (baseHigh).
pub(crate) fn switch_widget(ui: &mut Ui, theme: &Theme, checked: bool, enabled: bool) -> Option<bool> {
    // The Universal `Switch` keeps 8 px of padding after its 44x20 track.
    let (slot, mut response) = ui.allocate_exact_size(vec2(52.0, 20.0), Sense::click());
    let rect = egui::Rect::from_min_size(slot.min, vec2(44.0, 20.0));
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::Role::CheckBox, enabled, checked, "")
    });
    let mut value = None;
    if enabled && response.clicked() {
        value = Some(!checked);
        response.mark_changed();
    }
    let base = |opacity: f32| theme.text.gamma_multiply(opacity);
    let pressed = enabled && response.is_pointer_button_down_on();
    let hovered = enabled && response.hovered();
    let fill = if !enabled {
        Color32::TRANSPARENT
    } else if pressed {
        base(0.6)
    } else if checked {
        // Hovered and checked: the track drops to 0.9 opacity (dark theme)
        // or 0.7 (light).
        let level = if hovered { if theme.dark { 0.9 } else { 0.7 } } else { 1.0 };
        theme.accent.gamma_multiply(level)
    } else {
        Color32::TRANSPARENT
    };
    let border = if !enabled {
        base(0.2)
    } else if checked && !pressed {
        Color32::TRANSPARENT
    } else if hovered && !checked && !pressed {
        base(1.0)
    } else {
        base(0.6)
    };
    ui.painter().rect(rect, 10.0, fill, Stroke::new(2.0, border), egui::StrokeKind::Inside);
    let knob_x = rect.left()
        + motion::follow(
            ui.ctx(),
            theme,
            response.id.with("knob"),
            if checked { rect.width() - 10.0 } else { 10.0 },
            120.0,
            motion::ease_out_cubic,
        );
    let knob = if !enabled {
        base(0.2)
    } else if pressed || checked {
        // chromeWhiteColor.
        Color32::WHITE
    } else if hovered {
        base(1.0)
    } else {
        base(0.8)
    };
    ui.painter()
        .circle_filled(egui::pos2(knob_x, rect.center().y), 5.0, knob);
    value
}

fn switch_setting(
    ui: &mut Ui,
    theme: &Theme,
    value: bool,
    enabled: bool,
    set: impl FnOnce(bool),
) {
    if let Some(next) = switch_widget(ui, theme, value, enabled) {
        set(next);
    }
}

/// `AccentPicker`: 56pt accent swatch that opens the colour dialog.
fn accent_picker(ui: &mut Ui, theme: &Theme, color: Color32) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(vec2(56.0, theme.control_height as f32), Sense::click());
    ui.painter().rect(
        rect,
        theme.radius_small as u8,
        color,
        Stroke::new(
            1.0,
            if response.hovered() { theme.text } else { theme.divider },
        ),
        egui::StrokeKind::Inside,
    );
    response
}

/// `BaseTintPicker`: swatch showing "—" while no custom tint is set.
fn base_tint_picker(ui: &mut Ui, theme: &Theme, color: Option<Color32>) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(vec2(56.0, theme.control_height as f32), Sense::click());
    let fill = color.unwrap_or(Color32::TRANSPARENT);
    ui.painter().rect(
        rect,
        theme.radius_small as u8,
        fill,
        Stroke::new(
            1.0,
            if response.hovered() { theme.text } else { theme.divider },
        ),
        egui::StrokeKind::Inside,
    );
    if color.is_none() {
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            "\u{2014}",
            FontId::proportional(theme.font_normal as f32),
            theme.text_faint,
        );
    }
    response
}

fn ghost(ui: &mut Ui, theme: &Theme, label: &str) -> bool {
    fluent_button::style(ui);
    ui.add(fluent_button::button(theme, "", &tr(label, &[]), false))
        .clicked()
}

fn ghost_glyph(ui: &mut Ui, theme: &Theme, glyph: &str, label: &str) -> bool {
    fluent_button::style(ui);
    ui.add(fluent_button::button(theme, glyph, &tr(label, &[]), false))
        .clicked()
}

fn accent_button(ui: &mut Ui, theme: &Theme, label: &str) -> bool {
    fluent_button::style(ui);
    ui.add(fluent_button::button(theme, "", &tr(label, &[]), true))
        .clicked()
}

fn accent_button_glyph(ui: &mut Ui, theme: &Theme, glyph: &str, label: &str) -> bool {
    fluent_button::style(ui);
    ui.add(fluent_button::button(theme, glyph, &tr(label, &[]), true))
        .clicked()
}

fn danger_button<'t>(theme: &'t Theme, label: &str) -> fluent_button::PageButton<'t> {
    fluent_button::button(theme, "\u{e777}", &tr(label, &[]), true).danger()
}

/// `InfoIcon`: muted ⓘ with the QML tooltip text.
fn info_icon(ui: &mut Ui, theme: &Theme, tip: &str) {
    // `InfoIcon`: a 28 px ToolButton that gets the `controlFillHover`
    // background and the full text colour on hover; its tip waits 250 ms.
    let (rect, response) = ui.allocate_exact_size(vec2(28.0, 28.0), Sense::hover());
    if response.hovered() {
        ui.painter().rect_filled(rect, theme.radius_small as f32, theme.control_fill_hover);
    }
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        "\u{e946}",
        FontId::proportional(14.0),
        if response.hovered() { theme.text } else { theme.text_muted },
    );
    response.fluent_tip_after(tr(tip, &[]), 0.25);
}

/// Proxy-chain hop pill (`Rectangle radius 10, height 22`).
fn chip(ui: &mut Ui, theme: &Theme, label: &str, fill: Color32) {
    let galley = ui.painter().layout_no_wrap(
        label.to_string(),
        FontId::proportional(theme.font_small as f32),
        theme.text,
    );
    let width = (galley.size().x + 18.0).min(220.0).max(24.0);
    let (rect, _) = ui.allocate_exact_size(vec2(width, 22.0), Sense::hover());
    ui.painter().rect(
        rect,
        10.0,
        fill,
        Stroke::new(1.0, theme.border_solid),
        egui::StrokeKind::Inside,
    );
    ui.painter().galley(
        egui::pos2(
            rect.center().x - galley.size().x / 2.0,
            rect.center().y - galley.size().y / 2.0,
        ),
        galley,
        theme.text,
    );
}

/// Implicit width of a `ColumnLayout` of labels and `StyledArea`s: the widest
/// label, or the widest text line (or placeholder) plus the 10 + 10 padding.
fn column_preferred_width(
    ui: &Ui,
    labels: &[(&str, &egui::FontId)],
    areas: &[(&str, &str)],
    area_font: &egui::FontId,
) -> f32 {
    let width = |text: &str, font: &egui::FontId| {
        text.lines()
            .map(|line| ui.fonts_mut(|fonts| fonts.layout_no_wrap(line.to_owned(), font.clone(), Color32::WHITE).size().x))
            .fold(0.0_f32, f32::max)
    };
    let labels = labels.iter().map(|(text, font)| width(text, font)).fold(0.0_f32, f32::max);
    let areas = areas
        .iter()
        .map(|(text, hint)| width(text, area_font).max(width(hint, area_font)) + 20.0)
        .fold(0.0_f32, f32::max);
    labels.max(areas)
}

/// Two `fillWidth` ColumnLayouts in a GridLayout (`columnSpacing: 12`,
/// `rowSpacing: 12`; each column keeps the default `spacing: 5`). Qt shares
/// the width in proportion to the columns' implicit widths, so the column
/// with the longer text is wider. Narrow pages stack them in one column.
fn layout_columns(
    ui: &mut Ui,
    wide: bool,
    preferred: [f32; 2],
    left: impl FnOnce(&mut Ui),
    right: impl FnOnce(&mut Ui),
) {
    if !wide {
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 5.0;
            left(ui);
            ui.add_space(12.0 - 5.0);
            right(ui);
        });
        return;
    }
    let width = ui.available_width();
    let free = (width - 12.0).max(0.0);
    let total = (preferred[0] + preferred[1]).max(1.0);
    let left_width = (free * preferred[0] / total).round();
    let right_width = free - left_width;
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 12.0;
        let column = |ui: &mut Ui, column_width: f32, content: &mut dyn FnMut(&mut Ui)| {
            ui.allocate_ui_with_layout(vec2(column_width, 0.0), Layout::top_down(Align::LEFT), |ui| {
                ui.set_width(column_width);
                ui.spacing_mut().item_spacing.y = 5.0;
                content(ui);
            });
        };
        let mut left = Some(left);
        let mut right = Some(right);
        column(ui, left_width, &mut |ui| (left.take().unwrap())(ui));
        column(ui, right_width, &mut |ui| (right.take().unwrap())(ui));
    });
}

/// `StyledField`: `Theme.controlHeight` tall, 10 px side padding,
/// `Theme.card` fill (egui's default is the much darker extreme background).
fn styled_field<'t>(theme: &Theme, edit: TextEdit<'t>, hint: &str, width: f32) -> TextEdit<'t> {
    edit.font(egui::FontId::proportional(theme.font_normal as f32))
        .hint_text(hint)
        .background_color(theme.card)
        .desired_width(width - 20.0)
        .min_size(vec2(width, theme.control_height as f32))
        .margin(egui::Margin::symmetric(10, 0))
        .vertical_align(Align::Center)
}

/// `StyledArea`: fill-width multiline field with the QML fixed height,
/// 10 / 8 px padding and the `Theme.card` fill.
fn styled_area(ui: &mut Ui, theme: &Theme, value: &mut String, hint: &str, height: f32) {
    let width = ui.available_width().max(120.0);
    ui.allocate_ui_with_layout(vec2(width, height), Layout::top_down(Align::LEFT), |ui| {
        ui.add(
            TextEdit::multiline(value)
                .font(egui::FontId::proportional(theme.font_normal as f32))
                .hint_text(hint)
                .background_color(theme.card)
                .desired_width(width - 20.0)
                .min_size(vec2(width, height))
                .margin(egui::Margin::symmetric(10, 8)),
        );
    });
}

/// `StyledField` bound to a draft saved by an explicit button.
fn plain_field(ui: &mut Ui, theme: &Theme, value: &mut String, hint: &str, width: f32) {
    ui.add(styled_field(theme, TextEdit::singleline(value), hint, width));
}

/// `StyledField` with Qt `editingFinished` semantics (Enter or focus loss).
fn committed_field(
    ui: &mut Ui,
    theme: &Theme,
    id_salt: impl core::hash::Hash + core::fmt::Debug,
    current: &str,
    hint: &str,
    width: f32,
    password: bool,
) -> Option<String> {
    let id = ui.make_persistent_id(("settings-text", id_salt));
    let mut value = ui
        .data_mut(|data| data.get_temp::<String>(id))
        .unwrap_or_else(|| current.into());
    let response = ui.add(styled_field(
        theme,
        TextEdit::singleline(&mut value).id(id).password(password),
        hint,
        width,
    ));
    let committed = response.lost_focus()
        || (response.has_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter)));
    if committed {
        ui.data_mut(|data| data.remove::<String>(id));
        (value != current).then_some(value)
    } else {
        if response.has_focus() {
            ui.data_mut(|data| data.insert_temp(id, value));
        }
        None
    }
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

    fn frames(transition: &mut TabTransition, displayed: &mut usize, tab: usize, from: f64, to: f64) -> (f32, f32) {
        let mut last = (1.0, 0.0);
        let mut now = from;
        while now <= to + 1e-9 {
            last = transition.advance(now, tab, displayed, true, 1000.0);
            now += 0.005;
        }
        last
    }

    #[test]
    fn tab_click_during_the_exit_only_changes_the_destination() {
        let mut transition = TabTransition::default();
        let mut displayed = 0;
        frames(&mut transition, &mut displayed, 0, 0.0, 0.0);
        frames(&mut transition, &mut displayed, 2, 0.01, 0.05);
        assert_eq!(displayed, 0);
        // A second click mid-exit: tab 2 never becomes visible.
        frames(&mut transition, &mut displayed, 4, 0.055, 0.095);
        assert_eq!(displayed, 0);
        frames(&mut transition, &mut displayed, 4, 0.1, 0.12);
        assert_eq!(displayed, 4);
        let (opacity, x) = frames(&mut transition, &mut displayed, 4, 0.125, 0.5);
        assert_eq!((opacity, x), (1.0, 0.0));
        assert!(!transition.running());
    }

    #[test]
    fn tab_click_during_the_enter_starts_a_new_exit_from_the_current_values() {
        let mut transition = TabTransition::default();
        let mut displayed = 0;
        frames(&mut transition, &mut displayed, 0, 0.0, 0.0);
        let (opacity, x) = frames(&mut transition, &mut displayed, 3, 0.01, 0.14);
        assert_eq!(displayed, 3);
        assert!(opacity < 1.0 && x > 0.0, "mid-enter: {opacity} {x}");
        // Click back while the page is still sliding in.
        let (next_opacity, next_x) = frames(&mut transition, &mut displayed, 1, 0.145, 0.145);
        assert_eq!(displayed, 3, "the old page exits first");
        assert!((next_opacity - opacity).abs() < 0.05 && (next_x - x).abs() < 3.0, "no jump");
        frames(&mut transition, &mut displayed, 1, 0.15, 0.6);
        assert_eq!(displayed, 1);
        assert!(!transition.running());
    }

    #[test]
    fn tab_click_on_the_displayed_tab_cancels_the_exit() {
        let mut transition = TabTransition::default();
        let mut displayed = 0;
        frames(&mut transition, &mut displayed, 0, 0.0, 0.0);
        frames(&mut transition, &mut displayed, 2, 0.01, 0.04);
        let result = frames(&mut transition, &mut displayed, 0, 0.045, 0.045);
        assert_eq!((displayed, result), (0, (1.0, 0.0)));
        assert!(!transition.running());
    }
}



