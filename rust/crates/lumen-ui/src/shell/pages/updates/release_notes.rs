//! Render the inline formatting and links supported by `UpdatesPage.qml`.

use std::sync::LazyLock;

use egui::{Frame, RichText, ScrollArea, Stroke, Ui};
use regex::Regex;

use crate::shell::theme::{Theme, semibold};

static INLINE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"`([^`]+)`|\[([^\]]+)\]\((https?://[^)\s]+)\)|\*\*([^*]+)\*\*|\*([^*]+)\*|_([^_]+)_",
    )
    .unwrap()
});

pub(super) fn render(ui: &mut Ui, theme: &Theme, notes: &str) {
    Frame::new()
        .stroke(Stroke::new(1.0, theme.border_solid))
        .corner_radius(theme.radius_small as u8)
        .inner_margin(9.0)
        .show(ui, |ui| {
            ScrollArea::vertical()
                .id_salt("app-release-notes")
                .min_scrolled_height(100.0)
                .max_height(100.0)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    let spacing = ui.spacing_mut();
                    spacing.item_spacing.y = 2.0;
                    spacing.interact_size.y = 0.0;
                    let normalized = notes.replace("\r\n", "\n").replace('\r', "\n");
                    for line in normalized.split('\n') {
                        if line.trim().is_empty() {
                            ui.add_space(12.0);
                            continue;
                        }
                        ui.horizontal_wrapped(|ui| {
                            ui.spacing_mut().item_spacing.x = 0.0;
                            let (size, heading, text) = if let Some(text) = line.strip_prefix("# ")
                            {
                                (15.0, true, text)
                            } else if let Some(text) = line.strip_prefix("## ") {
                                (14.0, true, text)
                            } else if let Some(text) = line.strip_prefix("- ") {
                                ui.label(RichText::new("• ").size(12.0).color(theme.text_muted));
                                (12.0, false, text)
                            } else {
                                (12.0, false, line)
                            };
                            inline(ui, theme, text, size, heading);
                        });
                    }
                });
        });
}

fn inline(ui: &mut Ui, theme: &Theme, text: &str, size: f32, heading: bool) {
    let base = |value: &str| {
        let label = RichText::new(value).size(size).color(if heading {
            theme.text
        } else {
            theme.text_muted
        });
        if heading {
            label.font(semibold(size))
        } else {
            label
        }
    };
    let mut offset = 0;
    for captures in INLINE.captures_iter(text) {
        let matched = captures.get(0).expect("a complete inline match");
        if offset < matched.start() {
            ui.label(base(&text[offset..matched.start()]));
        }
        if let Some(code) = captures.get(1) {
            ui.label(base(code.as_str()).monospace().color(theme.text));
        } else if let (Some(label), Some(link)) = (captures.get(2), captures.get(3)) {
            ui.hyperlink_to(base(label.as_str()).color(theme.accent), link.as_str());
        } else if let Some(bold) = captures.get(4) {
            ui.label(base(bold.as_str()).font(semibold(size)));
        } else if let Some(italic) = captures.get(5).or_else(|| captures.get(6)) {
            ui.label(base(italic.as_str()).italics());
        }
        offset = matched.end();
    }
    if offset < text.len() {
        ui.label(base(&text[offset..]));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn release_notes_activate_only_http_links() {
        let text =
            "[remote](https://example.test) [local](file:///secret) [script](javascript:alert(1))";
        let links: Vec<_> = INLINE
            .captures_iter(text)
            .filter_map(|m| m.get(3))
            .map(|m| m.as_str())
            .collect();
        assert_eq!(links, ["https://example.test"]);
    }
}
