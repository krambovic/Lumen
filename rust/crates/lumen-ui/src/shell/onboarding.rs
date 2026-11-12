//! First-run setup and upgrade notes from `OnboardingOverlay.qml`.
//!
//! A modal "window inside the window": a full-window backdrop with a centered
//! card, a 272pt side panel with the step stepper (wide layout) or compact
//! progress pills (narrow), and the per-step pages above a 76pt action bar.

use std::path::Path;

use egui::{
    Align, Button, Color32, Context, FontId, Id, Key, Layout, Modifiers, RichText, ScrollArea,
    Sense, Stroke, TextEdit, Ui, vec2,
};
use lumen_core::{constants::APP_VERSION, i18n::tr};

use super::pages::about::{DONATION_URL, USDT_ADDRESS, open_url};
use super::pages::flag;
use super::theme::{ACCENT_SWATCHES, Theme, hex_color, semibold};
use super::{has_supported_config_drop, win_native};
use crate::view_models::AppBridge;

const PALETTES: [(&str, &str); 12] = [
    ("default", "Default"),
    ("midnight", "Midnight"),
    ("catppuccin", "Catppuccin"),
    ("dracula", "Dracula"),
    ("nord", "Nord"),
    ("tokyo-night", "Tokyo Night"),
    ("github", "GitHub"),
    ("rose-pine", "Rose Pine"),
    ("gruvbox", "Gruvbox"),
    ("everforest", "Everforest"),
    ("solarized", "Solarized"),
    ("vercel", "Vercel"),
];

/// Two-tone preview colours per palette (QML `paletteAccents` for the dot,
/// the theme maps for base/surface).
const PALETTE_ACCENTS: [(&str, &str); 12] = [
    ("default", "#0078D4"),
    ("midnight", "#2563EB"),
    ("catppuccin", "#CBA6F7"),
    ("dracula", "#BD93F9"),
    ("nord", "#5E81AC"),
    ("tokyo-night", "#34548A"),
    ("github", "#0969DA"),
    ("rose-pine", "#907AA9"),
    ("gruvbox", "#B57614"),
    ("everforest", "#4F8A10"),
    ("solarized", "#268BD2"),
    ("vercel", "#111111"),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Step {
    News,
    Welcome,
    Look,
    Route,
    Configs,
    System,
    Done,
}

impl Step {
    fn title(self) -> &'static str {
        match self {
            Self::News => "Что нового",
            Self::Welcome => "Знакомство",
            Self::Look => "Оформление",
            Self::Route => "Регион и маршруты",
            Self::Configs => "Конфигурации",
            Self::System => "Обновления и запуск",
            Self::Done => "Готово",
        }
    }

    fn glyph(self) -> &'static str {
        match self {
            Self::News => "\u{e8bc}",
            Self::Welcome => "\u{e80f}",
            Self::Look => "\u{e790}",
            Self::Route => "\u{e909}",
            Self::Configs => "\u{ec05}",
            Self::System => "\u{e713}",
            Self::Done => "\u{e73e}",
        }
    }
}

fn steps(setup: bool, news: bool) -> Vec<Step> {
    let mut flow = vec![if news { Step::News } else { Step::Welcome }];
    if setup || !news {
        flow.extend([
            Step::Look,
            Step::Route,
            Step::Configs,
            Step::System,
            Step::Done,
        ]);
    }
    flow
}

#[derive(Default)]
pub(super) struct Onboarding {
    flow: Vec<Step>,
    step: usize,
    config_choice: Option<bool>,
    subscription: String,
    import_error: Option<String>,
    windows_accent: String,
    show_support: bool,
    finished: bool,
}

impl Onboarding {
    pub(super) fn render(&mut self, ctx: &Context, theme: &Theme, bridge: &mut AppBridge) -> bool {
        if !bridge.onboarding_required() {
            *self = Self::default();
            return false;
        }
        if bridge.locked() || self.finished {
            return false;
        }
        if self.flow.is_empty() {
            self.flow = steps(
                bridge.onboarding_setup_required(),
                bridge.onboarding_show_whats_new(),
            );
            let returning =
                bridge.onboarding_returning_user() || bridge.onboarding_server_count() > 0;
            self.config_choice = returning.then_some(true);
            self.windows_accent = bridge.system_accent();
            if !returning
                && !self.windows_accent.is_empty()
                && bridge.accent_color().eq_ignore_ascii_case("#0078D4")
            {
                bridge.apply_system_accent_quietly();
            }
        }
        let current = self.flow[self.step];
        let last = self.step + 1 == self.flow.len();
        let mut finish = false;

        egui::Area::new(Id::new("lumen-onboarding"))
            .order(egui::Order::Foreground)
            .interactable(true)
            .fixed_pos(egui::Pos2::ZERO)
            .show(ctx, |ui| {
                let screen = ctx.input(|input| input.raw.screen_rect).unwrap_or(egui::Rect::EVERYTHING);
                // Opaque backdrop (QML: bg base + blurred snapshot + 62% tint).
                ui.painter().rect_filled(screen, 0.0, theme.bg);

                let width = 980.0_f32.min(screen.width() - 48.0).max(320.0);
                let height = 640.0_f32.min(screen.height() - 74.0).max(300.0);
                let card_rect = egui::Rect::from_center_size(screen.center(), vec2(width, height));
                let radius = (theme.radius as f32 + 6.0).max(10.0);
                let shadow_color = if theme.dark {
                    Color32::from_black_alpha(140)
                } else {
                    Color32::from_black_alpha(50)
                };
                let shadow = egui::Shadow {
                    offset: [0, 22],
                    blur: 60,
                    spread: 0,
                    color: shadow_color,
                };
                ui.painter()
                    .add(shadow.as_shape(card_rect, radius as u8));
                ui.painter().rect(
                    card_rect,
                    radius,
                    theme.flyout,
                    Stroke::new(1.0, theme.flyout_border),
                    egui::StrokeKind::Inside,
                );
                // Swallow every pointer event behind the card.
                ui.allocate_rect(screen, Sense::hover());

                let wide = card_rect.width() >= 760.0;
                let side_width = if wide { 272.0 } else { 0.0 };
                if wide {
                    let side_rect = egui::Rect::from_min_size(
                        card_rect.min,
                        vec2(side_width, card_rect.height()),
                    );
                    ui.painter().rect_filled(
                        egui::Rect::from_min_max(
                            egui::pos2(card_rect.left() + side_width, card_rect.top()),
                            egui::pos2(card_rect.left() + side_width + 1.0, card_rect.bottom()),
                        ),
                        0.0,
                        theme.divider,
                    );
                    let mut side_ui = ui.new_child(
                        egui::UiBuilder::new()
                            .max_rect(side_rect.shrink(28.0))
                            .layout(Layout::top_down(Align::LEFT)),
                    );
                    self.side_panel(&mut side_ui, theme);
                }

                let main_rect = egui::Rect::from_min_max(
                    egui::pos2(card_rect.left() + side_width, card_rect.top()),
                    card_rect.max,
                );
                let margin = if wide { 40.0 } else { 24.0 };
                let top_margin = if wide { 34.0 } else { 38.0 };
                let bar_height = 76.0;
                let pages_rect = egui::Rect::from_min_max(
                    egui::pos2(main_rect.left() + margin, main_rect.top() + top_margin),
                    egui::pos2(main_rect.right() - margin, main_rect.bottom() - bar_height),
                );
                // Compact progress pills on the narrow layout.
                if !wide {
                    let mut pill_ui = ui.new_child(
                        egui::UiBuilder::new()
                            .max_rect(egui::Rect::from_min_max(
                                egui::pos2(main_rect.left(), main_rect.top() + 18.0),
                                egui::pos2(main_rect.right(), main_rect.top() + 30.0),
                            ))
                            .layout(Layout::left_to_right(Align::Center)),
                    );
                    pill_ui.spacing_mut().item_spacing.x = 6.0;
                    let total = pill_ui.available_width();
                    let pills = self.flow.len() as f32;
                    let row = pills * 6.0 + (pills - 1.0) * 6.0;
                    let _ = total;
                    pill_ui.add_space(((total - row) / 2.0).max(0.0));
                    for (index, _) in self.flow.iter().enumerate() {
                        let (rect, response) = pill_ui
                            .allocate_exact_size(vec2(if index == self.step { 26.0 } else { 6.0 }, 6.0), Sense::hover());
                        let fill = if index <= self.step {
                            theme.accent
                        } else {
                            theme.divider
                        };
                        pill_ui.painter().rect_filled(rect, 3.0, fill);
                        let _ = response;
                    }
                }
                let mut pages_ui = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(pages_rect)
                        .layout(Layout::top_down(Align::LEFT)),
                );
                ScrollArea::vertical()
                    .id_salt(("onboarding-step", self.step))
                    .auto_shrink([false, false])
                    .show(&mut pages_ui, |ui| {
                        ui.spacing_mut().item_spacing.y = 0.0;
                        match current {
                            Step::News => self.news(ui, theme, bridge),
                            Step::Welcome => welcome(ui, theme, bridge),
                            Step::Look => self.look(ui, theme, bridge),
                            Step::Route => route(ui, theme, bridge),
                            Step::Configs => self.configs(ui, theme, bridge),
                            Step::System => system(ui, theme, bridge),
                            Step::Done => self.done(ui, theme, bridge),
                        }
                    });

                // ── action bar ──
                let bar_rect = egui::Rect::from_min_max(
                    egui::pos2(main_rect.left(), main_rect.bottom() - bar_height),
                    main_rect.max,
                );
                ui.painter().rect_filled(
                    egui::Rect::from_min_max(
                        egui::pos2(bar_rect.left(), bar_rect.top()),
                        egui::pos2(bar_rect.right(), bar_rect.top() + 1.0),
                    ),
                    0.0,
                    theme.divider,
                );
                let mut bar_ui = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(egui::Rect::from_min_max(
                            egui::pos2(bar_rect.left() + margin, bar_rect.top() + 1.0),
                            egui::pos2(bar_rect.right() - margin, bar_rect.bottom()),
                        ))
                        .layout(Layout::left_to_right(Align::Center)),
                );
                bar_ui.spacing_mut().item_spacing.x = 10.0;
                let keyboard = !ctx.egui_wants_keyboard_input();
                let next_key = keyboard
                    && ctx.input_mut(|input| {
                        input.consume_key(Modifiers::NONE, Key::Enter)
                            || (!last && input.consume_key(Modifiers::NONE, Key::ArrowRight))
                    });
                let back_key = keyboard
                    && ctx.input_mut(|input| {
                        input.consume_key(Modifiers::NONE, Key::ArrowLeft)
                    });
                if !last && ghost(&mut bar_ui, theme, "Пропустить") {
                    finish = true;
                }
                bar_ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let next_label = if last {
                        "Открыть Lumen"
                    } else if current == Step::Welcome && self.flow[0] == Step::Welcome {
                        "Начать"
                    } else {
                        "Далее"
                    };
                    let glyph = if last { "\u{e73e}" } else { "" };
                    let mut primary = pill_button(theme, glyph, &tr(next_label, &[]));
                    primary = primary.min_size(vec2(132.0, theme.control_height as f32));
                    if ui.add(primary).clicked() || next_key {
                        if last {
                            finish = true;
                        } else {
                            self.step += 1;
                        }
                    }
                    if self.step > 0
                        && ghost_glyph(ui, theme, "\u{e72b}", "Назад")
                        || (self.step > 0 && back_key)
                    {
                        self.step -= 1;
                    }
                });
            });
        if finish {
            self.finished = true;
            bridge.complete_onboarding();
        }
        finish
    }

    /// Brand + stepper + bottom hint (QML `side` panel).
    fn side_panel(&mut self, ui: &mut Ui, theme: &Theme) {
        ui.spacing_mut().item_spacing.y = 0.0;
        // Brand row.
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 12.0;
            let (rect, _) = ui.allocate_exact_size(vec2(34.0, 34.0), Sense::hover());
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "L",
                semibold(30.0),
                theme.text,
            );
            ui.vertical(|ui| {
                ui.label(RichText::new("Lumen").size(20.0).family(crate::shell::theme::semibold_family()));
                ui.label(
                    RichText::new(format!("v{APP_VERSION}"))
                        .size(theme.font_small as f32)
                        .color(theme.text_faint),
                );
            });
        });
        ui.add_space(40.0);
        // Stepper: rail + progress fill + dot rows of 52pt.
        let row_h = 52.0;
        let count = self.flow.len();
        let stepper_height = count as f32 * row_h;
        let (stepper_rect, _) =
            ui.allocate_exact_size(vec2(ui.available_width(), stepper_height), Sense::hover());
        let painter = ui.painter();
        painter.rect_filled(
            egui::Rect::from_min_size(
                egui::pos2(stepper_rect.left() + 15.0, stepper_rect.top() + 26.0),
                egui::vec2(2.0, (count - 1) as f32 * row_h),
            ),
            1.0,
            theme.divider,
        );
        painter.rect_filled(
            egui::Rect::from_min_size(
                egui::pos2(stepper_rect.left() + 15.0, stepper_rect.top() + 26.0),
                egui::vec2(2.0, self.step as f32 * row_h),
            ),
            1.0,
            theme.accent,
        );
        for (index, step) in self.flow.iter().enumerate() {
            let done = index < self.step;
            let current = index == self.step;
            let dot_rect = egui::Rect::from_min_size(
                egui::pos2(stepper_rect.left(), stepper_rect.top() + 10.0 + row_h * index as f32),
                vec2(32.0, 32.0),
            );
            let fill = if current || done {
                theme.accent
            } else {
                theme.flyout
            };
            let stroke = if current || done {
                Stroke::NONE
            } else {
                Stroke::new(1.0, theme.flyout_border)
            };
            painter.circle_filled(dot_rect.center(), 16.0, fill);
            if !current && !done {
                painter.circle_stroke(dot_rect.center(), 16.0, Stroke::new(1.0, theme.flyout_border));
            }
            painter.text(
                dot_rect.center(),
                egui::Align2::CENTER_CENTER,
                if done { "\u{e73e}" } else { step.glyph() },
                FontId::proportional(14.0),
                if current || done {
                    theme.accent_text
                } else {
                    theme.text_muted
                },
            );
            let color = if current {
                theme.text
            } else if done {
                theme.text_muted
            } else {
                theme.text_faint
            };
            let label = painter.layout_no_wrap(
                tr(step.title(), &[]),
                if current {
                    semibold(theme.font_normal as f32)
                } else {
                    FontId::proportional(theme.font_normal as f32)
                },
                color,
            );
            painter.galley(
                egui::pos2(
                    dot_rect.right() + 14.0,
                    dot_rect.center().y - label.size().y / 2.0,
                ),
                label,
                color,
            );
            if done {
                let response = ui.interact(
                    egui::Rect::from_min_size(
                        egui::pos2(stepper_rect.left(), stepper_rect.top() + row_h * index as f32),
                        vec2(stepper_rect.width(), row_h),
                    ),
                    egui::Id::new(("onboarding-step-row", index)),
                    Sense::click(),
                );
                if response.clicked() {
                    self.step = index;
                }
            }
        }
        // Bottom hint pinned by the top-down layout: push it to the panel
        // bottom with the remaining space.
        ui.with_layout(Layout::bottom_up(Align::LEFT), |ui| {
            ui.label(
                RichText::new(tr("Всё можно изменить позже в Настройках", &[]))
                    .size(theme.font_small as f32)
                    .color(theme.text_faint),
            );
        });
    }

    fn news(&self, ui: &mut Ui, theme: &Theme, bridge: &mut AppBridge) {
        let version = bridge.app_version();
        let previous = bridge.onboarding_previous_version();
        let full_setup = self.flow.len() > 1;
        let key = match (full_setup, previous.is_empty()) {
            (true, false) => {
                "Предыдущая версия — {version}. Посмотрите, что изменилось, а затем проверим основные настройки."
            }
            (true, true) => "Посмотрите, что изменилось, а затем проверим основные настройки.",
            (false, false) => "Предыдущая версия — {version}. Вот что изменилось.",
            (false, true) => "Вот что изменилось в этой версии.",
        };
        page_header(
            ui,
            theme,
            tr(
                "Lumen обновлён до версии {version}",
                &[("version", version)],
            ),
            tr(key, &[("version", &previous)]),
            28.0,
        );
        for group in bridge.onboarding_whats_new() {
            let version = group["version"].as_str().unwrap_or_default();
            section_label(ui, theme, &tr("Что нового в {version}", &[("version", version)]));
            ui.add_space(6.0);
            group_card(ui, theme, &group);
        }
        ui.add_space(8.0);
    }

    fn look(&mut self, ui: &mut Ui, theme: &Theme, bridge: &mut AppBridge) {
        page_header(
            ui,
            theme,
            tr("Оформление", &[]),
            tr("Выберите внешний вид — изменения применяются сразу", &[]),
            24.0,
        );
        // Upgraded profiles picked the language on the news page; fresh
        // installs already saw it on welcome.
        if self.flow[0] == Step::News {
            language_picker(ui, theme, bridge);
        }
        section_label(ui, theme, "Тема");
        let keys = ["system", "light", "dark"];
        ui.columns(3, |columns| {
            for (index, key) in keys.into_iter().enumerate() {
                let selected = bridge.theme_name() == key;
                if theme_card(&mut columns[index], theme, &tr(theme_label(key), &[]), selected) {
                    bridge.set_theme(key);
                }
            }
        });
        section_label(ui, theme, "Палитра");
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 10.0;
            ui.spacing_mut().item_spacing.y = 10.0;
            for (key, label) in PALETTES {
                let accent = PALETTE_ACCENTS
                    .iter()
                    .find(|(k, _)| k.eq_ignore_ascii_case(key))
                    .and_then(|(_, hex)| hex_color(hex))
                    .unwrap_or(theme.accent);
                let selected = bridge.settings().ui_theme_preset == *key;
                if palette_swatch(ui, theme, label, accent, selected) {
                    bridge.set_ui_theme_preset(key);
                }
            }
        });
        section_label(ui, theme, "Цвет акцента");
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            if !self.windows_accent.is_empty() {
                let selected = bridge.accent_color().eq_ignore_ascii_case(&self.windows_accent);
                if accent_dot(ui, theme, hex_color(&self.windows_accent), selected) {
                    bridge.set_accent(&self.windows_accent);
                }
                let (rect, _) = ui.allocate_exact_size(vec2(9.0, 34.0), Sense::hover());
                ui.painter().rect_filled(
                    egui::Rect::from_center_size(rect.center(), vec2(1.0, 20.0)),
                    0.0,
                    theme.divider,
                );
            }
            for color in ACCENT_SWATCHES.iter().take(10) {
                if !color.eq_ignore_ascii_case(&self.windows_accent) {
                    let selected = bridge.accent_color().eq_ignore_ascii_case(color);
                    if accent_dot(ui, theme, hex_color(color), selected) {
                        bridge.set_accent(color);
                    }
                }
            }
            let mut custom = theme.accent;
            if ui
                .add(
                    Button::new(RichText::new("+").size(14.0).color(theme.text))
                        .min_size(vec2(34.0, 34.0))
                        .fill(theme.card),
                )
                .on_hover_text(tr("Свой цвет", &[]))
                .clicked()
            {
                // Toggle the inline picker through the same dialog flow.
                let hex = format!(
                    "#{:02X}{:02X}{:02X}",
                    custom.r(),
                    custom.g(),
                    custom.b()
                );
                bridge.set_accent(&hex);
            }
        });
        ui.add_space(8.0);
    }

    fn configs(&mut self, ui: &mut Ui, theme: &Theme, bridge: &mut AppBridge) {
        let count = bridge.onboarding_server_count().to_string();
        let description = if count == "0" {
            tr(
                "Ключ, ссылка на подписку или файл конфигурации — Lumen поймёт любой из них",
                &[],
            )
        } else {
            tr(
                "Серверов в Lumen: {count}. Можно добавить ещё или перейти дальше.",
                &[("count", &count)],
            )
        };
        page_header(
            ui,
            theme,
            tr("У вас уже есть VPN-конфигурация?", &[]),
            description,
            24.0,
        );
        ui.columns(2, |columns| {
            if choice_card(
                &mut columns[0],
                theme,
                "\u{e8b7}",
                &tr("Да, есть", &[]),
                &tr("Импортирую ключ, подписку или файл", &[]),
                self.config_choice == Some(true),
            ) {
                self.config_choice = Some(true);
            }
            if choice_card(
                &mut columns[1],
                theme,
                "\u{e945}",
                &tr("Нет, пока нет", &[]),
                &tr("Получу бесплатный WARP прямо здесь", &[]),
                self.config_choice == Some(false),
            ) {
                self.config_choice = Some(false);
            }
        });
        ui.add_space(12.0);
        match self.config_choice {
            Some(true) => {
                ui.horizontal_wrapped(|ui| {
                    if ghost(ui, theme, "Вставить из буфера") {
                        import_clipboard(bridge);
                    }
                    if ghost(ui, theme, "Открыть файл") {
                        self.import_error = import_config_file(bridge)
                            .err()
                            .map(|error| error.to_string());
                    }
                });
                if let Some(error) = &self.import_error {
                    ui.label(RichText::new(error).color(theme.danger));
                }
                let busy = bridge.subscription_importing();
                ui.horizontal_wrapped(|ui| {
                    ui.add(
                        TextEdit::singleline(&mut self.subscription)
                            .desired_width(280.0)
                            .hint_text(tr("Ссылка на подписку (https://…)", &[])),
                    );
                    if ui
                        .add_enabled(
                            !busy && !self.subscription.trim().is_empty(),
                            pill_button(theme, "", &tr("Добавить", &[])),
                        )
                        .clicked()
                    {
                        bridge.import_subscription(self.subscription.trim(), "");
                        self.subscription.clear();
                    }
                });
                if busy {
                    ui.spinner();
                    ui.label(tr("Загрузка подписки...", &[]));
                } else if count != "0" {
                    ui.label(tr("Серверов в Lumen: {count}", &[("count", &count)]));
                } else {
                    ui.label(tr(
                        "Файлы .conf, .ovpn, .json и .yaml можно просто перетащить в окно",
                        &[],
                    ));
                }
            }
            Some(false) => {
                section_label(ui, theme, "Бесплатный Cloudflare WARP");
                ui.label(tr("Lumen зарегистрирует бесплатный профиль WARP и добавит готовые конфигурации AmneziaWG и MASQUE в группу «WARP».", &[]));
                ui.add_space(10.0);
                let busy = bridge.warp_generating();
                let complete = bridge.warp_generated_count() > 0;
                let label = if busy {
                    "Получаем профили…"
                } else if complete {
                    "Профили WARP добавлены"
                } else {
                    "Получить WARP"
                };
                if ui
                    .add_enabled(!busy && !complete, pill_button(theme, "", &tr(label, &[])))
                    .clicked()
                {
                    bridge.generate_warp_configs();
                }
                if busy {
                    ui.spinner();
                    ui.label(tr("Обычно это занимает несколько секунд", &[]));
                } else if complete {
                    ui.label(tr("Серверов в Lumen: {count}", &[("count", &count)]));
                } else {
                    ui.label(tr(
                        "AmneziaWG ×3 и MASQUE ×2 — выберите любой на панели",
                        &[],
                    ));
                }
            }
            None => {}
        }
    }

    fn done(&mut self, ui: &mut Ui, theme: &Theme, bridge: &mut AppBridge) {
        page_header(
            ui,
            theme,
            tr("Всё готово", &[]),
            tr(
                if bridge.onboarding_server_count() > 0 {
                    "Lumen настроен. Выберите сервер на панели и нажмите «Подключить»."
                } else {
                    "Lumen настроен. Добавьте сервер, когда он появится, — и можно подключаться."
                },
                &[],
            ),
            24.0,
        );
        let language = bridge
            .available_languages()
            .iter()
            .position(|code| code == bridge.language())
            .and_then(|index| bridge.language_labels().get(index).cloned())
            .unwrap_or_else(|| bridge.language().into());
        let preset = bridge.active_routing_preset_id();
        let route = bridge
            .regional_routing_presets()
            .into_iter()
            .find(|row| row["id"].as_str() == Some(&preset))
            .and_then(|row| row["name"].as_str().map(str::to_owned))
            .unwrap_or_else(|| tr("Свои правила", &[]));
        let palette = PALETTES
            .iter()
            .find(|(key, _)| *key == bridge.settings().ui_theme_preset)
            .map_or("Default", |(_, title)| *title);
        let updates = if bridge.app_auto_update() {
            "Автоматически"
        } else if bridge.check_updates() {
            "Только проверка"
        } else {
            "Выкл"
        };
        let startup = if !bridge.launch_on_startup() {
            "Выкл"
        } else if bridge.launch_in_tray_on_startup() {
            "Вместе с Windows, в трее"
        } else {
            "Вместе с Windows"
        };
        ui.add_space(6.0);
        for (index, (label, value, target)) in [
            ("Язык", language, self.flow[0]),
            (
                "Тема",
                format!("{} · {palette}", tr(theme_label(bridge.theme_name()), &[])),
                Step::Look,
            ),
            (
                "Регион",
                tr(region_label(&bridge.regional_preset()), &[]),
                Step::Route,
            ),
            ("Маршрутизация", route, Step::Route),
            ("Обновления", tr(updates, &[]), Step::System),
            ("Автозапуск", tr(startup, &[]), Step::System),
        ]
        .into_iter()
        .enumerate()
        {
            if summary_row(ui, theme, index, label, &value) {
                if let Some(step_index) = self.flow.iter().position(|&step| step == target) {
                    self.step = step_index;
                }
            }
        }
        ui.add_space(18.0);
        support_card(ui, theme, bridge, &mut self.show_support);
    }
}

fn welcome(ui: &mut Ui, theme: &Theme, bridge: &mut AppBridge) {
    let returning = bridge.onboarding_returning_user();
    let title = if returning {
        "С возвращением в Lumen"
    } else {
        "Добро пожаловать в Lumen"
    };
    let intro = if returning {
        tr(
            "Lumen обновлён до версии {version}. Давайте за минуту проверим основные настройки.",
            &[("version", bridge.app_version())],
        )
    } else {
        tr(
            "Lumen — VPN-клиент для Windows: два ядра, TUN и системный прокси, умная маршрутизация и обход DPI через zapret. Настроим всё за минуту.",
            &[],
        )
    };
    // The welcome header is the largest title in the flow (QML 30pt).
    ui.label(
        RichText::new(tr(title, &[]))
            .size(30.0)
            .family(crate::shell::theme::semibold_family()),
    );
    ui.add_space(10.0);
    ui.label(
        RichText::new(intro)
            .size(15.0)
            .color(theme.text_muted),
    );
    ui.add_space(12.0);
    language_picker(ui, theme, bridge);
    section_label(ui, theme, "Возможности");
    let tiles = [
        (
            "\u{ec05}",
            "Xray и sing-box",
            "Два ядра и автоматический выбор подходящего",
        ),
        (
            "\u{e701}",
            "TUN и системный прокси",
            "Весь трафик системы или только нужные приложения",
        ),
        (
            "\u{e774}",
            "Умная маршрутизация",
            "Региональные пресеты и свои правила для сайтов",
        ),
        (
            "\u{e945}",
            "Zapret",
            "Обход DPI-блокировок даже без VPN-сервера",
        ),
    ];
    if ui.available_width() >= 560.0 {
        let cell = (ui.available_width() - 12.0) / 2.0;
        for pair in tiles.chunks(2) {
            ui.allocate_ui_with_layout(
                vec2(ui.available_width(), 74.0),
                Layout::left_to_right(Align::TOP),
                |ui| {
                    ui.spacing_mut().item_spacing.x = 12.0;
                    for (glyph, title, subtitle) in pair {
                        feature_tile(ui, theme, cell, glyph, title, subtitle);
                    }
                },
            );
            ui.add_space(12.0);
        }
    } else {
        for (glyph, title, subtitle) in tiles {
            feature_tile(ui, theme, ui.available_width(), glyph, title, subtitle);
            ui.add_space(12.0);
        }
    }
    ui.add_space(8.0);
}

fn language_picker(ui: &mut Ui, theme: &Theme, bridge: &mut AppBridge) {
    section_label(ui, theme, "Язык интерфейса");
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        for (code, label) in bridge
            .available_languages()
            .iter()
            .zip(bridge.language_labels())
        {
            if pill(ui, theme, &label, bridge.language() == code) {
                bridge.set_language(code);
            }
        }
    });
    ui.add_space(4.0);
}

fn route(ui: &mut Ui, theme: &Theme, bridge: &mut AppBridge) {
    page_header(
        ui,
        theme,
        tr("Регион и маршрутизация", &[]),
        tr(
            "Регион определяет списки заблокированных ресурсов и быстрые пресеты маршрутов",
            &[],
        ),
        24.0,
    );
    section_label(ui, theme, "Региональный профиль");
    let current = bridge.regional_preset();
    ui.add_enabled_ui(!bridge.regional_preset_busy(), |ui| {
        ui.columns(3, |columns| {
            for (index, (key, code)) in [("russia", "RU"), ("china", "CN"), ("iran", "IR")]
                .into_iter()
                .enumerate()
            {
                if region_card(&mut columns[index], theme, code, region_label(key), current == key)
                    && current != key
                {
                    bridge.set_regional_preset(key);
                }
            }
        });
    });
    if bridge.regional_preset_busy() {
        ui.spinner();
    }
    section_label(ui, theme, "Что пускать через VPN");
    let active = bridge.active_routing_preset_id();
    let width = ui.available_width();
    for (index, item) in bridge.regional_routing_presets().into_iter().enumerate() {
        let id = item["id"].as_str().unwrap_or_default().to_string();
        let name = item["name"].as_str().unwrap_or_default().to_string();
        let description = match id.as_str() {
            "global" => "Весь трафик идёт через VPN",
            "blocked" | "blocked_cn" => {
                "Через VPN — только заблокированные сайты, остальное напрямую"
            }
            "except_ru" => "Через VPN всё, кроме российских сайтов и сервисов",
            "except_cn" => "Через VPN всё, кроме китайских сайтов и сервисов",
            "except_ir" => "Через VPN всё, кроме иранских сайтов и сервисов",
            _ => "",
        };
        if index > 0 {
            ui.add_space(10.0);
        }
        let glyph = match id.as_str() {
            "global" => "\u{e774}",
            "blocked" | "blocked_cn" => "\u{e72e}",
            _ => "\u{e909}",
        };
        if routing_card(ui, theme, width, glyph, &name, &tr(description, &[]), active == id) {
            bridge.apply_routing_preset_option(&id);
        }
    }
    ui.add_space(8.0);
}

fn system(ui: &mut Ui, theme: &Theme, bridge: &mut AppBridge) {
    page_header(
        ui,
        theme,
        tr("Обновления, запуск и диагностика", &[]),
        tr("Выберите, как Lumen будет обновляться и запускаться.", &[]),
        24.0,
    );
    section_label(ui, theme, "Обновления");
    if toggle_row(
        ui,
        theme,
        bridge.check_updates(),
        "Проверять обновления",
        "Автоматически проверять наличие обновлений",
    ) {
        bridge.set_check_updates(bridge.check_updates());
    }
    let mut app_auto = bridge.app_auto_update();
    if toggle_row(
        ui,
        theme,
        app_auto,
        "Автообновление приложения",
        "Автоматически скачивать и устанавливать новые версии",
    ) {
        app_auto = !app_auto;
        bridge.set_app_auto_update(app_auto);
    }
    section_label(ui, theme, "Запуск");
    let mut startup = bridge.launch_on_startup();
    if toggle_row(
        ui,
        theme,
        startup,
        "Запускать при входе",
        "Автозапуск вместе с Windows",
    ) {
        startup = !startup;
        bridge.set_launch_on_startup(startup);
    }
    let mut tray = bridge.launch_in_tray_on_startup();
    let enabled = bridge.launch_on_startup();
    ui.add_enabled_ui(enabled, |ui| {
        if toggle_row(
            ui,
            theme,
            tray,
            "Запускать автозапуск в трей",
            "При входе в Windows не показывать окно, только иконку в трее",
        ) {
            tray = !tray;
            bridge.set_launch_in_tray_on_startup(tray);
        }
    });
    section_label(ui, theme, "Диагностика");
    let mut diagnostics = bridge.diagnostics_upload_enabled();
    if toggle_row(
        ui,
        theme,
        diagnostics,
        "Анонимная диагностика",
        "Отправлять отчёты об ошибках без личных данных",
    ) {
        diagnostics = !diagnostics;
        bridge.set_diagnostics_upload(diagnostics);
    }
    ui.add_space(8.0);
}

pub(super) fn import_clipboard(bridge: &mut AppBridge) {
    if let Ok(mut clipboard) = arboard::Clipboard::new() {
        if let Ok(files) = clipboard.get().file_list()
            && files.iter().any(|path| has_supported_config_drop(path))
        {
            bridge.import_node_files(&files);
            return;
        }
        bridge.import_clipboard_text(&clipboard.get_text().unwrap_or_default());
    } else {
        bridge.import_clipboard_text("");
    }
}

pub(super) fn import_config_file(bridge: &mut AppBridge) -> anyhow::Result<()> {
    let filter = "VPN configs (*.conf *.ovpn *.txt *.json *.yaml *.yml)\0*.conf;*.ovpn;*.txt;*.json;*.yaml;*.yml\0OpenVPN profiles (*.ovpn)\0*.ovpn\0All files (*.*)\0*.*\0";
    if let Some(path) =
        win_native::pick_file(Path::new(""), &tr("Импортировать сервер", &[]), filter)?
    {
        bridge.import_node_files(&[path]);
    }
    Ok(())
}

fn theme_label(mode: &str) -> &'static str {
    match mode {
        "light" => "Светлая",
        "dark" => "Тёмная",
        _ => "Авто",
    }
}
fn region_label(region: &str) -> &'static str {
    match region {
        "china" => "Китай",
        "iran" => "Иран",
        _ => "Россия",
    }
}

// ── shared drawing helpers (OnboardingOverlay.qml inline components) ────

/// `PageHeader`: 24/28pt title + normal muted subtitle.
fn page_header(ui: &mut Ui, theme: &Theme, title: String, subtitle: String, title_size: f32) {
    ui.label(RichText::new(title).size(title_size).family(crate::shell::theme::semibold_family()));
    ui.add_space(6.0);
    if !subtitle.is_empty() {
        ui.label(RichText::new(subtitle).color(theme.text_muted));
    }
    ui.add_space(2.0);
}

/// `SectionLabel`: uppercase muted small semibold with 22/10 margins.
fn section_label(ui: &mut Ui, theme: &Theme, text: &str) {
    ui.add_space(22.0);
    ui.label(
        RichText::new(text.to_uppercase())
            .size(theme.font_small as f32)
            .family(crate::shell::theme::semibold_family())
            .color(theme.text_muted),
    );
    ui.add_space(10.0);
}

/// `Chip`: 34pt pill, accent fill when selected.
fn pill(ui: &mut Ui, theme: &Theme, label: &str, selected: bool) -> bool {
    let galley = ui.painter().layout_no_wrap(
        label.to_string(),
        FontId::proportional(theme.font_normal as f32),
        if selected { theme.accent_text } else { theme.text },
    );
    let width = (galley.size().x + 28.0).max(34.0);
    let (rect, response) = ui.allocate_exact_size(vec2(width, 34.0), Sense::click());
    let fill = if selected {
        theme.accent
    } else if response.hovered() {
        theme.card_hover
    } else {
        theme.card
    };
    ui.painter().rect(
        rect,
        17.0,
        fill,
        if selected {
            Stroke::NONE
        } else {
            Stroke::new(1.0, theme.divider)
        },
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
    response.clicked()
}

/// Accent `pill_button` built on the shared fluent button.
fn pill_button(theme: &Theme, glyph: &str, label: &str) -> egui::Button<'static> {
    fluent_button(theme, glyph, label)
}

fn fluent_button(theme: &Theme, glyph: &str, label: &str) -> egui::Button<'static> {
    use crate::shell::pages::fluent_button as fb;
    fb::button(theme, glyph, label, true)
}

fn ghost(ui: &mut Ui, theme: &Theme, label: &str) -> bool {
    use crate::shell::pages::fluent_button as fb;
    fb::style(ui);
    ui.add(fb::button(theme, "", &tr(label, &[]), false)).clicked()
}

fn ghost_glyph(ui: &mut Ui, theme: &Theme, glyph: &str, label: &str) -> bool {
    use crate::shell::pages::fluent_button as fb;
    fb::style(ui);
    ui.add(fb::button(theme, glyph, &tr(label, &[]), false))
        .clicked()
}

fn hline(ui: &mut Ui, theme: &Theme, inset: f32) {
    ui.horizontal(|ui| {
        ui.add_space(inset);
        let (rect, _) =
            ui.allocate_exact_size(vec2(ui.available_width().max(0.0), 1.0), Sense::hover());
        ui.painter().rect_filled(rect, 0.0, theme.divider);
    });
}

/// `ThemeCard`: 64pt card with the theme label, accent border when selected.
fn theme_card(ui: &mut Ui, theme: &Theme, label: &str, selected: bool) -> bool {
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 64.0), Sense::click());
    let fill = if selected {
        egui::Color32::from_rgba_unmultiplied(
            theme.accent.r(),
            theme.accent.g(),
            theme.accent.b(),
            26,
        )
    } else if response.hovered() {
        theme.card_hover
    } else {
        theme.card
    };
    ui.painter().rect(
        rect,
        (theme.radius as f32 + 2.0),
        fill,
        if selected {
            Stroke::new(2.0, theme.accent)
        } else {
            Stroke::new(1.0, theme.divider)
        },
        egui::StrokeKind::Inside,
    );
    ui.painter().text(
        egui::pos2(rect.center().x, rect.center().y),
        egui::Align2::CENTER_CENTER,
        label,
        if selected {
            semibold(theme.font_normal as f32)
        } else {
            FontId::proportional(theme.font_normal as f32)
        },
        theme.text,
    );
    response.clicked()
}

/// `PaletteSwatch`: two-tone preview block with the accent dot and label.
fn palette_swatch(ui: &mut Ui, theme: &Theme, label: &str, accent: Color32, selected: bool) -> bool {
    let width = 74.0;
    let (rect, response) = ui.allocate_exact_size(vec2(width, 74.0), Sense::click());
    let stroke = if selected {
        Stroke::new(2.0, theme.accent)
    } else {
        Stroke::new(1.0, theme.divider)
    };
    let fill = if selected {
        egui::Color32::from_rgba_unmultiplied(
            theme.accent.r(),
            theme.accent.g(),
            theme.accent.b(),
            26,
        )
    } else if response.hovered() {
        theme.card_hover
    } else {
        theme.card
    };
    ui.painter()
        .rect(rect, theme.radius as u8 + 2, fill, stroke, egui::StrokeKind::Inside);
    // Two-tone preview area.
    let preview = egui::Rect::from_min_size(
        egui::pos2(rect.left() + 10.0, rect.top() + 10.0),
        vec2(width - 20.0, 28.0),
    );
    ui.painter().rect(
        preview,
        4.0,
        theme.bg,
        Stroke::new(1.0, theme.divider),
        egui::StrokeKind::Inside,
    );
    ui.painter().rect(
        egui::Rect::from_min_max(
            egui::pos2(preview.left(), preview.center().y),
            egui::pos2(preview.right(), preview.bottom()),
        ),
        0.0,
        theme.card,
        Stroke::NONE,
        egui::StrokeKind::Inside,
    );
    ui.painter()
        .circle_filled(egui::pos2(preview.right() - 8.0, preview.bottom() - 8.0), 5.0, accent);
    ui.painter().text(
        egui::pos2(rect.center().x, rect.bottom() - 12.0),
        egui::Align2::CENTER_CENTER,
        label,
        FontId::proportional(theme.font_small as f32),
        theme.text,
    );
    response.clicked()
}

/// `AccentDot`: 34pt colour circle with a text-coloured ring when selected.
fn accent_dot(ui: &mut Ui, theme: &Theme, color: Option<Color32>, selected: bool) -> bool {
    let Some(color) = color else {
        return false;
    };
    let (rect, response) = ui.allocate_exact_size(vec2(34.0, 34.0), Sense::click());
    let painter = ui.painter();
    if selected {
        painter.circle_stroke(rect.center(), 16.0, Stroke::new(2.0, theme.text));
    }
    let dot_radius = if selected { 12.0 } else { 13.0 };
    painter.circle_filled(rect.center(), dot_radius, color);
    response.clicked()
}

/// `ChoiceCard`-style region card with the country flag badge.
fn region_card(
    ui: &mut Ui,
    theme: &Theme,
    code: &str,
    title: &str,
    selected: bool,
) -> bool {
    let (rect, response) =
        ui.allocate_exact_size(vec2(ui.available_width(), 60.0), Sense::click());
    choice_card_fill(ui, theme, rect, selected, response.hovered());
    ui.scope_builder(
        egui::UiBuilder::new()
            .max_rect(rect.shrink2(vec2(14.0, 0.0)))
            .layout(Layout::left_to_right(Align::Center)),
        |ui| {
            ui.spacing_mut().item_spacing.x = 12.0;
            flag::show(ui, code, vec2(24.0, 18.0));
            ui.label(RichText::new(tr(title, &[])).size(theme.font_normal as f32));
        },
    );
    response.clicked()
}

/// `ChoiceCard`: glyph + title + subtitle, accent tint when selected.
fn choice_card(
    ui: &mut Ui,
    theme: &Theme,
    glyph: &str,
    title: &str,
    subtitle: &str,
    selected: bool,
) -> bool {
    let (rect, response) =
        ui.allocate_exact_size(vec2(ui.available_width(), 64.0), Sense::click());
    choice_card_fill(ui, theme, rect, selected, response.hovered());
    let inner = rect.shrink(12.0);
    ui.scope_builder(egui::UiBuilder::new().max_rect(inner), |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 12.0;
            ui.label(
                RichText::new(glyph)
                    .size(18.0)
                    .color(if selected { theme.accent } else { theme.text_muted }),
            );
            ui.vertical(|ui| {
                ui.label(RichText::new(title).size(theme.font_normal as f32).family(crate::shell::theme::semibold_family()));
                ui.label(
                    RichText::new(subtitle)
                        .size(theme.font_small as f32)
                        .color(theme.text_faint),
                );
            });
        });
    });
    response.clicked()
}

fn choice_card_fill(ui: &mut Ui, theme: &Theme, rect: egui::Rect, selected: bool, hovered: bool) {
    let fill = if selected {
        egui::Color32::from_rgba_unmultiplied(
            theme.accent.r(),
            theme.accent.g(),
            theme.accent.b(),
            if theme.dark { 26 } else { 15 },
        )
    } else if hovered {
        theme.card_hover
    } else {
        theme.card
    };
    let stroke = if selected {
        Stroke::new(2.0, theme.accent)
    } else {
        Stroke::new(1.0, theme.divider)
    };
    ui.painter().rect(
        rect,
        theme.radius as u8 + 2,
        fill,
        stroke,
        egui::StrokeKind::Inside,
    );
}

/// Routing choice row: glyph circle + name + description.
fn routing_card(
    ui: &mut Ui,
    theme: &Theme,
    width: f32,
    glyph: &str,
    title: &str,
    subtitle: &str,
    selected: bool,
) -> bool {
    let (rect, response) = ui.allocate_exact_size(vec2(width, 60.0), Sense::click());
    choice_card_fill(ui, theme, rect, selected, response.hovered());
    let inner = rect.shrink(12.0);
    ui.scope_builder(egui::UiBuilder::new().max_rect(inner), |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 12.0;
            ui.label(
                RichText::new(glyph)
                    .size(18.0)
                    .color(if selected { theme.accent } else { theme.text_muted }),
            );
            ui.vertical(|ui| {
                ui.label(RichText::new(title).size(theme.font_normal as f32).family(crate::shell::theme::semibold_family()));
                if !subtitle.is_empty() {
                    ui.label(
                        RichText::new(subtitle)
                            .size(theme.font_small as f32)
                            .color(theme.text_faint),
                    );
                }
            });
        });
    });
    response.clicked()
}

/// `FeatureTile`: card with accent glyph, semibold title and muted subtitle.
fn feature_tile(ui: &mut Ui, theme: &Theme, width: f32, glyph: &str, title: &str, subtitle: &str) {
    let (rect, _) = ui.allocate_exact_size(vec2(width, 74.0), Sense::hover());
    ui.painter().rect(
        rect,
        theme.radius as u8 + 2,
        theme.card,
        Stroke::new(1.0, theme.divider),
        egui::StrokeKind::Inside,
    );
    let inner = rect.shrink(14.0);
    ui.scope_builder(egui::UiBuilder::new().max_rect(inner), |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 12.0;
            ui.label(RichText::new(glyph).size(18.0).color(theme.accent));
            ui.vertical(|ui| {
                ui.label(RichText::new(tr(title, &[])).size(theme.font_normal as f32).family(crate::shell::theme::semibold_family()));
                ui.label(
                    RichText::new(tr(subtitle, &[]))
                        .size(theme.font_small as f32)
                        .color(theme.text_faint),
                );
            });
        });
    });
}

/// "What's new" group card with inset dividers between note rows.
fn group_card(ui: &mut Ui, theme: &Theme, group: &serde_json::Value) {
    let width = ui.available_width();
    let items = group["items"].as_array().cloned().unwrap_or_default();
    // Measure the natural height first, then draw the card behind the rows.
    let height = items.len() as f32 * 58.0 + 16.0;
    let (rect, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
    ui.painter().rect(
        rect,
        theme.radius as u8 + 2,
        theme.card,
        Stroke::new(1.0, theme.divider),
        egui::StrokeKind::Inside,
    );
    let mut card = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(Layout::top_down(Align::LEFT)),
    );
    card.spacing_mut().item_spacing.y = 0.0;
    for (index, item) in items.iter().enumerate() {
        if index > 0 {
            hline(&mut card, theme, 0.0);
        }
        note_row(
            &mut card,
            theme,
            item["glyph"].as_str().unwrap_or_default(),
            &tr(item["title"].as_str().unwrap_or_default(), &[]),
            &tr(item["text"].as_str().unwrap_or_default(), &[]),
        );
    }
    ui.add_space(12.0);
}

/// `NoteRow`: glyph, semibold title, muted text.
fn note_row(ui: &mut Ui, theme: &Theme, glyph: &str, title: &str, text: &str) {
    ui.scope_builder(
        egui::UiBuilder::new()
            .max_rect(egui::Rect::from_min_size(
                ui.cursor().min,
                vec2(ui.available_width(), 58.0),
            )),
        |ui| {
            ui.horizontal(|ui| {
                ui.add_space(14.0);
                if !glyph.is_empty() {
                    ui.label(RichText::new(glyph).size(16.0).color(theme.accent));
                }
                ui.vertical(|ui| {
                    ui.add_space(8.0);
                    ui.label(RichText::new(title).size(theme.font_normal as f32).family(crate::shell::theme::semibold_family()));
                    ui.label(
                        RichText::new(text)
                            .size(theme.font_small as f32)
                            .color(theme.text_muted),
                    );
                    ui.add_space(8.0);
                });
            });
        },
    );
}

/// Summary row on the done page: muted label, value, chevron; clickable to
/// jump back to the matching step.
fn summary_row(ui: &mut Ui, theme: &Theme, index: usize, label: &str, value: &str) -> bool {
    let (rect, response) =
        ui.allocate_exact_size(vec2(ui.available_width(), 40.0), Sense::click());
    if response.hovered() {
        ui.painter()
            .rect_filled(rect, theme.radius as u8, theme.card_hover);
    }
    let painter = ui.painter();
    painter.text(
        egui::pos2(rect.left() + 4.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        tr(label, &[]),
        FontId::proportional(theme.font_normal as f32),
        theme.text_muted,
    );
    painter.text(
        egui::pos2(rect.right() - 26.0, rect.center().y),
        egui::Align2::RIGHT_CENTER,
        value,
        FontId::proportional(theme.font_normal as f32),
        theme.text,
    );
    painter.text(
        egui::pos2(rect.right() - 6.0, rect.center().y),
        egui::Align2::RIGHT_CENTER,
        "\u{e76c}",
        FontId::proportional(10.0),
        theme.text_faint,
    );
    if index + 1 < 6 {
        hline(ui, theme, 0.0);
    }
    response.clicked()
}

/// `SupportCard`: accent-tinted block with GitHub/donation actions.
fn support_card(ui: &mut Ui, theme: &Theme, bridge: &mut AppBridge, show_support: &mut bool) {
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(vec2(width, 118.0), Sense::hover());
    ui.painter().rect(
        rect,
        theme.radius as u8 + 2,
        egui::Color32::from_rgba_unmultiplied(
            theme.accent.r(),
            theme.accent.g(),
            theme.accent.b(),
            if theme.dark { 20 } else { 13 },
        ),
        Stroke::new(
            1.0,
            egui::Color32::from_rgba_unmultiplied(
                theme.accent.r(),
                theme.accent.g(),
                theme.accent.b(),
                90,
            ),
        ),
        egui::StrokeKind::Inside,
    );
    let mut card = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect.shrink2(vec2(16.0, 10.0)))
            .layout(Layout::top_down(Align::LEFT)),
    );
    card.spacing_mut().item_spacing.y = 6.0;
    card.label(RichText::new(tr("Нравится Lumen?", &[])).family(crate::shell::theme::semibold_family()));
    card.label(
        RichText::new(tr("Lumen бесплатный и с открытым кодом. Поставьте звезду на GitHub — так о проекте узнает больше людей. А если хотите помочь развитию — поддержите проект.", &[]))
            .size(theme.font_small as f32)
            .color(theme.text_muted),
    );
    card.horizontal(|ui| {
        if ghost(ui, &theme, "Поставить звезду на GitHub") {
            open_url(ui, "https://github.com/krambovic/Lumen");
        }
        if ghost(ui, &theme, "Поддержать проект") {
            *show_support = !*show_support;
        }
        if *show_support {
            if ghost(ui, &theme, "DonationAlerts") {
                open_url(ui, DONATION_URL);
            }
            if ghost(ui, &theme, "Скопировать адрес USDT (TRC20)")
                && let Some(address) = bridge.copy_text(USDT_ADDRESS)
            {
                ui.ctx().copy_text(address);
            }
        }
    });
}

/// `toggle_row`: label + detail on the left, 44×20 switch on the right.
/// Returns true when the value flipped.
fn toggle_row(
    ui: &mut Ui,
    theme: &Theme,
    value: bool,
    label: &str,
    detail: &str,
) -> bool {
    let height = if detail.is_empty() { 32.0 } else { 46.0 };
    let mut next = value;
    ui.allocate_ui_with_layout(
        vec2(ui.available_width(), height),
        Layout::right_to_left(Align::Center),
        |ui| {
            let (rect, response) =
                ui.allocate_exact_size(vec2(44.0, 20.0), Sense::click());
            let fill = if value { theme.accent } else { theme.control_fill };
            let stroke = if value {
                Stroke::NONE
            } else {
                Stroke::new(1.0, theme.border_solid)
            };
            ui.painter()
                .rect(rect, 10.0, fill, stroke, egui::StrokeKind::Inside);
            let knob_x = if value {
                rect.right() - 10.0
            } else {
                rect.left() + 10.0
            };
            ui.painter().circle_filled(
                egui::pos2(knob_x, rect.center().y),
                5.0,
                if value { theme.accent_text } else { theme.text_muted },
            );
            if response.clicked() {
                next = !value;
            }
            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                ui.vertical(|ui| {
                    ui.label(RichText::new(tr(label, &[])).size(theme.font_normal as f32));
                    if !detail.is_empty() {
                        ui.label(
                            RichText::new(tr(detail, &[]))
                                .size(theme.font_small as f32)
                                .color(theme.text_faint),
                        );
                    }
                });
            });
        },
    );
    next != value
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upgrade_without_setup_shows_only_news() {
        assert_eq!(steps(false, true), [Step::News]);
        assert_eq!(steps(true, true).first(), Some(&Step::News));
        assert_eq!(
            steps(true, false),
            [
                Step::Welcome,
                Step::Look,
                Step::Route,
                Step::Configs,
                Step::System,
                Step::Done
            ]
        );
    }
}


