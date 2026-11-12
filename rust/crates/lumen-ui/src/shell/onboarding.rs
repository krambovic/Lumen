//! First-run setup and upgrade notes from `OnboardingOverlay.qml`.

use std::path::Path;

use egui::{
    Button, Context, Frame, Id, Key, Modal, Modifiers, RichText, ScrollArea, Stroke, TextEdit, Ui,
};
use lumen_core::i18n::tr;

use super::pages::about::{DONATION_URL, USDT_ADDRESS, open_url};
use super::theme::{ACCENT_SWATCHES, Theme, hex_color};
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
        Modal::new(Id::unique("lumen-onboarding"))
            .backdrop_color(egui::Color32::from_black_alpha(155))
            .frame(
                Frame::popup(&ctx.global_style())
                    .fill(theme.flyout)
                    .stroke(Stroke::new(1.0, theme.flyout_border)),
            )
            .show(ctx, |ui| {
                ui.set_width((ctx.content_rect().width() - 54.0).clamp(260.0, 920.0));
                if self.flow.len() > 1 {
                    ui.horizontal_wrapped(|ui| {
                        for (index, step) in self.flow.iter().enumerate() {
                            let label =
                                Button::selectable(index == self.step, tr(step.title(), &[]));
                            if ui.add_enabled(index <= self.step, label).clicked() {
                                self.step = index;
                            }
                        }
                    });
                    ui.separator();
                }
                ScrollArea::vertical()
                    .id_salt(("onboarding-step", self.step))
                    .max_height((ctx.content_rect().height() - 174.0).max(110.0))
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        let spacing = theme.spacing as f32;
                        ui.spacing_mut().item_spacing.y = spacing.max(8.0);
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
                ui.separator();
                let keyboard = !ctx.egui_wants_keyboard_input();
                let next_key = keyboard
                    && ui.input_mut(|input| {
                        input.consume_key(Modifiers::NONE, Key::Enter)
                            || (!last && input.consume_key(Modifiers::NONE, Key::ArrowRight))
                    });
                let back_key = keyboard
                    && ui.input_mut(|input| input.consume_key(Modifiers::NONE, Key::ArrowLeft));
                ui.horizontal_wrapped(|ui| {
                    if self.flow.len() > 1 && button(ui, "Пропустить") {
                        finish = true;
                    }
                    if self.step > 0 && (button(ui, "Назад") || back_key) {
                        self.step -= 1;
                    }
                    let next = if last {
                        "Открыть Lumen"
                    } else if current == Step::Welcome {
                        "Начать"
                    } else {
                        "Далее"
                    };
                    if ui
                        .add(
                            Button::new(RichText::new(tr(next, &[])).color(theme.accent_text))
                                .fill(theme.accent),
                        )
                        .clicked()
                        || next_key
                    {
                        if last {
                            finish = true;
                        } else {
                            self.step += 1;
                        }
                    }
                });
            });
        if finish {
            self.finished = true;
            bridge.complete_onboarding();
        }
        finish
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
        heading(
            ui,
            theme,
            tr(
                "Lumen обновлён до версии {version}",
                &[("version", version)],
            ),
            tr(key, &[("version", &previous)]),
        );
        languages(ui, theme, bridge);
        for group in bridge.onboarding_whats_new() {
            ui.group(|ui| {
                let version = group["version"].as_str().unwrap_or_default();
                ui.label(
                    RichText::new(tr("Что нового в {version}", &[("version", version)])).strong(),
                );
                for item in group["items"].as_array().into_iter().flatten() {
                    ui.label(
                        RichText::new(tr(item["title"].as_str().unwrap_or_default(), &[])).strong(),
                    );
                    ui.label(tr(item["text"].as_str().unwrap_or_default(), &[]));
                }
            });
        }
    }

    fn look(&mut self, ui: &mut Ui, theme: &Theme, bridge: &mut AppBridge) {
        heading(
            ui,
            theme,
            tr("Оформление", &[]),
            tr("Выберите внешний вид — изменения применяются сразу", &[]),
        );
        section(ui, "Тема");
        ui.horizontal_wrapped(|ui| {
            for mode in ["system", "light", "dark"] {
                if ui
                    .selectable_label(bridge.theme_name() == mode, tr(theme_label(mode), &[]))
                    .clicked()
                {
                    bridge.set_theme(mode);
                }
            }
        });
        section(ui, "Палитра");
        ui.horizontal_wrapped(|ui| {
            for (key, title) in PALETTES {
                if ui
                    .selectable_label(bridge.settings().ui_theme_preset == key, title)
                    .clicked()
                {
                    bridge.set_ui_theme_preset(key);
                }
            }
        });
        section(ui, "Цвет акцента");
        ui.horizontal_wrapped(|ui| {
            if !self.windows_accent.is_empty()
                && swatch(ui, bridge, &self.windows_accent, "Windows")
            {
                bridge.set_accent(&self.windows_accent);
            }
            for color in ACCENT_SWATCHES.iter().take(10) {
                if !color.eq_ignore_ascii_case(&self.windows_accent)
                    && swatch(ui, bridge, color, color)
                {
                    bridge.set_accent(color);
                }
            }
            let mut custom = theme.accent;
            if ui
                .color_edit_button_srgba(&mut custom)
                .on_hover_text(tr("Свой цвет", &[]))
                .changed()
            {
                bridge.set_accent(&format!(
                    "#{:02X}{:02X}{:02X}",
                    custom.r(),
                    custom.g(),
                    custom.b()
                ));
            }
        });
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
        heading(
            ui,
            theme,
            tr("У вас уже есть VPN-конфигурация?", &[]),
            description,
        );
        ui.horizontal_wrapped(|ui| {
            if ui
                .selectable_label(self.config_choice == Some(true), tr("Да, есть", &[]))
                .on_hover_text(tr("Импортирую ключ, подписку или файл", &[]))
                .clicked()
            {
                self.config_choice = Some(true);
            }
            if ui
                .selectable_label(self.config_choice == Some(false), tr("Нет, пока нет", &[]))
                .on_hover_text(tr("Получу бесплатный WARP прямо здесь", &[]))
                .clicked()
            {
                self.config_choice = Some(false);
            }
        });
        match self.config_choice {
            Some(true) => {
                ui.horizontal_wrapped(|ui| {
                    if button(ui, "Вставить из буфера") {
                        import_clipboard(bridge);
                    }
                    if button(ui, "Открыть файл") {
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
                            Button::new(tr("Добавить", &[])),
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
                section(ui, "Бесплатный Cloudflare WARP");
                ui.label(tr("Lumen зарегистрирует бесплатный профиль WARP и добавит готовые конфигурации AmneziaWG и MASQUE в группу «WARP».", &[]));
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
                    .add_enabled(!busy && !complete, Button::new(tr(label, &[])))
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
        heading(
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
        for (label, value, target) in [
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
        ] {
            if ui
                .button(format!("{}  ·  {value}", tr(label, &[])))
                .clicked()
                && let Some(index) = self.flow.iter().position(|&step| step == target)
            {
                self.step = index;
            }
        }
        section(ui, "Нравится Lumen?");
        ui.label(tr("Lumen бесплатный и с открытым кодом. Поставьте звезду на GitHub — так о проекте узнает больше людей. А если хотите помочь развитию — поддержите проект.", &[]));
        ui.horizontal_wrapped(|ui| {
            if button(ui, "Поставить звезду на GitHub") {
                open_url(ui, "https://github.com/krambovic/Lumen");
            }
            if button(ui, "Поддержать проект") {
                self.show_support = !self.show_support;
            }
        });
        if self.show_support {
            ui.horizontal_wrapped(|ui| {
                if ui.button("DonationAlerts").clicked() {
                    open_url(ui, DONATION_URL);
                }
                if button(ui, "Скопировать адрес USDT (TRC20)")
                    && let Some(address) = bridge.copy_text(USDT_ADDRESS)
                {
                    ui.ctx().copy_text(address);
                }
            });
        }
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
    heading(ui, theme, tr(title, &[]), intro);
    languages(ui, theme, bridge);
    section(ui, "Возможности");
    for (title, text) in [
        (
            "Xray и sing-box",
            "Два ядра и автоматический выбор подходящего",
        ),
        (
            "TUN и системный прокси",
            "Весь трафик системы или только нужные приложения",
        ),
        (
            "Умная маршрутизация",
            "Региональные пресеты и свои правила для сайтов",
        ),
        ("Zapret", "Обход DPI-блокировок даже без VPN-сервера"),
    ] {
        ui.label(RichText::new(tr(title, &[])).strong());
        ui.label(tr(text, &[]));
    }
}

fn languages(ui: &mut Ui, _theme: &Theme, bridge: &mut AppBridge) {
    section(ui, "Язык интерфейса");
    ui.horizontal_wrapped(|ui| {
        for (code, label) in bridge
            .available_languages()
            .iter()
            .zip(bridge.language_labels())
        {
            if ui
                .selectable_label(bridge.language() == code, label)
                .clicked()
            {
                bridge.set_language(code);
            }
        }
    });
}

fn route(ui: &mut Ui, theme: &Theme, bridge: &mut AppBridge) {
    heading(
        ui,
        theme,
        tr("Регион и маршрутизация", &[]),
        tr(
            "Регион определяет списки заблокированных ресурсов и быстрые пресеты маршрутов",
            &[],
        ),
    );
    section(ui, "Региональный профиль");
    let current = bridge.regional_preset();
    ui.add_enabled_ui(!bridge.regional_preset_busy(), |ui| {
        ui.horizontal_wrapped(|ui| {
            for key in ["russia", "china", "iran"] {
                if ui
                    .selectable_label(current == key, tr(region_label(key), &[]))
                    .clicked()
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
    section(ui, "Что пускать через VPN");
    let active = bridge.active_routing_preset_id();
    for item in bridge.regional_routing_presets() {
        let id = item["id"].as_str().unwrap_or_default();
        let name = item["name"].as_str().unwrap_or_default();
        if ui.selectable_label(active == id, name).clicked() {
            bridge.apply_routing_preset_option(id);
        }
        let description = match id {
            "global" => "Весь трафик идёт через VPN",
            "blocked" | "blocked_cn" => {
                "Через VPN — только заблокированные сайты, остальное напрямую"
            }
            "except_ru" => "Через VPN всё, кроме российских сайтов и сервисов",
            "except_cn" => "Через VPN всё, кроме китайских сайтов и сервисов",
            "except_ir" => "Через VPN всё, кроме иранских сайтов и сервисов",
            _ => "",
        };
        if !description.is_empty() {
            ui.label(RichText::new(tr(description, &[])).color(theme.text_muted));
        }
    }
}

fn system(ui: &mut Ui, theme: &Theme, bridge: &mut AppBridge) {
    heading(
        ui,
        theme,
        tr("Обновления, запуск и диагностика", &[]),
        tr("Выберите, как Lumen будет обновляться и запускаться.", &[]),
    );
    section(ui, "Обновления");
    if let Some(value) = toggle(
        ui,
        bridge.check_updates(),
        "Проверять обновления",
        "Автоматически проверять наличие обновлений",
    ) {
        bridge.set_check_updates(value);
    }
    if let Some(value) = toggle(
        ui,
        bridge.app_auto_update(),
        "Автообновление приложения",
        "Автоматически скачивать и устанавливать новые версии",
    ) {
        bridge.set_app_auto_update(value);
    }
    section(ui, "Запуск");
    if let Some(value) = toggle(
        ui,
        bridge.launch_on_startup(),
        "Запускать при входе",
        "Автозапуск вместе с Windows",
    ) {
        bridge.set_launch_on_startup(value);
    }
    ui.add_enabled_ui(bridge.launch_on_startup(), |ui| {
        if let Some(value) = toggle(
            ui,
            bridge.launch_in_tray_on_startup(),
            "Запускать автозапуск в трей",
            "При входе в Windows не показывать окно, только иконку в трее",
        ) {
            bridge.set_launch_in_tray_on_startup(value);
        }
    });
    section(ui, "Диагностика");
    if let Some(value) = toggle(
        ui,
        bridge.diagnostics_upload_enabled(),
        "Анонимная диагностика",
        "Отправлять отчёты об ошибках без личных данных",
    ) {
        bridge.set_diagnostics_upload(value);
    }
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
fn button(ui: &mut Ui, key: &str) -> bool {
    ui.button(tr(key, &[])).clicked()
}
fn section(ui: &mut Ui, key: &str) {
    ui.label(RichText::new(tr(key, &[])).strong());
}
fn heading(ui: &mut Ui, theme: &Theme, title: String, detail: String) {
    ui.label(RichText::new(title).size(theme.font_title as f32).strong());
    ui.label(RichText::new(detail).color(theme.text_muted));
}
fn toggle(ui: &mut Ui, mut value: bool, label: &str, detail: &str) -> Option<bool> {
    ui.checkbox(&mut value, tr(label, &[]))
        .on_hover_text(tr(detail, &[]))
        .changed()
        .then_some(value)
}
fn swatch(ui: &mut Ui, bridge: &AppBridge, value: &str, label: &str) -> bool {
    let Some(color) = hex_color(value) else {
        return false;
    };
    let mark = if bridge.accent_color().eq_ignore_ascii_case(value) {
        "✓"
    } else {
        " "
    };
    ui.add(
        Button::new(RichText::new(mark).color(if Theme::accent_is_light(color) {
            egui::Color32::BLACK
        } else {
            egui::Color32::WHITE
        }))
        .fill(color)
        .min_size(egui::vec2(25.0, 25.0)),
    )
    .on_hover_text(label)
    .clicked()
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
