//! The servers page dialogs from `NodesPage.qml` and `BulkEditDialog.qml`:
//! new group (`groupDialog`), subscription properties (`infoDialog`), group
//! deletion (`deleteGroupDialog`) and moving servers (`BulkEditDialog`).

use crate::shell::wheel::GlideRoom;
use egui::{Align, Context, FontId, Layout, ScrollArea, Sense, TextEdit, Ui, pos2, vec2};
use lumen_core::i18n::tr;
use serde_json::{Value, json};

use super::{NodesPage, field, subscription};
use crate::shell::fluent_dialog::{self, DialogAction, FluentDialog, body_text};
use crate::shell::theme::{Theme, bold};
use crate::shell::widgets::{self, ButtonKind, FluentButton};
use crate::view_models::AppBridge;

/// Right-aligned button row (`Item { fillWidth } + buttons`, spacing 10).
fn buttons(ui: &mut Ui, theme: &Theme, add: impl FnOnce(&mut Ui)) {
    let width = ui.available_width();
    let (row, _) = ui.allocate_exact_size(vec2(width, theme.control_height as f32), Sense::hover());
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(row)
            .layout(Layout::right_to_left(Align::Center)),
    );
    child.spacing_mut().item_spacing.x = 10.0;
    add(&mut child);
}

fn small(ui: &mut Ui, theme: &Theme, text: &str, color: egui::Color32) {
    body_text(ui, text, FontId::proportional(theme.font_small as f32), color);
}

impl NodesPage {
    /// `groupDialog`: 420 px, padding 18, spacing 12; Enter creates.
    /// Returns true while it stays open.
    pub(super) fn group_dialog(&mut self, ctx: &Context, theme: &Theme, bridge: &mut AppBridge) -> bool {
        let mut create = false;
        let (cancel, close) = fluent_dialog::custom(ctx, theme, "nodes-group", 420.0, 18.0, 115, true, |ui, _| {
            body_text(ui, &tr("Новая группа", &[]), bold(theme.font_strong as f32), theme.text);
            ui.add_space(12.0);
            small(ui, theme, &tr("Группа будет создана отдельно от подписок. В неё можно переносить серверы через редактирование или массовое редактирование.", &[]), theme.text_faint);
            ui.add_space(12.0);
            small(ui, theme, &tr("Имя группы", &[]), theme.text_muted);
            ui.add_space(12.0);
            let field = fluent_dialog::text_field(
                ui,
                theme,
                "nodes-group-name",
                &mut self.input,
                &tr("Например: Мои серверы", &[]),
                theme.card,
            );
            if self.subscription_url_focus_pending {
                field.request_focus();
                self.subscription_url_focus_pending = false;
            }
            let valid = !self.input.trim().is_empty();
            let enter = field.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
            ui.add_space(12.0 + 6.0);
            let mut cancel = false;
            buttons(ui, theme, |ui| {
                create = FluentButton::new(ButtonKind::Accent, "", &tr("Создать", &[]))
                    .enabled(valid)
                    .show(ui, theme)
                    .clicked()
                    || (enter && valid);
                cancel = FluentButton::new(ButtonKind::Ghost, "", &tr("Отмена", &[])).show(ui, theme).clicked();
            });
            cancel
        });
        if create {
            let name = self.input.trim().to_string();
            if bridge.create_manual_group(&name) {
                self.choose_source(bridge, &name, "");
                return false;
            }
        }
        !(cancel || close)
    }

    /// `infoDialog`: 520 px, padding 18, spacing 12: the subscription
    /// picker, a scrolling block (90-420 px) of `infoRows()` and Close.
    pub(super) fn info_dialog(&mut self, ctx: &Context, theme: &Theme, bridge: &mut AppBridge, subs: &[Value]) -> bool {
        let (close_clicked, close) = fluent_dialog::custom(ctx, theme, "nodes-info", 520.0, 18.0, 115, true, |ui, width| {
            body_text(ui, &tr("Свойства подписки", &[]), bold(theme.font_strong as f32), theme.text);
            ui.add_space(12.0);
            let labels: Vec<String> = if subs.is_empty() {
                vec![tr("Нет подписок", &[])]
            } else {
                subs.iter()
                    .map(|sub| {
                        let name = field(sub, "name");
                        let name = if name.is_empty() { field(sub, "url") } else { name };
                        format!("{name} ({})", sub["node_count"].as_u64().unwrap_or(0))
                    })
                    .collect()
            };
            let current = subs.iter().position(|sub| field(sub, "id") == self.info_id).unwrap_or(0);
            if let Some(index) = widgets::combo(ui, theme, "nodes-info-sub", width, &labels, Some(current), !subs.is_empty())
                && let Some(sub) = subs.get(index)
            {
                self.info_id = field(sub, "id").to_string();
            }
            ui.add_space(12.0);
            let selected = subs.get(current);
            let rows = selected.map(subscription::info_rows).unwrap_or_default();
            ScrollArea::vertical()
                .id_salt("nodes-info-rows")
                .min_scrolled_height(90.0)
                .max_height(420.0)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    ui.set_width(width - 4.0);
                    for (index, (label, value)) in rows.iter().enumerate() {
                        if index > 0 {
                            ui.add_space(6.0);
                        }
                        info_row(ui, theme, bridge, label, value);
                    }
                }).glide_room(ui);
            let no_info = selected.is_some_and(|sub| sub["userinfo"].as_object().is_none_or(|info| info.is_empty()));
            if no_info {
                ui.add_space(12.0);
                small(ui, theme, &tr("В ссылке этой подписки нет доп. информации о пользователе.", &[]), theme.text_faint);
            }
            // RowLayout topMargin 6, rightMargin 4, bottomMargin 4.
            ui.add_space(12.0 + 6.0);
            let mut clicked = false;
            ui.horizontal(|ui| {
                let (row, _) = ui.allocate_exact_size(vec2(width - 4.0, theme.control_height as f32), Sense::hover());
                let mut child = ui.new_child(
                    egui::UiBuilder::new().max_rect(row).layout(Layout::right_to_left(Align::Center)),
                );
                clicked = FluentButton::new(ButtonKind::Accent, "", &tr("Закрыть", &[])).show(&mut child, theme).clicked();
            });
            ui.add_space(4.0);
            clicked
        });
        !(close_clicked || close)
    }

    /// `deleteGroupDialog`: FluentDialog, 440 px.
    pub(super) fn delete_group_dialog(&mut self, ctx: &Context, theme: &Theme, bridge: &mut AppBridge, group: &str) -> bool {
        let action = FluentDialog::new("nodes-delete-group", tr("Удалить группу", &[]), 440.0)
            .ok(tr("Удалить", &[]))
            .cancel(tr("Отмена", &[]))
            .show(ctx, theme, |ui| {
                body_text(
                    ui,
                    &tr("Группа «{name}», все её серверы и связанная подписка будут удалены.", &[("name", group)]),
                    FontId::proportional(theme.font_normal as f32),
                    theme.text_muted,
                );
            });
        if action == DialogAction::Accept && bridge.delete_group(group) {
            self.choose_source(bridge, "Default", "");
        }
        action == DialogAction::None
    }

    /// `BulkEditDialog` ("Переместить"): a 440 px popup without padding over
    /// a 40% backdrop that only Escape closes. The group combo lists the
    /// manual and subscription groups plus "Новая группа…", which shows the
    /// name field.
    pub(super) fn move_dialog(
        &mut self,
        ctx: &Context,
        theme: &Theme,
        bridge: &mut AppBridge,
        subs: &[Value],
        ids: &[String],
    ) -> bool {
        let mut groups = bridge.group_options();
        for sub in subs {
            let group = subscription::group(sub).to_string();
            if !groups.contains(&group) {
                groups.push(group);
            }
        }
        let mut choices = groups.clone();
        choices.push(tr("Новая группа…", &[]));
        let choice_id = egui::Id::unique("nodes-move-choice");
        let mut choice = ctx.data(|data| data.get_temp::<usize>(choice_id)).unwrap_or(0).min(choices.len() - 1);
        let mut apply = false;
        let (cancel, close) = fluent_dialog::custom(ctx, theme, "nodes-move", 440.0, 0.0, 102, false, |ui, width| {
            ui.add_space(20.0);
            let inner = width - 40.0;
            let mut column = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(egui::Rect::from_min_size(pos2(ui.cursor().left() + 20.0, ui.cursor().top()), vec2(inner, 10_000.0)))
                    .layout(Layout::top_down(Align::LEFT)),
            );
            column.spacing_mut().item_spacing = egui::Vec2::ZERO;
            body_text(&mut column, &tr("Переместить", &[]), bold(theme.font_title as f32), theme.text);
            column.add_space(10.0);
            body_text(
                &mut column,
                &tr("Серверов: {count}", &[("count", &ids.len().to_string())]),
                FontId::proportional(theme.font_normal as f32),
                theme.text_muted,
            );
            column.add_space(6.0 + 4.0);
            small(&mut column, theme, &tr("Группа", &[]), theme.text_muted);
            column.add_space(6.0);
            if let Some(picked) = widgets::combo(&mut column, theme, "nodes-move-group", inner, &choices, Some(choice), true) {
                choice = picked;
            }
            let creating = choice + 1 == choices.len();
            if creating {
                column.add_space(6.0 + 4.0);
                small(&mut column, theme, &tr("Имя группы", &[]), theme.text_muted);
                column.add_space(6.0);
                let field = fluent_dialog::text_field(
                    &mut column,
                    theme,
                    "nodes-move-name",
                    &mut self.input,
                    &tr("Например: Мои серверы", &[]),
                    theme.control_fill,
                );
                if field.lost_focus() && column.input(|input| input.key_pressed(egui::Key::Enter)) {
                    apply = true;
                }
            }
            let target = if creating { self.input.trim().to_string() } else { groups.get(choice).cloned().unwrap_or_default() };
            let used = column.min_rect().height();
            ui.add_space(used + 10.0 + 6.0);
            let (row, _) = ui.allocate_exact_size(vec2(width, theme.control_height as f32), Sense::hover());
            let mut bar = ui.new_child(
                egui::UiBuilder::new().max_rect(row.shrink2(vec2(20.0, 0.0))).layout(Layout::right_to_left(Align::Center)),
            );
            bar.spacing_mut().item_spacing.x = 10.0;
            apply |= FluentButton::new(ButtonKind::Accent, "", &tr("Переместить", &[]))
                .enabled(!target.is_empty())
                .show(&mut bar, theme)
                .clicked();
            let cancel = FluentButton::new(ButtonKind::Ghost, "", &tr("Отмена", &[])).show(&mut bar, theme).clicked();
            ui.add_space(20.0);
            if apply && !target.is_empty() {
                bridge.bulk_edit_nodes(ids, &json!({ "group": target }));
            } else {
                apply = false;
            }
            cancel
        });
        ctx.data_mut(|data| data.insert_temp(choice_id, choice));
        let open = !(cancel || close || apply);
        if !open {
            ctx.data_mut(|data| data.remove::<usize>(choice_id));
        }
        open
    }
}

/// One `infoRows()` row: a 140 px muted label, 12 px gap and the value
/// (small, wrapped anywhere); the URL row is selectable with copy and open
/// icon buttons.
fn info_row(ui: &mut Ui, theme: &Theme, bridge: &mut AppBridge, label: &str, value: &str) {
    let width = ui.available_width();
    let font = FontId::proportional(theme.font_small as f32);
    let label_galley = ui.painter().layout(label.to_owned(), font.clone(), theme.text_muted, 140.0);
    let is_url = label.eq_ignore_ascii_case("url");
    let value_left = 140.0 + 12.0;
    let buttons = if is_url { 2.0 * (theme.control_height as f32 + 12.0) } else { 0.0 };
    let value_width = (width - value_left - buttons).max(40.0);
    let mut job = egui::text::LayoutJob::simple(value.to_owned(), font.clone(), theme.text, value_width);
    job.wrap.break_anywhere = true;
    let value_galley = ui.painter().layout_job(job);
    let height = label_galley.size().y.max(value_galley.size().y).max(if is_url { theme.control_height as f32 } else { 0.0 });
    let (row, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
    ui.painter().galley(row.min, label_galley, theme.text_muted);
    let value_rect = egui::Rect::from_min_size(pos2(row.left() + value_left, row.top()), vec2(value_width, height));
    if is_url {
        // FluentTextEdit centres one line against the 32 px icon buttons.
        let line = ui.fonts_mut(|fonts| fonts.row_height(&font));
        let offset = ((theme.control_height as f32 - value_galley.size().y.max(line)) / 2.0).max(0.0);
        let mut child = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(value_rect.translate(vec2(0.0, offset)))
                .layout(Layout::top_down(Align::LEFT)),
        );
        let mut text = value;
        child.add(
            TextEdit::multiline(&mut text)
                .font(font)
                .frame(egui::Frame::NONE)
                .desired_width(value_width)
                .desired_rows(1),
        );
        let mut bar = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(egui::Rect::from_min_max(pos2(value_rect.right() + 12.0, row.top()), row.right_bottom()))
                .layout(Layout::left_to_right(Align::Min)),
        );
        bar.spacing_mut().item_spacing.x = 12.0;
        if FluentButton::icon("\u{e8c8}", &tr("Копировать ссылку", &[])).show(&mut bar, theme).clicked()
            && let Some(text) = bridge.copy_text(value)
        {
            bar.ctx().copy_text(text);
        }
        if FluentButton::icon("\u{e8a7}", &tr("Открыть", &[])).show(&mut bar, theme).clicked() {
            crate::shell::win_native::open_url(value);
        }
    } else {
        ui.painter().galley(value_rect.min, value_galley, theme.text);
    }
}
