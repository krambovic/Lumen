//! Application, core and resource update cards from `UpdatesPage.qml`.

mod release_notes;

use std::sync::LazyLock;

use egui::{
    Align, Align2, Button, Color32, FontId, Layout, RichText, ScrollArea, Sense, Stroke, Ui,
    vec2,
};
use lumen_core::i18n::tr;
use regex::Regex;
use serde_json::Value;

use super::PageContext;
use crate::shell::theme::{Theme, semibold};
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
        let (track, response) =
            ui.allocate_exact_size(vec2(ui.available_width(), 10.0), Sense::hover());
        let indeterminate = self.phase == Phase::Checking || (unknown_percent && self.percent == 0);
        response.widget_info(|| {
            let role = egui::WidgetType::ProgressIndicator;
            if indeterminate {
                egui::WidgetInfo::new(role)
            } else {
                egui::WidgetInfo::labeled(role, ui.is_enabled(), format!("{}%", self.percent))
            }
        });
        if !ui.is_rect_visible(track) {
            return;
        }
        ui.painter()
            .rect_filled(track, 0.0, theme.text_faint.gamma_multiply(0.45));
        let (x, width) = if indeterminate {
            let phase = ui.input(|input| (input.time / 1.2).fract() as f32);
            let width = track.width() * 0.25;
            ui.request_repaint();
            (
                track.left() - width + phase * (track.width() + width),
                width,
            )
        } else {
            (track.left(), track.width() * self.percent as f32 / 100.0)
        };
        let segment =
            egui::Rect::from_min_size(egui::pos2(x, track.top()), vec2(width, track.height()));
        ui.painter()
            .rect_filled(segment.intersect(track), 0.0, theme.accent);
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
            // Probed on a worker thread; the cards fill in when it is done.
            if let Some(initial) = bridge.updates_initial_state() {
                for (component, update) in COMPONENTS.iter().zip(&mut self.components) {
                    if update.version.is_empty() {
                        update.version =
                            component_version(component.key, text(&initial, component.version_key));
                    }
                }
                self.initialized = true;
            } else {
                ui.ctx().request_repaint_after(std::time::Duration::from_millis(50));
            }
        }
        ScrollArea::vertical()
            .id_salt("updates-page")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = theme.spacing_large as f32;
                ui.label(
                    RichText::new(tr("Обновления", &[])).font(semibold(theme.font_title as f32)),
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
            if let Some(channel) = version_and_channel(
                ui,
                theme,
                ("Текущая версия:", &format!("v{}", bridge.app_version())),
                (&bridge.release_channel(), "prerelease"),
                busy,
            ) {
                bridge.set_release_channel(channel);
            }
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
            action_row(ui, |ui| {
                if action_button(
                    ui,
                    theme,
                    "\u{e72c}",
                    tr("Проверить обновления", &[]),
                    true,
                    !busy,
                ) {
                    bridge.check_app_update();
                }
                if app.state.phase.install_visible()
                    && action_button(ui, theme, "\u{e896}", app.install_label(), false, !busy)
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
        if xray {
            if let Some(channel) = version_and_channel(
                ui,
                theme,
                ("Версия:", &update.version),
                (&bridge.xray_release_channel(), "beta"),
                busy,
            ) {
                bridge.set_xray_release_channel(channel);
            }
        } else if component.key == "singbox" {
            version(ui, theme, "Версия:", &update.version);
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
        action_row(ui, |ui| {
            if action_button(ui, theme, "\u{e72c}", tr(component.check, &[]), true, !busy) {
                if xray {
                    bridge.check_xray_update();
                } else {
                    bridge.resource_update(component.key, false);
                }
            }
            if state.phase.install_visible()
                && action_button(
                    ui,
                    theme,
                    "\u{e896}",
                    tr(component.install, &[]),
                    false,
                    !busy,
                )
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
    crate::shell::widgets::card_frame(theme, 18.0)
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 10.0;
            ui.label(RichText::new(title).font(semibold(theme.font_strong as f32)));
            content(ui);
        });
}

fn version(ui: &mut Ui, theme: &Theme, label: &str, version: &str) {
    ui.allocate_ui_with_layout(
        vec2(ui.available_width(), theme.font_normal as f32 + 4.0),
        Layout::left_to_right(Align::Center),
        |ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            ui.label(
                RichText::new(tr(label, &[]))
                    .size(theme.font_normal as f32)
                    .color(theme.text_muted),
            );
            let version = if version.is_empty() {
                tr("не найдена", &[])
            } else {
                version.into()
            };
            ui.label(RichText::new(version).font(semibold(theme.font_normal as f32)));
        },
    );
}

fn version_and_channel(
    ui: &mut Ui,
    theme: &Theme,
    (label, current_version): (&str, &str),
    (channel, beta_key): (&str, &'static str),
    busy: bool,
) -> Option<&'static str> {
    if ui.available_width() < 430.0 {
        version(ui, theme, label, current_version);
        return ui
            .horizontal(|ui| {
                ui.label(
                    RichText::new(tr("Канал:", &[]))
                        .size(theme.font_normal as f32)
                        .color(theme.text_muted),
                );
                channel_buttons(ui, theme, channel, beta_key, busy)
            })
            .inner;
    }
    ui.allocate_ui_with_layout(
        vec2(ui.available_width(), 30.0),
        Layout::right_to_left(Align::Center),
        |ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            let selected = channel_buttons(ui, theme, channel, beta_key, busy);
            ui.label(
                RichText::new(tr("Канал:", &[]))
                    .size(theme.font_normal as f32)
                    .color(theme.text_muted),
            );
            version(ui, theme, label, current_version);
            selected
        },
    )
    .inner
}

fn channel_buttons(
    ui: &mut Ui,
    theme: &Theme,
    channel: &str,
    beta_key: &'static str,
    busy: bool,
) -> Option<&'static str> {
    let segments = [("stable", "Stable"), (beta_key, "Beta")].map(|(key, label)| {
        let font = if key == channel {
            semibold(theme.font_small as f32)
        } else {
            FontId::proportional(theme.font_small as f32)
        };
        let label = tr(label, &[]);
        let width = (ui
            .painter()
            .layout_no_wrap(label.clone(), font.clone(), theme.text)
            .size()
            .x
            + 26.0)
            .max(76.0);
        (key, label, font, width)
    });
    let width = segments.iter().map(|segment| segment.3).sum::<f32>() + 4.0;
    let (track, _) = ui.allocate_exact_size(vec2(width, 30.0), Sense::hover());
    let painter = ui.painter();
    let fade = |color: Color32| {
        if busy {
            color.gamma_multiply(0.5)
        } else {
            color
        }
    };
    painter.rect(
        track,
        15.0,
        fade(theme.control_fill),
        Stroke::new(1.0, fade(theme.border_solid)),
        egui::StrokeKind::Inside,
    );
    let mut position = track.min + vec2(2.0, 2.0);
    let mut selection = None;
    for (key, label, font, width) in segments {
        let rect = egui::Rect::from_min_size(position, vec2(width, 26.0));
        position.x += width;
        let active = key == channel;
        let response = ui.interact(
            rect,
            ui.scope_id().with(("update-channel", beta_key, key)),
            if busy { Sense::hover() } else { Sense::click() },
        );
        response.widget_info(|| {
            egui::WidgetInfo::selected(egui::WidgetType::Button, !busy, active, &label)
        });
        if active {
            painter.rect_filled(rect, 13.0, fade(theme.accent));
        } else if response.hovered() && !busy {
            painter.rect_filled(rect, 13.0, theme.control_fill_hover);
        }
        let text_color = if active {
            theme.accent_text
        } else {
            theme.text_muted
        };
        painter.text(
            rect.center(),
            Align2::CENTER_CENTER,
            label,
            font,
            fade(text_color),
        );
        if !busy
            && response
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .clicked()
            && !active
        {
            selection = Some(key);
        }
    }
    selection
}

fn status(ui: &mut Ui, theme: &Theme, phase: Phase, message: String, strong: bool) {
    let label = RichText::new(message)
        .color(phase.color(theme))
        .size(theme.font_small as f32);
    ui.label(if strong {
        label.font(semibold(theme.font_small as f32))
    } else {
        label
    });
}

fn action_row(ui: &mut Ui, content: impl FnOnce(&mut Ui)) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        ui.spacing_mut().button_padding = vec2(14.0, 0.0);
        content(ui);
    });
}

fn action_button(
    ui: &mut Ui,
    theme: &Theme,
    icon: &str,
    label: String,
    accent: bool,
    enabled: bool,
) -> bool {
    let size = theme.font_normal as f32;
    let color = if accent {
        theme.accent_text
    } else {
        theme.text
    };
    let mut text = egui::text::LayoutJob::default();
    text.append(
        &format!("{icon}  "),
        0.0,
        egui::TextFormat::simple(FontId::proportional(size), color),
    );
    let font = if accent {
        semibold(size)
    } else {
        FontId::proportional(size)
    };
    text.append(&label, 0.0, egui::TextFormat::simple(font, color));
    let mut button = Button::new(text).min_size(vec2(0.0, theme.control_height as f32));
    if accent {
        button = button.fill(theme.accent).stroke(Stroke::NONE);
    }
    ui.add_enabled(enabled, button).clicked()
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

    #[test]
    fn channel_switch_uses_the_component_key_and_ignores_clicks_while_busy() {
        let ctx = egui::Context::default();
        let theme = Theme::resolve(&Default::default());
        crate::shell::theme::apply_fonts(&ctx, &theme);
        let mut beta = egui::Pos2::ZERO;
        let mut frame = |events, busy| {
            let mut selection = None;
            let input = egui::RawInput {
                events,
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |ui| {
                beta = ui.next_widget_position() + vec2(115.0, 15.0);
                selection = channel_buttons(ui, &theme, "stable", "prerelease", busy);
            });
            output.textures_delta.clear();
            (beta, selection)
        };
        let (beta, _) = frame(vec![], false);
        let click = |pressed| egui::Event::PointerButton {
            pos: beta,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        frame(vec![egui::Event::PointerMoved(beta), click(true)], false);
        assert_eq!(frame(vec![click(false)], false).1, Some("prerelease"));
        frame(vec![], true);
        frame(vec![click(true)], true);
        assert_eq!(frame(vec![click(false)], true).1, None);
    }
}
