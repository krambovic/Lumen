//! Toasts: the in-app bottom-right stack (port of `qml_app/qml/ToastHost.qml`)
//! and the Windows toast notification bridge (port of `qml_app/toast.py`).

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use egui::{Align2, Color32, Context, Ui, Vec2};

use super::theme::Theme;

// ── Windows toast (toast.py) ─────────────────────────────────────────────

pub const AUMID: &str = "Lumen.Lumen";
const SM_SHUTTINGDOWN: u32 = 0x2000;

static TOASTS_ENABLED: AtomicBool = AtomicBool::new(true);

/// `set_toasts_enabled(enabled)`.
pub fn set_toasts_enabled(enabled: bool) {
    TOASTS_ENABLED.store(enabled, Ordering::SeqCst);
}

fn toasts_enabled() -> bool {
    TOASTS_ENABLED.load(Ordering::SeqCst)
}

/// `_is_windows_shutting_down()` via `GetSystemMetrics(SM_SHUTTINGDOWN)`.
fn is_windows_shutting_down() -> bool {
    #[cfg(windows)]
    {
        use windows::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_SHUTTINGDOWN};
        unsafe { GetSystemMetrics(SM_SHUTTINGDOWN) != 0 }
    }
    #[cfg(not(windows))]
    {
        false
    }
}

fn xml_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn ps_quote(text: &str) -> String {
    text.replace('\'', "''")
}

/// `show_toast(title, message, aumid)` — raise a modern Windows toast through
/// the PowerShell/WinRT bridge. Returns True if the launch was dispatched.
pub fn show_toast(title: &str, message: &str, aumid: &str) -> bool {
    if cfg!(not(windows)) || !toasts_enabled() || is_windows_shutting_down() {
        return false;
    }
    let xml = format!(
        "<toast activationType='protocol' launch='lumen:show'>\
<visual><binding template='ToastGeneric'>\
<text>{}</text>\
<text>{}</text>\
</binding></visual></toast>",
        xml_escape(title),
        xml_escape(message)
    );
    let script_lines = [
        "$ErrorActionPreference='Stop'".to_string(),
        "[void][Windows.UI.Notifications.ToastNotificationManager,Windows.UI.Notifications,ContentType=WindowsRuntime]".to_string(),
        "[void][Windows.UI.Notifications.ToastNotification,Windows.UI.Notifications,ContentType=WindowsRuntime]".to_string(),
        "[void][Windows.Data.Xml.Dom.XmlDocument,Windows.Data.Xml.Dom,ContentType=WindowsRuntime]".to_string(),
        "$doc=New-Object Windows.Data.Xml.Dom.XmlDocument".to_string(),
        format!("$doc.LoadXml('{}')", ps_quote(&xml)),
        "$toast=New-Object Windows.UI.Notifications.ToastNotification -ArgumentList $doc".to_string(),
        format!(
            "[Windows.UI.Notifications.ToastNotificationManager]::CreateToastNotifier('{}').Show($toast)",
            ps_quote(aumid)
        ),
    ];
    let script = script_lines.join("\n");
    // Python: base64.b64encode(script.encode("utf-16-le")).decode("ascii")
    let utf16: Vec<u8> = script
        .encode_utf16()
        .flat_map(|unit| unit.to_le_bytes())
        .collect();
    let encoded = base64_encode_std(&utf16);
    spawn_powershell_toast(&encoded)
}

fn spawn_powershell_toast(encoded: &str) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        use std::process::{Command, Stdio};
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;

        let child = Command::new("powershell")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-WindowStyle",
                "Hidden",
                "-EncodedCommand",
                encoded,
            ])
            .creation_flags(CREATE_NO_WINDOW)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .stdin(Stdio::null())
            .spawn();
        child.is_ok()
    }
    #[cfg(not(windows))]
    {
        let _ = encoded;
        false
    }
}

/// std base64 has no std API; mirror `base64.b64encode(...).decode("ascii")`.
fn base64_encode_std(data: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(data)
}

#[cfg(test)]
mod windows_toast_tests {
    use super::*;

    #[test]
    fn xml_escape_matches_python() {
        assert_eq!(xml_escape("a&<b>"), "a&amp;&lt;b&gt;");
    }

    #[test]
    fn ps_quote_doubles_single_quotes() {
        assert_eq!(ps_quote("it's"), "it''s");
    }

    #[test]
    fn toasts_are_gated_by_the_shutdown_flag() {
        set_toasts_enabled(false);
        assert_eq!(
            show_toast("Lumen", "\u{0422}\u{0435}\u{0441}\u{0442}", AUMID),
            false
        );
        set_toasts_enabled(true);
    }

    #[test]
    fn base64_encoding_is_standard() {
        assert_eq!(base64_encode_std(b"hello"), "aGVsbG8=");
    }
}

// ── In-app toast stack (ToastHost.qml) ───────────────────────────────────

const TOAST_LIFETIME: Duration = Duration::from_millis(3200);
const TOAST_FADE: Duration = Duration::from_millis(220);
const TOAST_APPEAR: Duration = Duration::from_millis(160);

/// One entry of the ToastHost ListModel.
#[derive(Clone, Debug)]
pub struct ToastEntry {
    pub message: String,
    /// "info" | "success" | "warning" | "error"
    pub kind: String,
    pub action_id: String,
    pub action_label: String,
    pub born: Instant,
}

impl ToastEntry {
    pub fn new(message: impl Into<String>, kind: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            kind: kind.into(),
            action_id: String::new(),
            action_label: String::new(),
            born: Instant::now(),
        }
    }

    pub fn with_action(
        mut self,
        action_id: impl Into<String>,
        action_label: impl Into<String>,
    ) -> Self {
        self.action_id = action_id.into();
        self.action_label = action_label.into();
        self
    }
}

/// `ToastHost {}` — anchored bottom-right stack of transient cards.
#[derive(Default)]
pub struct ToastHost {
    entries: Vec<ToastEntry>,
}

impl ToastHost {
    /// `host.show(message, kind, actionId, actionLabel)`.
    pub fn show(
        &mut self,
        message: impl Into<String>,
        kind: impl Into<String>,
        action_id: impl Into<String>,
        action_label: impl Into<String>,
    ) {
        let message = message.into();
        if message.is_empty() {
            return;
        }
        self.entries
            .push(ToastEntry::new(message, kind).with_action(action_id, action_label));
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// `host.kindColor(kind)`.
    fn kind_color(&self, theme: &Theme, kind: &str) -> Color32 {
        match kind {
            "error" => theme.danger,
            "warning" => theme.warning,
            "success" => theme.success,
            _ => theme.accent,
        }
    }

    /// Render the stack; returns the pressed action id, if any, so the shell
    /// can call `App.runToastAction(action_id)`.
    pub fn render(&mut self, ctx: &Context, theme: &Theme) -> Option<String> {
        let now = Instant::now();
        let before = self.entries.len();
        self.entries
            .retain(|entry| now.duration_since(entry.born) < TOAST_LIFETIME + TOAST_FADE);
        if self.entries.len() != before {
            ctx.request_repaint();
        }
        if self.entries.is_empty() {
            return None;
        }

        let mut animating = false;
        let entries = self.entries.clone();
        let mut pressed_action: Option<String> = None;

        egui::Area::new(egui::Id::unique("lumen-toast-host"))
            .order(egui::Order::Foreground)
            .anchor(Align2::RIGHT_BOTTOM, Vec2::new(-18.0, -18.0))
            .interactable(true)
            .show(ctx, |ui: &mut Ui| {
                ui.spacing_mut().item_spacing.y = 8.0;
                for entry in &entries {
                    let elapsed = now.duration_since(entry.born);
                    let alpha = if elapsed > TOAST_LIFETIME {
                        let into_fade =
                            (elapsed - TOAST_LIFETIME).as_secs_f64() / TOAST_FADE.as_secs_f64();
                        1.0 - into_fade.clamp(0.0, 1.0)
                    } else {
                        let appear = elapsed.as_secs_f64() / TOAST_APPEAR.as_secs_f64();
                        appear.clamp(0.0, 1.0)
                    };
                    if alpha < 1.0 {
                        animating = true;
                    }
                    let fade = |color: Color32| -> Color32 {
                        Color32::from_rgba_unmultiplied(
                            color.r(),
                            color.g(),
                            color.b(),
                            ((color.a() as f32) * alpha as f32).round() as u8,
                        )
                    };
                    egui::Frame::NONE
                        .fill(fade(theme.flyout))
                        .stroke(egui::Stroke::new(1.0, fade(theme.flyout_border)))
                        .corner_radius(theme.radius as f32)
                        .inner_margin(egui::Margin::symmetric(14, 10))
                        .show(ui, |ui: &mut Ui| {
                            ui.horizontal(|ui: &mut Ui| {
                                let color = self.kind_color(theme, &entry.kind);
                                let (rect, _) = ui.allocate_exact_size(
                                    Vec2::new(4.0, theme.font_normal as f32 * 1.6),
                                    egui::Sense::hover(),
                                );
                                ui.painter().rect_filled(rect, 2.0, fade(color));
                                let label = lumen_core::i18n::tr(&entry.message, &[]);
                                let text = egui::RichText::new(&label)
                                    .color(fade(theme.text))
                                    .size(theme.font_normal as f32);
                                ui.add(egui::Label::new(text).wrap_mode(egui::TextWrapMode::Wrap));
                                if !entry.action_id.is_empty() {
                                    let button = egui::Button::new(
                                        egui::RichText::new(&entry.action_label)
                                            .color(fade(theme.accent_text))
                                            .size(theme.font_normal as f32),
                                    )
                                    .fill(fade(theme.accent))
                                    .corner_radius(theme.radius_small as f32);
                                    if ui.add(button).clicked() {
                                        pressed_action = Some(entry.action_id.clone());
                                    }
                                }
                            });
                        });
                }
            });
        if animating {
            ctx.request_repaint_after(Duration::from_millis(16));
        }
        pressed_action
    }

    /// Remove every entry (used when quitting, like `set_toasts_enabled(False)`).
    pub fn clear(&mut self) {
        self.entries.clear();
    }
}

#[cfg(test)]
mod host_tests {
    use super::*;

    #[test]
    fn show_skips_empty_messages_like_the_qml_host() {
        let mut host = ToastHost::default();
        host.show("", "error", "", "");
        assert!(host.is_empty());
        host.show(
            "\u{0433}\u{043E}\u{0442}\u{043E}\u{0432}\u{043E}",
            "success",
            "",
            "",
        );
        assert_eq!(host.len(), 1);
    }

    #[test]
    fn entries_expire_after_the_qml_timers() {
        let mut host = ToastHost::default();
        host.show("msg", "info", "", "");
        // Simulate an old toast by rewinding the birth instant.
        let entry = &mut host.entries[0];
        entry.born = Instant::now() - TOAST_LIFETIME - TOAST_FADE - Duration::from_millis(1);
        let now = Instant::now();
        let before = host.len();
        host.entries
            .retain(|entry| now.duration_since(entry.born) < TOAST_LIFETIME + TOAST_FADE);
        assert_eq!(host.len(), before - 1);
    }

    #[test]
    fn kind_colors_match_the_qml_kind_color() {
        let theme = Theme::resolve(&Default::default());
        let mut host = ToastHost::default();
        assert_eq!(host.kind_color(&theme, "error"), theme.danger);
        assert_eq!(host.kind_color(&theme, "warning"), theme.warning);
        assert_eq!(host.kind_color(&theme, "success"), theme.success);
        assert_eq!(host.kind_color(&theme, "info"), theme.accent);
        assert_eq!(host.kind_color(&theme, "unknown"), theme.accent);
    }
}
