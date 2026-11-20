//! The servers page source picker: `sourceButton` and `sourcePopup` in
//! `NodesPage.qml` (one control for "all servers", subscriptions and groups).

use egui::{
    Align2, Color32, FontId, Id, Popup, PopupCloseBehavior, Rect, RectAlign, ScrollArea, Sense,
    Stroke, Ui, pos2, text::LayoutJob, vec2,
};
use lumen_core::i18n::tr;
use serde_json::Value;

use super::{Dialog, NodesPage, field, subscription};
use crate::shell::motion;
use crate::shell::theme::{Theme, semibold};
use crate::shell::widgets::{self, ButtonKind, FluentButton};
use crate::view_models::AppBridge;

/// One row of `sourceEntries()`.
enum Entry {
    All { active: bool },
    Header(String),
    Sub { index: usize, title: String, segments: Vec<(String, Color32)>, bar: Option<(f32, Color32)>, count: String, active: bool },
    Group { name: String, active: bool, deletable: bool },
}

enum Choice {
    All,
    Sub(usize),
    Group(String),
    DeleteGroup(String),
    Import,
    NewGroup,
}

impl NodesPage {
    /// `sourceButton`: title (DemiBold, at most 45% of the width), the
    /// subscription line or "Серверов: N" in fontSmall, and the chevron.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn source_picker(
        &mut self,
        ui: &mut Ui,
        theme: &Theme,
        bridge: &mut AppBridge,
        subscriptions: &[Value],
        selected_sub: Option<&Value>,
        group: &str,
        width: f32,
    ) {
        let height = theme.control_height as f32;
        let (rect, response) = ui.allocate_exact_size(vec2(width, height), Sense::click());
        let popup_id = Popup::default_response_id(&response);
        let open = Popup::is_id_open(ui.ctx(), popup_id);
        let fill = motion::follow_color(
            ui.ctx(),
            theme,
            response.id.with("fill"),
            if response.hovered() || open { theme.control_fill_hover } else { theme.control_fill },
            theme.anim_fast as f32,
            motion::linear,
        );
        widgets::bordered_rect(
            ui.painter(),
            rect,
            theme.radius as f32,
            fill,
            if open { theme.accent } else { theme.border_solid },
        );
        if response.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }

        let title = if let Some(sub) = selected_sub {
            let name = field(sub, "name");
            if name.is_empty() { tr("Подписка", &[]) } else { name.to_string() }
        } else if group.is_empty() {
            tr("Все серверы", &[])
        } else {
            group.to_string()
        };
        let segments = selected_sub
            .map(|sub| subscription::Status::new(sub).segments(theme))
            .filter(|segments| !segments.is_empty())
            .unwrap_or_else(|| {
                let count = bridge.node_model.row_count().to_string();
                vec![(tr("Серверов: {count}", &[("count", &count)]), theme.text_muted)]
            });
        // RowLayout: 12 px left, 10 px right, 10 px spacing.
        let chevron = ui.painter().layout_no_wrap("\u{e70d}".into(), FontId::proportional(11.0), theme.text_muted);
        let inner_left = rect.left() + 12.0;
        let inner_right = rect.right() - 10.0;
        let title = widgets::elided(
            ui,
            &title,
            semibold(theme.font_normal as f32),
            theme.text,
            width * 0.45,
        );
        let painter = ui.painter();
        painter.galley(pos2(inner_left, rect.center().y - title.size().y / 2.0), title.clone(), theme.text);
        let line_left = inner_left + title.size().x + 10.0;
        let line_width = (inner_right - chevron.size().x - 10.0 - line_left).max(0.0);
        let line = line_galley(ui, theme, &segments, line_width);
        painter.galley(pos2(line_left, rect.center().y - line.size().y / 2.0), line, theme.text_muted);
        painter.galley(
            pos2(inner_right - chevron.size().x, rect.center().y - chevron.size().y / 2.0),
            chevron,
            theme.text_muted,
        );

        let entries = entries(theme, bridge, subscriptions, selected_sub, group);
        let mut choice = None;
        Popup::menu(&response)
            .close_behavior(PopupCloseBehavior::CloseOnClickOutside)
            // `x: sourceButton.width - width`, `y: sourceButton.height + 6`.
            .align(RectAlign::BOTTOM_END)
            .gap(6.0)
            .width(360.0 - 12.0)
            .frame(
                egui::Frame::new()
                    .fill(theme.flyout)
                    .stroke(Stroke::new(1.0, theme.flyout_border))
                    .corner_radius(8)
                    .inner_margin(6),
            )
            .show(|ui| {
                ui.set_width(360.0 - 12.0);
                ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                ScrollArea::vertical()
                    .id_salt("nodes-source-list")
                    .max_height(440.0)
                    .show(ui, |ui| {
                        for (index, entry) in entries.iter().enumerate() {
                            if index > 0 {
                                ui.add_space(2.0);
                            }
                            if let Some(picked) = entry_row(ui, theme, entry, index) {
                                choice = Some(picked);
                            }
                        }
                    });
                // ColumnLayout spacing 4, divider with a 2 px top margin.
                ui.add_space(6.0);
                let (line, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
                ui.painter().rect_filled(line, 0.0, theme.divider);
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    if FluentButton::new(ButtonKind::Ghost, "\u{e71b}", &tr("Импорт подписки", &[]))
                        .show(ui, theme)
                        .clicked()
                    {
                        choice = Some(Choice::Import);
                    }
                    if FluentButton::new(ButtonKind::Ghost, "\u{e8f4}", &tr("Новая группа", &[]))
                        .show(ui, theme)
                        .clicked()
                    {
                        choice = Some(Choice::NewGroup);
                    }
                });
                if choice.is_some() {
                    ui.close();
                }
            });

        match choice {
            Some(Choice::All) => self.choose_source(bridge, "", ""),
            Some(Choice::Sub(index)) => {
                if let Some(sub) = subscriptions.get(index) {
                    self.choose_source(bridge, subscription::group(sub), field(sub, "id"));
                }
            }
            Some(Choice::Group(name)) => self.choose_source(bridge, &name, ""),
            Some(Choice::DeleteGroup(name)) => self.open(Dialog::DeleteGroup(name)),
            Some(Choice::Import) => self.open(Dialog::Subscription),
            Some(Choice::NewGroup) => self.open(Dialog::Group),
            None => {}
        }
    }
}

/// `sourceEntries()`: all servers, the subscriptions and the manual groups
/// that do not belong to a subscription.
fn entries(
    theme: &Theme,
    bridge: &AppBridge,
    subscriptions: &[Value],
    selected_sub: Option<&Value>,
    group: &str,
) -> Vec<Entry> {
    let mut rows = vec![Entry::All { active: selected_sub.is_none() && group.is_empty() }];
    if !subscriptions.is_empty() {
        rows.push(Entry::Header(tr("Подписки", &[])));
    }
    let selected_id = selected_sub.map(|sub| field(sub, "id")).unwrap_or_default();
    for (index, sub) in subscriptions.iter().enumerate() {
        let status = subscription::Status::new(sub);
        let name = field(sub, "name");
        rows.push(Entry::Sub {
            index,
            title: if name.is_empty() { tr("Подписка", &[]) } else { name.to_string() },
            segments: status.segments(theme),
            bar: status.bar().map(|fraction| (fraction, status.traffic_color(theme))),
            count: sub["node_count"].as_u64().unwrap_or(0).to_string(),
            active: !selected_id.is_empty() && selected_id == field(sub, "id"),
        });
    }
    let groups: Vec<String> = bridge
        .group_options()
        .into_iter()
        .filter(|name| !subscriptions.iter().any(|sub| subscription::group(sub) == name))
        .collect();
    if !groups.is_empty() {
        rows.push(Entry::Header(tr("Группы", &[])));
    }
    for name in groups {
        rows.push(Entry::Group {
            active: selected_sub.is_none() && group == name,
            deletable: !name.eq_ignore_ascii_case("default"),
            name,
        });
    }
    rows
}

fn line_galley(
    ui: &Ui,
    theme: &Theme,
    segments: &[(String, Color32)],
    width: f32,
) -> std::sync::Arc<egui::Galley> {
    let mut job = LayoutJob::default();
    for (text, color) in segments {
        job.append(
            text,
            0.0,
            egui::TextFormat {
                font_id: FontId::proportional(theme.font_small as f32),
                color: *color,
                ..Default::default()
            },
        );
    }
    job.wrap = egui::text::TextWrapping::truncate_at_width(width);
    ui.painter().layout_job(job)
}

/// One popup row: 28 px headers (faint, bottom-aligned at x 10), 40 px rows
/// and 54 px subscription rows with a subtitle; radius 6, `accentSoft` when
/// active with a 3 x 16 accent marker, `cardHover` on hover.
fn entry_row(ui: &mut Ui, theme: &Theme, entry: &Entry, index: usize) -> Option<Choice> {
    let width = ui.available_width();
    if let Entry::Header(title) = entry {
        let (rect, _) = ui.allocate_exact_size(vec2(width, 28.0), Sense::hover());
        let galley = ui.painter().layout_no_wrap(
            title.clone(),
            FontId::proportional(theme.font_small as f32),
            theme.text_faint,
        );
        ui.painter().galley(
            pos2(rect.left() + 10.0, rect.bottom() - 4.0 - galley.size().y),
            galley,
            theme.text_faint,
        );
        return None;
    }
    let (title, segments, bar, count, active) = match entry {
        Entry::All { active } => (tr("Все серверы", &[]), &[][..], None, None, *active),
        Entry::Sub { title, segments, bar, count, active, .. } => {
            (title.clone(), segments.as_slice(), *bar, Some(count.as_str()), *active)
        }
        Entry::Group { name, active, .. } => (name.clone(), &[][..], None, None, *active),
        Entry::Header(_) => unreachable!(),
    };
    let height = if matches!(entry, Entry::Sub { .. }) && !segments.is_empty() { 54.0 } else { 40.0 };
    let (rect, response) = ui.allocate_exact_size(vec2(width, height), Sense::click());
    let hovered = ui.rect_contains_pointer(rect);
    let fill = if active {
        theme.accent_soft
    } else if hovered {
        theme.card_hover
    } else {
        Color32::TRANSPARENT
    };
    let painter = ui.painter();
    painter.rect_filled(rect, 6.0, fill);
    if active {
        painter.rect_filled(
            Rect::from_center_size(pos2(rect.left() + 1.5, rect.center().y), vec2(3.0, 16.0)),
            1.5,
            theme.accent,
        );
    }
    if hovered {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }

    // Right side: the delete button (groups, on hover) and the server count.
    let mut right = rect.right() - 8.0;
    let mut choice = None;
    if let Entry::Group { name, deletable: true, .. } = entry
        && hovered
    {
        let button = Rect::from_min_size(pos2(right - 28.0, rect.center().y - 14.0), vec2(28.0, 28.0));
        let delete = ui.interact(button, Id::unique(("nodes-source-delete", index)), Sense::click());
        if delete.hovered() {
            ui.painter().rect_filled(button, theme.radius_small as f32, theme.control_fill_hover);
        }
        ui.painter().text(
            button.center(),
            Align2::CENTER_CENTER,
            "\u{e74d}",
            FontId::proportional(theme.font_normal as f32),
            theme.text,
        );
        let delete = delete.on_hover_text(tr("Удалить выбранную группу", &[]));
        if delete.clicked() {
            choice = Some(Choice::DeleteGroup(name.clone()));
        }
        right -= 28.0 + 10.0;
    }
    if let Some(count) = count {
        let galley = ui.painter().layout_no_wrap(
            count.to_string(),
            FontId::proportional(theme.font_small as f32),
            theme.text_faint,
        );
        right -= galley.size().x;
        ui.painter().galley(pos2(right, rect.center().y - galley.size().y / 2.0), galley, theme.text_faint);
        right -= 10.0;
    }

    // Column (spacing 3): title, usage bar, subtitle — centred on the row.
    let left = rect.left() + 10.0;
    let column_width = (right - left).max(0.0);
    let title = widgets::elided(ui, &title, FontId::proportional(theme.font_normal as f32), theme.text, column_width);
    let subtitle = (!segments.is_empty()).then(|| line_galley(ui, theme, segments, column_width));
    let column = title.size().y
        + bar.map_or(0.0, |_| 3.0 + 4.0)
        + subtitle.as_ref().map_or(0.0, |galley| 3.0 + galley.size().y);
    let mut y = rect.center().y - column / 2.0;
    let painter = ui.painter();
    let title_height = title.size().y;
    painter.galley(pos2(left, y), title, theme.text);
    y += title_height;
    if let Some((fraction, color)) = bar {
        y += 3.0;
        let track = Rect::from_min_size(pos2(left, y), vec2(column_width, 4.0));
        let track_color = if theme.dark { Color32::from_black_alpha(71) } else { Color32::from_black_alpha(20) };
        painter.rect_filled(track, 2.0, track_color);
        painter.rect_filled(
            Rect::from_min_size(track.min, vec2(column_width * fraction.min(1.0), 4.0)),
            2.0,
            color,
        );
        y += 4.0;
    }
    if let Some(subtitle) = subtitle {
        painter.galley(pos2(left, y + 3.0), subtitle, theme.text_muted);
    }
    if choice.is_none() && response.clicked() {
        choice = Some(match entry {
            Entry::All { .. } => Choice::All,
            Entry::Sub { index, .. } => Choice::Sub(*index),
            Entry::Group { name, .. } => Choice::Group(name.clone()),
            Entry::Header(_) => unreachable!(),
        });
    }
    choice
}
