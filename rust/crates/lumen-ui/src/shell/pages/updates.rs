//! Application, core and resource update cards from `UpdatesPage.qml`.

mod release_notes;

use std::sync::LazyLock;

use egui::{Button, Color32, Frame, ProgressBar, RichText, ScrollArea, Stroke, Ui};
use lumen_core::i18n::tr;
use regex::Regex;
use serde_json::Value;

use super::PageContext;
use crate::shell::theme::Theme;
use crate::view_models::{AppBridge, app_bridge::UiEvent};

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum Phase {
    #[default]
    Idle,
    Checking,
    Available,
    Transferring,
    UpToDate,
    Updated,
    Error,
}

impl Phase {
    fn busy(self) -> bool {
        matches!(self, Self::Checking | Self::Transferring)
    }

    fn install_visible(self) -> bool {
        matches!(self, Self::Available | Self::Transferring)
    }

    fn color(self, theme: &Theme) -> Color32 {
        match self {
            Self::Error => theme.danger,
            Self::Available | Self::UpToDate | Self::Updated => theme.success,
            _ => theme.text_muted,
        }
    }
}

#[derive(Default)]
struct UpdateState {
    phase: Phase,
    message: String,
    percent: i64,
}

impl UpdateState {
    fn apply(&mut self, payload: &Value) {
        self.phase = match text(payload, "phase") {
            "checking" => Phase::Checking,
            "available" => Phase::Available,
            "updating" | "downloading" => Phase::Transferring,
            "uptodate" => Phase::UpToDate,
            "updated" | "ready" => Phase::Updated,
            "error" => Phase::Error,
            _ => Phase::Idle,
        };
        self.message = text(payload, "message").into();
        if self.phase == Phase::Checking {
            self.percent = 0;
        } else if let Some(percent) = payload["percent"].as_i64() {
            self.percent = percent.clamp(0, 100);
        }
    }

    fn progress(&self, ui: &mut Ui, theme: &Theme, unknown_percent: bool) {
        if !self.phase.busy() {
            return;
        }
        if self.phase != Phase::Checking && !(unknown_percent && self.percent == 0) {
            ui.add(
                ProgressBar::new(self.percent as f32 / 100.0)
                    .desired_height(5.0)
                    .fill(theme.accent),
            );
            return;
        }
        let (track, _) =
            ui.allocate_exact_size(egui::vec2(ui.available_width(), 5.0), egui::Sense::hover());
        if ui.is_rect_visible(track) {
            let phase = ui.input(|input| (input.time / 1.2).fract() as f32);
            let width = track.width() * 0.25;
            let x = track.left() - width + phase * (track.width() + width);
            let segment = egui::Rect::from_min_size(
                egui::pos2(x, track.top()),
                egui::vec2(width, track.height()),
            );
            ui.painter().rect_filled(track, 2.0, theme.control_fill);
            ui.painter()
                .rect_filled(segment.intersect(track), 2.0, theme.accent);
            ui.request_repaint();
        }
    }
}

#[derive(Default)]
struct AppUpdate {
    state: UpdateState,
    target_version: String,
    notes: String,
    downgrade: bool,
}

impl AppUpdate {
    fn apply(&mut self, payload: &Value) {
        self.state.apply(payload);
        if let Some(version) = payload["version"].as_str() {
            self.target_version = version.into();
        }
        if let Some(notes) = payload["notes"].as_str() {
            self.notes = notes.into();
        }
        if let Some(downgrade) = payload["isDowngrade"].as_bool() {
            self.downgrade = downgrade;
        }
        if self.state.phase == Phase::Checking {
            self.target_version.clear();
            self.notes.clear();
            self.downgrade = false;
        }
    }

    fn status(&self) -> String {
        match self.state.phase {
            Phase::Idle => tr(
                "Нажмите «Проверить обновления», чтобы узнать о новых версиях.",
                &[],
            ),
            Phase::Checking => tr("Проверка обновлений...", &[]),
            Phase::UpToDate => tr("У вас последняя версия", &[]),
            Phase::Available => {
                let prefix = if self.downgrade {
                    "Доступен переход на v"
                } else {
                    "Доступна новая версия: v"
                };
                format!("{}{}", tr(prefix, &[]), self.target_version)
            }
            _ if !self.state.message.is_empty() => tr(&self.state.message, &[]),
            Phase::Transferring => format!("{}{}%", tr("Загрузка: ", &[]), self.state.percent),
            Phase::Updated => tr("Обновление загружено. Перезапуск...", &[]),
            Phase::Error => tr("Ошибка проверки обновлений", &[]),
        }
    }

    fn install_label(&self) -> String {
        if self.target_version.is_empty() {
            return tr("Скачать и установить", &[]);
        }
        let prefix = if self.downgrade {
            "Перейти на v"
        } else {
            "Скачать v"
        };
        format!(
            "{}{}{}",
            tr(prefix, &[]),
            self.target_version,
            tr(" и установить", &[])
        )
    }
}

#[derive(Default)]
struct ComponentUpdate {
    state: UpdateState,
    version: String,
}

struct Component {
    key: &'static str,
    version_key: &'static str,
    title: &'static str,
    idle: &'static str,
    check: &'static str,
    install: &'static str,
}

const COMPONENTS: [Component; 4] = [
    Component {
        key: "xray",
        version_key: "xrayVersion",
        title: "Ядро Xray",
        idle: "Проверьте наличие новой версии ядра Xray-core от разработчиков.",
        check: "Проверить обновления Xray",
        install: "Обновить Xray core",
    },
    Component {
        key: "singbox",
        version_key: "singboxVersion",
        title: "Ядро sing-box extended",
        idle: "Проверьте наличие новой версии sing-box extended.",
        check: "Проверить sing-box",
        install: "Обновить sing-box",
    },
    Component {
        key: "geodata",
        version_key: "",
        title: "GeoIP и GeoSite",
        idle: "Обновляет geoip.dat и geosite.dat для правил маршрутизации.",
        check: "Проверить geoip/geosite",
        install: "Обновить geoip/geosite",
    },
    Component {
        key: "droute",
        version_key: "drouteVersion",
        title: "Discord · droute",
        idle: "Поставляется с Lumen; обновляется с GitHub.",
        check: "Проверить droute",
        install: "Обновить droute",
    },
];

#[derive(Default)]
pub(super) struct UpdatesPage {
    initialized: bool,
    app: AppUpdate,
    components: [ComponentUpdate; COMPONENTS.len()],
}

impl UpdatesPage {
    pub(super) fn on_event(&mut self, event: &UiEvent) {
        let (kind, payload) = match event {
            UiEvent::AppUpdateState(payload) => {
                self.app.apply(payload);
                return;
            }
            UiEvent::XrayUpdateState(payload) => ("xray", payload),
            UiEvent::ResourceUpdateState(payload) => (text(payload, "kind"), payload),
            _ => return,
        };
        if let Some(index) = COMPONENTS
            .iter()
            .position(|component| component.key == kind)
        {
            let update = &mut self.components[index];
            update.state.apply(payload);
            let version = text(payload, "version");
            if !version.is_empty() {
                update.version = component_version(kind, version);
            }
        }
    }

    pub(super) fn render(&mut self, ui: &mut Ui, page: &mut PageContext<'_>) {
        let theme = page.theme;
        let bridge = &mut *page.bridge;
        if !self.initialized {
            let initial = bridge.updates_initial_state();
            for (component, update) in COMPONENTS.iter().zip(&mut self.components) {
                if update.version.is_empty() {
                    update.version =
                        component_version(component.key, text(&initial, component.version_key));
                }
            }
            self.initialized = true;
        }
        ScrollArea::vertical()
            .id_salt("updates-page")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = theme.spacing_large as f32;
                ui.label(
                    RichText::new(tr("Обновления", &[]))
                        .size(theme.font_title as f32)
                        .strong(),
                );
                self.render_app(ui, theme, bridge);
                for (component, update) in COMPONENTS.iter().zip(&self.components) {
                    render_component(ui, theme, bridge, component, update);
                }
            });
    }

    fn render_app(&self, ui: &mut Ui, theme: &Theme, bridge: &mut AppBridge) {
        let app = &self.app;
        let busy = app.state.phase.busy() || bridge.app_update_in_progress();
        card(ui, theme, &tr("Приложение", &[]), |ui| {
            ui.horizontal_wrapped(|ui| {
                version(
                    ui,
                    theme,
                    "Текущая версия:",
                    &format!("v{}", bridge.app_version()),
                );
                if let Some(channel) =
                    channel_buttons(ui, theme, &bridge.release_channel(), "prerelease", busy)
                {
                    bridge.set_release_channel(channel);
                }
            });
            status(
                ui,
                theme,
                app.state.phase,
                app.status(),
                matches!(
                    app.state.phase,
                    Phase::Available | Phase::UpToDate | Phase::Updated | Phase::Error
                ),
            );
            if !app.notes.is_empty() {
                release_notes::render(ui, theme, &app.notes);
            }
            app.state.progress(ui, theme, false);
            ui.horizontal_wrapped(|ui| {
                if check_button(ui, theme, "Проверить обновления", busy) {
                    bridge.check_app_update();
                }
                if app.state.phase.install_visible()
                    && ui
                        .add_enabled(!busy, Button::new(app.install_label()))
                        .clicked()
                {
                    bridge.install_app_update();
                }
            });
        });
    }
}

fn render_component(
    ui: &mut Ui,
    theme: &Theme,
    bridge: &mut AppBridge,
    component: &Component,
    update: &ComponentUpdate,
) {
    let xray = component.key == "xray";
    let state = &update.state;
    let busy = state.phase.busy();
    let mut title = tr(component.title, &[]);
    if component.key == "droute" && !update.version.is_empty() {
        title = format!("{title} · {}", update.version);
    }
    card(ui, theme, &title, |ui| {
        if matches!(component.key, "xray" | "singbox") {
            ui.horizontal_wrapped(|ui| {
                version(ui, theme, "Версия:", &update.version);
                if xray
                    && let Some(channel) =
                        channel_buttons(ui, theme, &bridge.xray_release_channel(), "beta", busy)
                {
                    bridge.set_xray_release_channel(channel);
                }
            });
        }
        let message = match state.phase {
            Phase::Idle => tr(component.idle, &[]),
            Phase::Checking if xray => tr("Проверка обновлений Xray...", &[]),
            Phase::Checking => tr("Проверка обновлений...", &[]),
            Phase::Transferring => {
                let message = tr(
                    if xray {
                        "Обновление Xray..."
                    } else {
                        "Обновление..."
                    },
                    &[],
                );
                if state.percent > 0 {
                    format!("{message} {}%", state.percent)
                } else {
                    message
                }
            }
            _ => tr(&state.message, &[]),
        };
        status(
            ui,
            theme,
            state.phase,
            message,
            component.key != "droute" && matches!(state.phase, Phase::Updated | Phase::Error),
        );
        state.progress(ui, theme, true);
        ui.horizontal_wrapped(|ui| {
            if check_button(ui, theme, component.check, busy) {
                if xray {
                    bridge.check_xray_update();
                } else {
                    bridge.resource_update(component.key, false);
                }
            }
            if state.phase.install_visible()
                && ui
                    .add_enabled(!busy, Button::new(tr(component.install, &[])))
                    .clicked()
            {
                if xray {
                    bridge.update_xray_core();
                } else {
                    bridge.resource_update(component.key, true);
                }
            }
        });
    });
}

fn card(ui: &mut Ui, theme: &Theme, title: &str, content: impl FnOnce(&mut Ui)) {
    Frame::new()
        .fill(theme.card)
        .stroke(Stroke::new(1.0, theme.border_solid))
        .corner_radius(theme.radius as u8)
        .inner_margin(18.0)
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 10.0;
            ui.label(RichText::new(title).size(theme.font_strong as f32).strong());
            content(ui);
        });
}

fn version(ui: &mut Ui, theme: &Theme, label: &str, version: &str) {
    ui.label(RichText::new(tr(label, &[])).color(theme.text_muted));
    let version = if version.is_empty() {
        tr("не найдена", &[])
    } else {
        version.into()
    };
    ui.label(RichText::new(version).strong());
}

fn channel_buttons(
    ui: &mut Ui,
    theme: &Theme,
    channel: &str,
    beta_key: &'static str,
    busy: bool,
) -> Option<&'static str> {
    ui.label(RichText::new(tr("Канал:", &[])).color(theme.text_muted));
    ui.add_enabled_ui(!busy, |ui| {
        let mut selected = None;
        for (key, label) in [("stable", "Stable"), (beta_key, "Beta")] {
            if ui
                .add(Button::selectable(channel == key, tr(label, &[])))
                .clicked()
                && key != channel
            {
                selected = Some(key);
            }
        }
        selected
    })
    .inner
}

fn status(ui: &mut Ui, theme: &Theme, phase: Phase, message: String, strong: bool) {
    let label = RichText::new(message)
        .color(phase.color(theme))
        .size(theme.font_small as f32);
    ui.label(if strong { label.strong() } else { label });
}

fn check_button(ui: &mut Ui, theme: &Theme, label: &str, busy: bool) -> bool {
    ui.add_enabled(
        !busy,
        Button::new(RichText::new(tr(label, &[])).color(theme.accent_text)).fill(theme.accent),
    )
    .clicked()
}

fn text<'a>(payload: &'a Value, key: &str) -> &'a str {
    payload[key].as_str().unwrap_or_default()
}

fn component_version(kind: &str, value: &str) -> String {
    static LUMEN_SUFFIX: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)-lumen(?:\.[0-9a-z.-]+)?$").unwrap());
    if kind == "singbox" {
        LUMEN_SUFFIX.replace(value, "").into_owned()
    } else {
        value.into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn app_update_progress_preserves_release_details_and_new_check_resets_them() {
        let mut page = UpdatesPage::default();
        for state in [
            json!({"phase": "available", "version": "2.1.3", "notes": "Fix", "isDowngrade": true}),
            json!({"phase": "downloading", "percent": 42}),
            json!({"phase": "downloading", "message": "Verifying"}),
        ] {
            page.on_event(&UiEvent::AppUpdateState(state));
        }
        assert_eq!(page.app.state.percent, 42);
        assert_eq!(page.app.target_version, "2.1.3");
        assert_eq!(page.app.notes, "Fix");
        assert!(page.app.downgrade);
        page.on_event(&UiEvent::AppUpdateState(json!({"phase": "checking"})));
        assert_eq!(page.app.state.percent, 0);
        assert!(page.app.notes.is_empty());
        assert!(!page.app.downgrade);
    }

    #[test]
    fn resource_events_before_opening_page_update_only_the_matching_card() {
        let mut page = UpdatesPage::default();
        page.on_event(&UiEvent::ResourceUpdateState(json!({
            "kind": "singbox", "phase": "updated", "version": "1.13.0-lumen.2", "percent": 100
        })));
        page.on_event(&UiEvent::ResourceUpdateState(
            json!({"kind": "geodata", "phase": "checking"}),
        ));
        assert!(!page.initialized);
        assert_eq!(page.components[1].version, "1.13.0");
        assert_eq!(page.components[1].state.percent, 100);
        assert!(page.components[2].state.phase.busy());
        assert!(!page.components[0].state.phase.busy());
        assert!(!page.components[3].state.phase.busy());
    }
}
