//! Server table, inspector, bulk actions and subscription sources.

mod subscription;

use std::collections::HashSet;

use egui::{
    Button, ComboBox, Frame, Id, Key, Modal, Modifiers, RichText, ScrollArea, Sense, Stroke,
    TextEdit, Ui,
};
use egui_extras::{Column, TableBuilder};
use lumen_core::i18n::tr;
use serde_json::{Value, json};

use super::{PageContext, flag, node_editor::NodeEditor, search_field};
use crate::shell::{
    onboarding::{import_clipboard, import_config_file},
    theme::{Theme, semibold},
    win_native,
};
use crate::view_models::{AppBridge, app_bridge::UiEvent, node_list_model::NodeListModel};

const COLUMNS: [(&str, &str, &str, f32, f32); 10] = [
    ("name", "name", "Сервер", 200.0, 140.0),
    ("type", "scheme", "Тип", 80.0, 56.0),
    ("transport", "transport", "Транспорт", 92.0, 60.0),
    ("address", "address", "Адрес", 160.0, 110.0),
    ("port", "", "Порт", 60.0, 44.0),
    ("group", "group", "Группа", 120.0, 70.0),
    ("ping", "ping", "Пинг", 76.0, 56.0),
    ("speed", "speed", "Скорость", 100.0, 76.0),
    ("status", "status", "Статус", 64.0, 50.0),
    ("last", "last", "Последнее", 130.0, 70.0),
];

const INSPECTOR_BREAKPOINT: f32 = 1100.0;
const INSPECTOR_WIDTH: f32 = 300.0;
const TABLE_LEFT_PAD: f32 = 12.0;
const TABLE_RIGHT_PAD: f32 = 26.0;
const TABLE_CARD_MARGIN: f32 = 20.0;

#[derive(Clone)]
enum Dialog {
    Group,
    Subscription,
    Info,
    DeleteGroup(String),
    Move(Vec<String>),
}

#[derive(Default)]
pub(super) struct NodesPage {
    editor: NodeEditor,
    query: String,
    picked: HashSet<String>,
    anchor: Option<String>,
    drag_start: Option<usize>,
    dialog: Option<Dialog>,
    input: String,
    second: String,
    info_id: String,
    inspector_node: bool,
    widths: Option<Vec<f32>>,
    resized_column: Option<usize>,
    resize_origin: Option<(usize, f32, f32)>,
    scroll_row: Option<usize>,
    imported_node: Option<String>,
    imported_group: Option<String>,
    file_error: Option<String>,
}

impl NodesPage {
    pub(super) fn on_event(&mut self, event: &UiEvent) {
        match event {
            UiEvent::NodeImported(id) => self.imported_node = Some(id.clone()),
            UiEvent::SubscriptionImported(group) => self.imported_group = Some(group.clone()),
            _ => {}
        }
    }

    pub(super) fn render(&mut self, ui: &mut Ui, page: &mut PageContext<'_>) {
        let theme = page.theme;
        let bridge = &mut *page.bridge;
        let subscriptions = bridge.subscriptions();
        if let Some(group) = self.imported_group.take() {
            let id = subscriptions
                .iter()
                .find(|sub| subscription::group(sub) == group)
                .map_or("", |sub| field(sub, "id"));
            self.choose_source(bridge, &group, id);
        }
        if let Some(id) = self.imported_node.take() {
            self.reveal(bridge, &id);
        }
        let selected_sub = subscriptions
            .iter()
            .find(|sub| field(sub, "id") == bridge.selected_subscription_id());
        let show_inspector = ui.available_width() >= INSPECTOR_BREAKPOINT;
        let spacing = theme.spacing as f32;
        Frame::new().inner_margin(spacing).show(ui, |ui| {
            ui.spacing_mut().item_spacing = egui::Vec2::splat(spacing);
            self.toolbar(ui, theme, bridge, &subscriptions, selected_sub);
            if let Some(error) = &self.file_error {
                ui.colored_label(theme.danger, error);
            }
            self.shortcuts(ui, bridge);
            let height = ui.available_height();
            if show_inspector {
                let left = ui.available_width() - INSPECTOR_WIDTH - spacing;
                ui.horizontal_top(|ui| {
                    ui.allocate_ui_with_layout(
                        egui::vec2(left, height),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            ui.set_width(left);
                            self.table(ui, theme, bridge, height);
                        },
                    );
                    ui.allocate_ui_with_layout(
                        egui::vec2(INSPECTOR_WIDTH, height),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            ui.set_width(INSPECTOR_WIDTH);
                            self.inspector(ui, theme, bridge, selected_sub, height);
                        },
                    );
                });
            } else {
                self.table(ui, theme, bridge, height);
            }
        });
        if let Some(id) = self.editor.render(page.ctx, theme, bridge) {
            self.reveal(bridge, &id);
        }
        if let Some(dialog) = self.dialog.clone() {
            self.render_dialog(page.ctx, theme, bridge, &subscriptions, dialog);
        }
        if bridge.subscription_importing() {
            Modal::new(Id::unique("node-sub-import-progress")).show(page.ctx, |ui| {
                ui.set_width(360.0_f32.min((page.ctx.content_rect().width() - 48.0).max(0.0)));
                ui.heading(tr("Импорт подписки", &[]));
                ui.spinner();
                ui.label(bridge.subscription_import_status());
                if button(ui, "Отмена") {
                    bridge.cancel_subscription_import();
                }
            });
        }
    }

    fn reveal(&mut self, bridge: &mut AppBridge, id: &str) {
        self.query.clear();
        let group = field(&bridge.node_details(id), "group").to_string();
        let source = bridge
            .subscriptions()
            .into_iter()
            .find(|sub| subscription::group(sub) == group);
        self.choose_source(
            bridge,
            &group,
            source.as_ref().map_or("", |sub| field(sub, "id")),
        );
        let index = bridge.node_model.index_of_id(id);
        self.scroll_row = usize::try_from(index).ok();
        self.picked.insert(id.into());
        self.anchor = Some(id.into());
        self.inspector_node = true;
    }

    fn choose_source(&mut self, bridge: &mut AppBridge, group: &str, id: &str) {
        self.picked.clear();
        self.anchor = None;
        self.inspector_node = id.is_empty();
        bridge.set_selected_subscription_id(id);
        bridge.set_node_filter(group, &self.query);
    }

    fn open(&mut self, dialog: Dialog) {
        self.input.clear();
        self.second.clear();
        self.dialog = Some(dialog);
    }

    fn toolbar(
        &mut self,
        ui: &mut Ui,
        theme: &Theme,
        bridge: &mut AppBridge,
        subscriptions: &[Value],
        selected_sub: Option<&Value>,
    ) {
        let group = bridge.node_filter_group();
        let available = ui.available_width();
        let source_width = (available * 0.24)
            .clamp(240.0, 340.0)
            .min((available - 238.0).max(100.0));
        toolbar_row(ui, 10.0, |ui| {
            menu(
                ui,
                Button::new(
                    RichText::new(format!("\u{e710}  {}", tr("Добавить", &[])))
                        .color(theme.accent_text),
                )
                .fill(theme.accent),
                112.0,
                |ui| {
                    if button(ui, "Импорт подписки") {
                        self.open(Dialog::Subscription);
                        ui.close();
                    }
                    if button(ui, "Импорт из буфера") {
                        import_clipboard(bridge);
                        ui.close();
                    }
                    if button(ui, "Импорт файла конфигурации") {
                        self.file_error = import_config_file(bridge)
                            .err()
                            .map(|error| error.to_string());
                        ui.close();
                    }
                    if button(ui, "Добавить сервер вручную") {
                        self.editor.create(bridge, &group);
                        ui.close();
                    }
                    if button(ui, "Создать группу") {
                        self.open(Dialog::Group);
                        ui.close();
                    }
                },
            );
            let name = if let Some(sub) = selected_sub {
                subscription::name(sub).to_string()
            } else if group.is_empty() {
                tr("Все серверы", &[])
            } else {
                group.clone()
            };
            let mut label = egui::text::LayoutJob::default();
            label.append(
                &name,
                0.0,
                egui::TextFormat {
                    font_id: semibold(theme.font_normal as f32),
                    color: theme.text,
                    ..Default::default()
                },
            );
            if let Some(status) = selected_sub.map(subscription::Status::new)
                && let Some(summary) = status.summary()
            {
                let (usage, time) = summary.split_once(" · ").unwrap_or((&summary, ""));
                label.append(
                    usage,
                    10.0,
                    egui::TextFormat {
                        font_id: egui::FontId::proportional(11.0),
                        color: status.color(theme),
                        ..Default::default()
                    },
                );
                if !time.is_empty() {
                    label.append(
                        &format!(" · {time}"),
                        0.0,
                        egui::TextFormat {
                            font_id: egui::FontId::proportional(11.0),
                            color: theme.text_muted,
                            ..Default::default()
                        },
                    );
                }
            }
            let picker = menu(ui, Button::new(label).truncate(), source_width, |ui| {
                if button(ui, "Все серверы") {
                    self.choose_source(bridge, "", "");
                    ui.close();
                }
                if !subscriptions.is_empty() {
                    ui.separator();
                    ui.label(tr("Подписки", &[]));
                }
                for sub in subscriptions {
                    let state = subscription::Status::new(sub);
                    let label = format!(
                        "{} · {}",
                        subscription::name(sub),
                        field_value(&sub["node_count"])
                    );
                    if ui
                        .selectable_label(
                            selected_sub
                                .is_some_and(|active| field(active, "id") == field(sub, "id")),
                            label,
                        )
                        .clicked()
                    {
                        self.choose_source(bridge, subscription::group(sub), field(sub, "id"));
                        ui.close();
                    }
                    if let Some(summary) = state.summary() {
                        ui.label(
                            RichText::new(summary)
                                .size(theme.font_small as f32)
                                .color(state.color(theme)),
                        );
                    }
                }
                ui.separator();
                ui.label(tr("Группы", &[]));
                for manual in bridge.group_options() {
                    if subscriptions
                        .iter()
                        .any(|sub| subscription::group(sub) == manual)
                    {
                        continue;
                    }
                    ui.horizontal(|ui| {
                        if ui.selectable_label(group == manual, &manual).clicked() {
                            self.choose_source(bridge, &manual, "");
                            ui.close();
                        }
                        if !manual.eq_ignore_ascii_case("default") && button(ui, "Удалить") {
                            self.open(Dialog::DeleteGroup(manual.clone()));
                            ui.close();
                        }
                    });
                }
                if button(ui, "Импорт подписки") {
                    self.open(Dialog::Subscription);
                    ui.close();
                }
                if button(ui, "Новая группа") {
                    self.open(Dialog::Group);
                    ui.close();
                }
            });
            ui.painter().text(
                egui::pos2(picker.rect.right() - 12.0, picker.rect.center().y),
                egui::Align2::CENTER_CENTER,
                "\u{e70d}",
                egui::FontId::proportional(9.0),
                theme.text_muted,
            );
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.label(RichText::new(tr("Серверы", &[])).font(semibold(theme.font_title as f32)));
                if !self.picked.is_empty() && ui.available_width() > 76.0 {
                    ui.label(
                        RichText::new(tr(
                            "Выбрано: {count}",
                            &[("count", &self.picked.len().to_string())],
                        ))
                        .size(theme.font_small as f32)
                        .color(theme.text_muted),
                    );
                }
            });
        });
        let sort_width = (available - 345.0).clamp(100.0, 190.0);
        toolbar_row(ui, 8.0, |ui| {
            menu(ui, Button::new("\u{e712}"), 32.0, |ui| {
                if button(ui, "Выбрать все") {
                    self.select_all(bridge);
                    ui.close();
                }
                if button(ui, "Тест скорости всех в списке") {
                    let targets = visible_ids(bridge).into_iter().collect::<HashSet<_>>();
                    if !targets.is_empty() {
                        bridge.speed_test_nodes(Some(&targets));
                    }
                    ui.close();
                }
                if ui
                    .add_enabled(
                        bridge.tests_running(),
                        Button::new(tr("Остановить тест", &[])),
                    )
                    .clicked()
                {
                    bridge.cancel_tests();
                    ui.close();
                }
                if button(ui, "Экспорт всех (ссылки)") {
                    copy(ui, bridge.export_node_links(&[]));
                    ui.close();
                }
                if let Some(sub) = selected_sub {
                    if button(ui, "Свойства подписки") {
                        self.open_info(sub);
                        ui.close();
                    }
                    if button(ui, "Удалить подписку (с серверами)") {
                        bridge.remove_subscription(field(sub, "url"), true);
                        ui.close();
                    }
                }
                if ui
                    .add_enabled(
                        !subscriptions.is_empty(),
                        Button::new(tr("Обновить все подписки", &[])),
                    )
                    .clicked()
                {
                    bridge.update_all_subscriptions();
                    ui.close();
                }
            });
            if ui
                .add_sized(
                    [81.0, 32.0],
                    Button::new(format!("\u{e724}  {}", tr("Пинг", &[]))),
                )
                .clicked()
            {
                let targets = self.targets(bridge);
                if !targets.is_empty() {
                    bridge.ping_nodes(Some(&targets));
                }
            }
            let key = bridge.node_sort_key();
            let ascending = bridge.node_sort_ascending();
            if ui
                .add_sized(
                    [32.0, 32.0],
                    Button::new(if ascending { "\u{e74a}" } else { "\u{e74b}" }),
                )
                .on_hover_text(tr(
                    if ascending {
                        "По возрастанию"
                    } else {
                        "По убыванию"
                    },
                    &[],
                ))
                .clicked()
            {
                bridge.set_node_sort(&key, !ascending);
            }
            let mut next = None;
            let sorts = std::iter::once(("manual", "Вручную")).chain(
                COLUMNS
                    .iter()
                    .filter(|column| !column.1.is_empty())
                    .map(|column| {
                        (
                            column.1,
                            if column.1 == "name" {
                                "Имя"
                            } else {
                                column.2
                            },
                        )
                    }),
            );
            let options = sorts.collect::<Vec<_>>();
            ComboBox::from_id_salt("node-sort-order").icon(crate::shell::widgets::combo_chevron)
                .width(sort_width)
                .height(350.0)
                .selected_text(tr(
                    options
                        .iter()
                        .find(|option| option.0 == key)
                        .map_or("Вручную", |option| option.1),
                    &[],
                ))
                .show_ui(ui, |ui| {
                    for (value, label) in options {
                        if ui.selectable_label(key == value, tr(label, &[])).clicked() {
                            next = Some(value);
                        }
                    }
                });
            if let Some(key) = next {
                bridge.set_node_sort(key, true);
            }
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                let (response, _) = search_field::show(
                    ui,
                    theme,
                    &mut self.query,
                    &tr("Поиск серверов…", &[]),
                    false,
                );
                if response.changed() {
                    bridge.set_node_filter(&group, &self.query);
                    self.picked.clear();
                }
            });
        });
    }

    fn shortcuts(&mut self, ui: &mut Ui, bridge: &mut AppBridge) {
        if ui.ctx().egui_wants_keyboard_input()
            || egui::Popup::is_any_open(ui.ctx())
            || self.editor.is_open()
            || self.dialog.is_some()
            || bridge.subscription_importing()
        {
            return;
        }
        if ui.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::A)) {
            self.select_all(bridge);
        }
        if ui.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::V)) {
            import_clipboard(bridge);
        }
        if ui.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::C))
            && self.picked.len() == 1
            && let Some(id) = self.picked.iter().next()
        {
            copy(ui, bridge.copy_node_link(id));
        }
        if ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Delete)) {
            self.delete(bridge);
        }
        if ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
            self.picked.clear();
            self.anchor = None;
        }
    }

    fn select_all(&mut self, bridge: &AppBridge) {
        self.picked = visible_ids(bridge).into_iter().collect();
    }

    fn targets(&self, bridge: &AppBridge) -> HashSet<String> {
        if self.picked.is_empty() {
            visible_ids(bridge).into_iter().collect()
        } else {
            self.picked.clone()
        }
    }

    fn delete(&mut self, bridge: &mut AppBridge) {
        bridge.delete_nodes(&self.picked.drain().collect::<Vec<_>>());
        self.anchor = None;
    }

    fn table(&mut self, ui: &mut Ui, theme: &Theme, bridge: &mut AppBridge, height: f32) {
        let row_height = if bridge.compact_mode() { 30.0 } else { 34.0 };
        let ids = visible_ids(bridge);
        let sort = bridge.node_sort_key();
        let ascending = bridge.node_sort_ascending();
        let released = ui.input(|i| i.pointer.primary_released());
        if self.widths.is_none() {
            let layout = bridge.node_table_layout();
            if layout["manual"].as_bool() == Some(true) {
                self.widths = Some(
                    COLUMNS
                        .iter()
                        .map(|column| layout[column.0].as_f64().unwrap_or(column.3 as f64) as f32)
                        .collect(),
                );
            }
        }
        let width = (ui.available_width() - 2.0 * TABLE_CARD_MARGIN).max(0.0);
        let preferred = fit_widths(width, self.widths.as_deref(), self.resized_column);
        let table_width = preferred.iter().sum::<f32>() + TABLE_LEFT_PAD + TABLE_RIGHT_PAD;
        let scroll_height = (height
            - 2.0 * TABLE_CARD_MARGIN
            - 30.0
            - if table_width > width + 1.0 { 12.0 } else { 0.0 })
        .max(0.0);
        let mut sort_clicked = None;
        let mut resized = None;
        Frame::new()
            .fill(theme.card)
            .stroke(Stroke::new(1.0, theme.border_solid))
            .corner_radius(theme.radius as u8)
            .inner_margin(TABLE_CARD_MARGIN - 1.0)
            .show(ui, |ui| {
                ui.set_min_height((height - 2.0 * TABLE_CARD_MARGIN).max(0.0));
                ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
                ui.visuals_mut().selection.bg_fill = theme.accent_soft;
                ScrollArea::horizontal()
                    .id_salt("nodes-columns")
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        ui.set_width(table_width.max(width));
                        let mut table = TableBuilder::new(ui)
                            .id_salt("nodes-table")
                            .striped(false)
                            .sense(Sense::click_and_drag())
                            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                            .min_scrolled_height(scroll_height)
                            .max_scroll_height(scroll_height)
                            .auto_shrink([false, false]);
                        for (index, width) in preferred.iter().enumerate() {
                            table = table.column(Column::exact(
                                *width + if index == 0 { TABLE_LEFT_PAD } else { 0.0 },
                            ));
                        }
                        if let Some(index) = self.scroll_row.take() {
                            table = table.scroll_to_row(index, Some(egui::Align::Center));
                        }
                        let mut header_cells = [egui::Rect::NOTHING; COLUMNS.len()];
                        table
                            .header(30.0, |mut row| {
                                for (index, (_, key, label, _, _)) in
                                    COLUMNS.into_iter().enumerate()
                                {
                                    row.col(|ui| {
                                        header_cells[index] = ui.max_rect();
                                        row_rule(ui, theme);
                                        ui.add_space(if index == 0 {
                                            TABLE_LEFT_PAD + 4.0
                                        } else {
                                            4.0
                                        });
                                        let caption = tr(label, &[]);
                                        if key.is_empty() {
                                            ui.label(
                                                RichText::new(caption)
                                                    .font(semibold(12.0))
                                                    .color(theme.text_faint),
                                            );
                                        } else {
                                            let arrow = if key == sort {
                                                if ascending { " ▲" } else { " ▼" }
                                            } else {
                                                ""
                                            };
                                            if ui
                                                .add(
                                                    Button::new(
                                                        RichText::new(format!("{caption}{arrow}"))
                                                            .font(semibold(12.0))
                                                            .color(theme.text_faint),
                                                    )
                                                    .frame(false)
                                                    .truncate(),
                                                )
                                                .clicked()
                                            {
                                                sort_clicked = Some(key);
                                            }
                                        }
                                    });
                                }
                            })
                            .body(|body| {
                                body.rows(row_height, ids.len(), |mut row| {
                                    let index = row.index();
                                    let id = &ids[index];
                                    row.set_selected(self.picked.contains(id));
                                    for column in 0..COLUMNS.len() {
                                        row.col(|ui| {
                                            row_rule(ui, theme);
                                            if column == 0 && bridge.node_model.selected_at(index) {
                                                let rect = ui.max_rect();
                                                ui.painter().rect_filled(
                                                    egui::Rect::from_min_size(
                                                        rect.min,
                                                        egui::vec2(3.0, rect.height()),
                                                    ),
                                                    0.0,
                                                    theme.accent,
                                                );
                                            }
                                            ui.add_space(if column == 0 {
                                                TABLE_LEFT_PAD + 4.0
                                            } else {
                                                4.0
                                            });
                                            render_cell(
                                                ui,
                                                theme,
                                                &bridge.node_model,
                                                index,
                                                column,
                                            )
                                        });
                                    }
                                    let response = row.response();
                                    if response.double_clicked() {
                                        bridge.select_node(id);
                                    }
                                    if response.clicked() {
                                        let modifiers = response.ctx.input(|i| i.modifiers);
                                        self.pick_row(&ids, index, modifiers);
                                    }
                                    if response.drag_started() {
                                        self.drag_start = Some(index);
                                        self.inspector_node = true;
                                    }
                                    if let Some(start) = self.drag_start
                                        && response.contains_pointer()
                                    {
                                        self.picked =
                                            range(&ids, start, index).iter().cloned().collect();
                                    }
                                    response.context_menu(|ui| {
                                        if !self.picked.contains(id) {
                                            self.pick_row(&ids, index, Modifiers::NONE);
                                        }
                                        self.context_menu(ui, bridge, id);
                                    });
                                });
                            });
                        // Register handles after the table so its cells cannot claim the drag.
                        for (index, rect) in header_cells.into_iter().enumerate() {
                            let grip = egui::Rect::from_min_max(
                                egui::pos2(rect.right() - 5.0, rect.top()),
                                egui::pos2(rect.right() + 4.0, rect.bottom()),
                            );
                            let handle = ui
                                .interact(
                                    grip,
                                    ui.id().with(("column-width", index)),
                                    Sense::drag(),
                                )
                                .on_hover_cursor(egui::CursorIcon::ResizeColumn);
                            if handle.drag_started_by(egui::PointerButton::Primary)
                                && let Some(origin) = ui.input(|input| input.pointer.press_origin())
                            {
                                self.resize_origin = Some((index, preferred[index], origin.x));
                            }
                            if (handle.dragged() || handle.drag_stopped())
                                && let Some((column, width, start_x)) = self.resize_origin
                                && column == index
                                && let Some(pointer) = handle.interact_pointer_pos()
                            {
                                let width =
                                    (width + pointer.x - start_x).clamp(COLUMNS[index].4, 2000.0);
                                resized = Some((index, width));
                            }
                            if handle.hovered() || handle.dragged() {
                                ui.painter().vline(
                                    rect.right(),
                                    rect.y_range(),
                                    Stroke::new(1.0, theme.accent),
                                );
                            }
                        }
                    });
            });
        if released {
            self.drag_start = None;
        }
        if let Some(key) = sort_clicked {
            if key == sort && !ascending {
                bridge.set_node_sort("manual", true);
            } else {
                bridge.set_node_sort(key, key != sort);
            }
        }
        if let Some((index, width)) = resized {
            let widths = self.widths.get_or_insert(preferred);
            widths[index] = width;
            self.resized_column = Some(index);
        }
        if released
            && self.resize_origin.take().is_some()
            && let Some(widths) = &self.widths
        {
            let mut layout = serde_json::Map::from_iter([("manual".into(), json!(true))]);
            for (column, value) in COLUMNS.iter().zip(widths) {
                layout.insert(column.0.into(), json!(value.round()));
            }
            bridge.set_node_table_layout(&Value::Object(layout));
        }
    }

    fn pick_row(&mut self, ids: &[String], index: usize, modifiers: Modifiers) {
        let id = &ids[index];
        self.inspector_node = true;
        if modifiers.shift {
            let anchor = self
                .anchor
                .as_ref()
                .and_then(|id| ids.iter().position(|row| row == id))
                .unwrap_or(index);
            self.picked = range(ids, anchor, index).iter().cloned().collect();
        } else {
            if modifiers.ctrl {
                if !self.picked.remove(id) {
                    self.picked.insert(id.clone());
                }
            } else {
                self.picked = HashSet::from([id.clone()]);
            }
            self.anchor = Some(id.clone());
        }
    }

    fn context_menu(&mut self, ui: &mut Ui, bridge: &mut AppBridge, id: &str) {
        let single = self.picked.len() == 1;
        if ui
            .add_enabled(single, Button::new(tr("Установить как активный", &[])))
            .clicked()
        {
            bridge.select_node(id);
            ui.close();
        }
        ui.menu_button(tr("Пинг", &[]), |ui| {
            for (label, action) in [
                ("Способ из настроек", 0),
                ("Тест TCP ping", 1),
                ("Тест HTTP GET", 2),
                ("Тест ICMP", 3),
            ] {
                if button(ui, label) {
                    match action {
                        0 => bridge.ping_nodes(Some(&self.picked)),
                        1 => bridge.tcping_nodes(Some(&self.picked)),
                        2 => bridge.http_get_nodes(Some(&self.picked)),
                        _ => bridge.icmp_nodes(Some(&self.picked)),
                    }
                    ui.close();
                }
            }
        });
        if button(ui, "Тест скорости загрузки") {
            bridge.speed_test_nodes(Some(&self.picked));
            ui.close();
        }
        if ui
            .add_enabled(
                bridge.tests_running(),
                Button::new(tr("Остановить тест", &[])),
            )
            .clicked()
        {
            bridge.cancel_tests();
            ui.close();
        }
        ui.separator();
        if ui
            .add_enabled(single, Button::new(tr("Редактировать", &[])))
            .clicked()
        {
            self.editor.edit(bridge, id);
            ui.close();
        }
        if button(ui, "Переместить") {
            self.open(Dialog::Move(self.picked.iter().cloned().collect()));
            ui.close();
        }
        if ui
            .add_enabled(single, Button::new(tr("Копировать ссылку", &[])))
            .clicked()
        {
            copy(ui, bridge.copy_node_link(id));
            ui.close();
        }
        if ui
            .button(tr(
                "Экспорт выбранных — ссылки ({count})",
                &[("count", &self.picked.len().to_string())],
            ))
            .clicked()
        {
            copy(
                ui,
                bridge.export_node_links(&self.picked.iter().cloned().collect::<Vec<_>>()),
            );
            ui.close();
        }
        if ui
            .add_enabled(single, Button::new(tr("Экспорт в QR-код", &[])))
            .clicked()
        {
            bridge.show_node_qr(id);
            ui.close();
        }
        for (label, runtime) in [
            ("Экспорт outbound JSON", false),
            ("Экспорт runtime JSON", true),
        ] {
            if ui
                .add_enabled(single, Button::new(tr(label, &[])))
                .clicked()
            {
                export_json(ui, bridge, id, runtime);
                ui.close();
            }
        }
        for (label, direction) in [("В начало списка", "top"), ("В конец списка", "bottom")]
        {
            if ui
                .add_enabled(single, Button::new(tr(label, &[])))
                .clicked()
            {
                bridge.reorder_node(id, direction);
                ui.close();
            }
        }
        if button(ui, "Выбрать все") {
            self.select_all(bridge);
            ui.close();
        }
        if ui
            .button(tr(
                "Удалить ({count})",
                &[("count", &self.picked.len().to_string())],
            ))
            .clicked()
        {
            self.delete(bridge);
            ui.close();
        }
    }

    fn inspector(
        &mut self,
        ui: &mut Ui,
        theme: &Theme,
        bridge: &mut AppBridge,
        sub: Option<&Value>,
        height: f32,
    ) {
        let selected_sub = sub.filter(|_| !self.inspector_node);
        let id = self
            .anchor
            .as_ref()
            .filter(|id| self.picked.contains(*id))
            .or_else(|| self.picked.iter().next())
            .map(String::as_str)
            .unwrap_or_else(|| bridge.selected_node_id())
            .to_owned();
        let details = bridge.node_details(&id);
        let has_node = details.get("id").is_some();
        Frame::new()
            .fill(theme.card)
            .stroke(Stroke::new(1.0, theme.border_solid))
            .corner_radius(theme.radius as u8)
            .show(ui, |ui| {
                ui.set_min_size(egui::vec2(INSPECTOR_WIDTH - 2.0, (height - 2.0).max(0.0)));
                ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
                if sub.is_some() {
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.add_space(10.0);
                        ui.spacing_mut().item_spacing.x = 4.0;
                        for (node, label) in [(false, "Подписка"), (true, "Сервер")] {
                            if inspector_tab(ui, theme, &tr(label, &[]), node == self.inspector_node) {
                                self.inspector_node = node;
                            }
                        }
                    });
                    inspector_rule(ui, theme);
                }
                let footer_height = if selected_sub.is_some() {
                    55.0
                } else if has_node {
                    93.0
                } else {
                    0.0
                };
                let body_height =
                    (height - footer_height - if sub.is_some() { 41.0 } else { 2.0 }).max(0.0);
                ScrollArea::vertical()
                    .id_salt("nodes-inspector")
                    .max_width(INSPECTOR_WIDTH - 2.0)
                    .max_height(body_height)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        Frame::new().inner_margin(14).show(ui, |ui| {
                            ui.spacing_mut().item_spacing.y = 12.0;
                            if let Some(sub) = selected_sub {
                                subscription::inspector(ui, theme, sub);
                            } else {
                                server_inspector(ui, theme, &details, has_node);
                            }
                        });
                    });
                if footer_height > 0.0 {
                    inspector_rule(ui, theme);
                    Frame::new().inner_margin(11).show(ui, |ui| {
                        ui.spacing_mut().item_spacing = egui::vec2(6.0, 6.0);
                        ui.spacing_mut().button_padding.x = 7.0;
                        ui.spacing_mut().button_padding.y = 0.0;
                        if let Some(sub) = selected_sub {
                            let url = field(sub, "url");
                            let can_copy = sub["presentation"]["hideUrl"].as_bool() != Some(true);
                            ui.horizontal(|ui| {
                                let update_width = (ui.available_width()
                                    - if can_copy { 114.0 } else { 76.0 })
                                .max(32.0);
                                if ui
                                    .add_sized(
                                        [update_width, 32.0],
                                        Button::new(format!("  {}", tr("Обновить", &[]))),
                                    )
                                    .clicked()
                                {
                                    bridge.update_subscription(url);
                                }
                                if icon_button(ui, "", "Свойства подписки") {
                                    self.open_info(sub);
                                }
                                if can_copy && icon_button(ui, "", "Копировать ссылку")
                                {
                                    copy(ui, bridge.copy_text(url));
                                }
                                if ui
                                    .add_sized([32.0, 32.0], Button::new(RichText::new("").color(theme.accent_text)).fill(theme.danger_fill))
                                    .on_hover_text(tr("Удалить подписку (с серверами)", &[]))
                                    .clicked()
                                {
                                    bridge.remove_subscription(url, true);
                                }
                            });
                        } else if has_node {
                            let active = Button::new(
                                RichText::new(format!("  {}", tr("Установить как активный", &[])))
                                    .color(theme.accent_text),
                            )
                            .fill(theme.accent);
                            if ui.add_sized([ui.available_width(), 32.0], active).clicked() {
                                bridge.select_node(&id);
                            }
                            ui.horizontal(|ui| {
                                let edit_width = (ui.available_width() - 114.0).max(32.0);
                                if ui
                                    .add_sized(
                                        [edit_width, 32.0],
                                        Button::new(format!("  {}", tr("Редактировать", &[]))),
                                    )
                                    .clicked()
                                {
                                    self.editor.edit(bridge, &id);
                                }
                                if icon_button(ui, "", "Пинг") {
                                    bridge.ping_nodes(Some(&HashSet::from([id.clone()])));
                                }
                                if icon_button(ui, "", "Копировать ссылку") {
                                    copy(ui, bridge.copy_node_link(&id));
                                }
                                if icon_button(ui, "", "Экспорт в QR-код") {
                                    bridge.show_node_qr(&id);
                                }
                            });
                        }
                    });
                }
            });
    }

    fn open_info(&mut self, sub: &Value) {
        self.info_id = field(sub, "id").into();
        self.open(Dialog::Info);
    }

    fn render_dialog(
        &mut self,
        ctx: &egui::Context,
        theme: &Theme,
        bridge: &mut AppBridge,
        subs: &[Value],
        dialog: Dialog,
    ) {
        let modal = Modal::new(Id::unique("nodes-dialog")).show(ctx, |ui| {
            ui.set_width(520.0_f32.min((ctx.content_rect().width() - 48.0).max(0.0)));
            let title = match &dialog {
                Dialog::Group => "Новая группа",
                Dialog::Subscription => "Импорт подписки",
                Dialog::Info => "Свойства подписки",
                Dialog::DeleteGroup(_) => "Удалить группу",
                Dialog::Move(_) => "Переместить",
            };
            ui.label(
                RichText::new(tr(title, &[]))
                    .size(theme.font_title as f32)
                    .family(crate::shell::theme::semibold_family()),
            );
            match &dialog {
                Dialog::Group => edit(ui, &mut self.input, "Имя группы"),
                Dialog::Subscription => {
                    edit(ui, &mut self.input, "Ссылка на подписку");
                    edit(ui, &mut self.second, "Название (необязательно)");
                }
                Dialog::Info => {
                    let selected = subs
                        .iter()
                        .find(|sub| field(sub, "id") == self.info_id)
                        .or_else(|| subs.first());
                    ComboBox::from_id_salt("nodes-subscription-info").icon(crate::shell::widgets::combo_chevron)
                        .selected_text(selected.map(subscription::name).unwrap_or_default())
                        .show_ui(ui, |ui| {
                            for sub in subs {
                                ui.selectable_value(
                                    &mut self.info_id,
                                    field(sub, "id").to_string(),
                                    subscription::name(sub),
                                );
                            }
                        });
                    if let Some(sub) = selected {
                        ScrollArea::vertical()
                            .max_height((ctx.content_rect().height() - 160.0).max(110.0))
                            .show(ui, |ui| subscription::details(ui, theme, bridge, sub));
                    }
                }
                Dialog::DeleteGroup(group) => {
                    ui.label(tr(
                        "Удалить группу «{name}» и все её серверы?",
                        &[("name", group)],
                    ));
                }
                Dialog::Move(ids) => {
                    ui.label(tr("Выбрано: {count}", &[("count", &ids.len().to_string())]));
                    ComboBox::from_id_salt("nodes-move-group").icon(crate::shell::widgets::combo_chevron)
                        .selected_text(if self.second.is_empty() {
                            tr("Выберите группу", &[])
                        } else {
                            self.second.clone()
                        })
                        .show_ui(ui, |ui| {
                            for group in bridge.group_options() {
                                ui.selectable_value(&mut self.second, group.clone(), group);
                            }
                            for sub in subs {
                                let group = subscription::group(sub);
                                ui.selectable_value(&mut self.second, group.to_string(), group);
                            }
                        });
                    edit(ui, &mut self.input, "Новая группа");
                }
            }
            let valid = match dialog {
                Dialog::Info | Dialog::DeleteGroup(_) => true,
                Dialog::Move(_) => !self.input.trim().is_empty() || !self.second.trim().is_empty(),
                _ => !self.input.trim().is_empty(),
            };
            ui.horizontal_wrapped(|ui| {
                let cancel = !matches!(dialog, Dialog::Info) && button(ui, "Отмена");
                let label = match dialog {
                    Dialog::Info => "Закрыть",
                    Dialog::DeleteGroup(_) => "Удалить",
                    Dialog::Move(_) => "Переместить",
                    Dialog::Subscription => "Импорт",
                    Dialog::Group => "Создать",
                };
                let apply = ui.add_enabled(valid, Button::new(tr(label, &[]))).clicked();
                (cancel, apply)
            })
            .inner
        });
        let (cancel, apply) = modal.inner;
        if apply {
            let text = self.input.trim().to_string();
            match dialog {
                Dialog::Group => {
                    if !bridge.create_manual_group(&text) {
                        return;
                    }
                    self.choose_source(bridge, &text, "");
                }
                Dialog::Subscription => bridge.import_subscription(&text, self.second.trim()),
                Dialog::DeleteGroup(group) => {
                    if bridge.delete_group(&group) && bridge.node_filter_group() == group {
                        self.choose_source(bridge, "", "");
                    }
                }
                Dialog::Move(ids) => {
                    let group = if text.is_empty() {
                        self.second.trim()
                    } else {
                        &text
                    };
                    bridge.bulk_edit_nodes(&ids, &json!({"group":group}));
                }
                Dialog::Info => {}
            }
        }
        if cancel || apply || modal.should_close() {
            self.dialog = None;
        }
    }
}

fn fit_widths(available: f32, manual: Option<&[f32]>, resized: Option<usize>) -> Vec<f32> {
    let content = (available - TABLE_LEFT_PAD - TABLE_RIGHT_PAD).floor();
    if let Some(manual) = manual {
        let mut widths = manual.to_vec();
        let extra = content - widths.iter().sum::<f32>();
        if extra >= 0.0 {
            *widths.last_mut().expect("last table column") += extra;
        } else {
            shrink_widths(&mut widths, -extra, 0, resized);
        }
        return widths;
    }
    let mut widths = COLUMNS.map(|column| column.3);
    let extra = content - widths.iter().sum::<f32>();
    if extra >= 0.0 {
        widths[0] += extra;
        return widths.to_vec();
    }
    let cut = (-extra).min(widths[0] - COLUMNS[0].4);
    widths[0] -= cut;
    shrink_widths(&mut widths, -extra - cut, 1, None);
    widths.to_vec()
}

fn shrink_widths(widths: &mut [f32], deficit: f32, start: usize, keep: Option<usize>) {
    let slack = |index: usize, width: f32| {
        if Some(index) == keep {
            0.0
        } else {
            (width - COLUMNS[index].4).max(0.0)
        }
    };
    let room = widths
        .iter()
        .enumerate()
        .skip(start)
        .map(|(index, &width)| slack(index, width))
        .sum::<f32>();
    if room > 0.0 {
        let fraction = (deficit / room).min(1.0);
        for (index, width) in widths.iter_mut().enumerate().skip(start) {
            *width = (*width - slack(index, *width) * fraction).round();
        }
    }
}

fn toolbar_row(ui: &mut Ui, spacing: f32, content: impl FnOnce(&mut Ui)) {
    ui.allocate_ui_with_layout(
        egui::vec2(ui.available_width(), 32.0),
        egui::Layout::right_to_left(egui::Align::Center),
        |ui| {
            ui.spacing_mut().item_spacing.x = spacing;
            ui.spacing_mut().interact_size.y = 32.0;
            ui.spacing_mut().button_padding.x = 8.0;
            ui.spacing_mut().button_padding.y = 0.0;
            content(ui);
        },
    );
}

fn menu(
    ui: &mut Ui,
    button: Button<'_>,
    width: f32,
    content: impl FnOnce(&mut Ui),
) -> egui::Response {
    let response = ui.add_sized([width, 32.0], button);
    egui::Popup::menu(&response).show(content);
    response
}

fn icon_button(ui: &mut Ui, glyph: &str, label: &str) -> bool {
    ui.add_sized([32.0, 32.0], Button::new(glyph))
        .on_hover_text(tr(label, &[]))
        .clicked()
}

fn render_cell(ui: &mut Ui, theme: &Theme, model: &NodeListModel, row: usize, column: usize) {
    let mut color = if matches!(column, 3 | 7 | 8) {
        theme.text
    } else {
        theme.text_muted
    };
    let text = match column {
        0 => {
            flag::show(ui, &model.country_at(row), egui::vec2(24.0, 18.0));
            ui.add_space(8.0);
            ui.vertical(|ui| {
                let name = model.name_at(row);
                ui.add(egui::Label::new(RichText::new(&name).size(13.0)).truncate())
                    .on_hover_text(name);
                let description = model.description_at(row);
                if !description.is_empty() {
                    ui.add(
                        egui::Label::new(
                            RichText::new(description)
                                .size(10.0)
                                .color(theme.text_muted),
                        )
                        .truncate(),
                    );
                }
            });
            return;
        }
        1 => model.scheme_at(row),
        2 => model.transport_at(row),
        3 => model.server_at(row),
        4 => match model.port_at(row) {
            port if port > 0 => port.to_string(),
            _ => "—".into(),
        },
        5 => model.group_at(row),
        6 => {
            let ping = model.ping_at(row);
            color = theme.ping_color((ping >= 0).then_some(ping));
            if model.pinging_at(row) {
                ui.spinner();
                return;
            } else if ping < 0 {
                "—".into()
            } else {
                format!("{ping} ms")
            }
        }
        7 => {
            let progress = model.speed_progress_at(row);
            let speed = model.speed_at(row);
            if progress >= 0 {
                color = theme.accent;
                format!("{progress}%")
            } else if speed >= 0.0 {
                format!("{speed:.1} MB/s")
            } else {
                "—".into()
            }
        }
        8 => {
            if model.alive_at(row) {
                color = theme.success;
                "OK".into()
            } else if model.tested_at(row) {
                color = theme.danger;
                "✕".into()
            } else {
                color = theme.text_faint;
                "—".into()
            }
        }
        _ => subscription::time_label(&json!(model.last_used_at(row))),
    };
    let label = RichText::new(&text).size(13.0).color(color);
    ui.add(egui::Label::new(if column == 8 { label.family(crate::shell::theme::semibold_family()) } else { label }).truncate())
        .on_hover_text(text);
}

fn row_rule(ui: &Ui, theme: &Theme) {
    let rect = ui.max_rect();
    ui.painter().hline(
        rect.x_range(),
        rect.bottom() - 0.5,
        Stroke::new(1.0, theme.divider),
    );
}

fn inspector_rule(ui: &mut Ui, theme: &Theme) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 1.0), Sense::hover());
    ui.painter().hline(
        rect.x_range(),
        rect.center().y,
        Stroke::new(1.0, theme.divider),
    );
}

fn visible_ids(bridge: &AppBridge) -> Vec<String> {
    bridge
        .node_model
        .nodes()
        .iter()
        .map(|node| node.id.clone())
        .collect()
}
fn range<T>(values: &[T], a: usize, b: usize) -> &[T] {
    &values[a.min(b)..=a.max(b)]
}
fn field<'a>(value: &'a Value, key: &str) -> &'a str {
    value[key].as_str().unwrap_or_default()
}
fn field_value(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}
fn button(ui: &mut Ui, label: &str) -> bool {
    ui.button(tr(label, &[])).clicked()
}
fn copy(ui: &Ui, payload: Option<String>) {
    if let Some(text) = payload {
        ui.ctx().copy_text(text);
    }
}
fn edit(ui: &mut Ui, text: &mut String, hint: &str) {
    ui.add(
        TextEdit::singleline(text)
            .desired_width(f32::INFINITY)
            .hint_text(tr(hint, &[])),
    );
}
/// NodesPage.qml inspector tab: text + 20 px wide, 36 px tall, DemiBold
/// and a 3 px accent bar under the active one.
fn inspector_tab(ui: &mut Ui, theme: &Theme, label: &str, active: bool) -> bool {
    let family = if active {
        crate::shell::theme::semibold_family()
    } else {
        egui::FontFamily::Proportional
    };
    let font = egui::FontId::new(theme.font_normal as f32, family);
    let galley = ui.painter().layout_no_wrap(label.to_owned(), font, theme.text);
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(galley.size().x + 20.0, 36.0),
        egui::Sense::click(),
    );
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    let color = if active || response.hovered() { theme.text } else { theme.text_muted };
    ui.painter().galley_with_override_text_color(
        rect.center() - galley.size() / 2.0,
        galley.clone(),
        color,
    );
    if active {
        let width = (galley.size().x - 4.0).max(16.0);
        let bar = egui::Rect::from_center_size(
            egui::pos2(rect.center().x, rect.bottom() - 1.5),
            egui::vec2(width, 3.0),
        );
        ui.painter().rect_filled(bar, 1.5, theme.accent);
    }
    response.clicked()
}

/// NodesPage.qml `serverInfo`: name, protocol line, ping / speed / status
/// tiles and the `serverDetailRows` table.
fn server_inspector(ui: &mut Ui, theme: &Theme, details: &Value, has_node: bool) {
    let small = theme.font_small as f32;
    if !has_node {
        ui.add(
            egui::Label::new(
                RichText::new(tr("Выберите сервер в списке.", &[]))
                    .size(small)
                    .color(theme.text_muted),
            )
            .wrap(),
        );
        return;
    }
    let protocol = field(details, "protocol");
    let security = field(details, "security");
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 2.0;
        ui.add(
            egui::Label::new(
                RichText::new(field(details, "name"))
                    .size((theme.font_strong + 2) as f32)
                    .family(crate::shell::theme::semibold_family())
                    .color(theme.text),
            )
            .wrap(),
        );
        let subtitle = if !security.is_empty() && !protocol.contains(security) {
            format!("{protocol} \u{00B7} {security}")
        } else {
            protocol.to_owned()
        };
        ui.add(
            egui::Label::new(RichText::new(subtitle).size(small).color(theme.text_muted)).truncate(),
        );
    });

    let ping = details["ping"].as_i64().unwrap_or(-1);
    let speed = details["speed"].as_f64().unwrap_or(-1.0);
    let alive = field(details, "alive");
    let tiles = [
        (
            tr("Пинг", &[]),
            if ping > 0 { format!("{ping} ms") } else { "\u{2014}".to_owned() },
            if ping > 0 { theme.ping_color(Some(ping)) } else { theme.text_faint },
        ),
        (
            tr("Скорость", &[]),
            if speed >= 0.0 { format!("{speed:.1} MB/s") } else { "\u{2014}".to_owned() },
            if speed >= 0.0 { theme.text } else { theme.text_faint },
        ),
        (
            tr("Статус", &[]),
            match alive {
                "ok" => "OK".to_owned(),
                "fail" => "\u{2715}".to_owned(),
                _ => "\u{2014}".to_owned(),
            },
            match alive {
                "ok" => theme.success,
                "fail" => theme.danger,
                _ => theme.text_faint,
            },
        ),
    ];
    let width = ((ui.available_width() - 16.0) / 3.0).floor();
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        for (label, value, color) in tiles {
            let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 54.0), egui::Sense::hover());
            let fill = if theme.dark {
                egui::Color32::from_black_alpha(41)
            } else {
                egui::Color32::from_black_alpha(10)
            };
            ui.painter().rect_filled(rect, theme.radius_small as f32, fill);
            let painter = ui.painter_at(rect);
            let label = painter.layout_no_wrap(label, egui::FontId::proportional(small), theme.text_muted);
            let value = painter.layout_no_wrap(
                value,
                egui::FontId::new(
                    (theme.font_normal + 1) as f32,
                    crate::shell::theme::semibold_family(),
                ),
                color,
            );
            let top = rect.center().y - (label.size().y + 2.0 + value.size().y) / 2.0;
            let left = rect.left() + 10.0;
            painter.galley(egui::pos2(left, top), label.clone(), theme.text_muted);
            painter.galley(egui::pos2(left, top + label.size().y + 2.0), value, color);
        }
    });

    let port = details["port"].as_i64().unwrap_or(0);
    let server = field(details, "server");
    let mut rows = vec![
        (tr("Адрес", &[]), if port > 0 { format!("{server}:{port}") } else { server.to_owned() }),
        (tr("Протокол", &[]), protocol.to_owned()),
        (tr("Транспорт", &[]), field(details, "transport").to_owned()),
        (
            tr("Безопасность", &[]),
            if security.is_empty() { "\u{2014}".to_owned() } else { security.to_owned() },
        ),
    ];
    let mut optional = |label: String, value: String| {
        if !value.is_empty() {
            rows.push((label, value));
        }
    };
    optional("SNI".to_owned(), field(details, "sni").to_owned());
    optional(tr("Отпечаток", &[]), field(details, "fingerprint").to_owned());
    optional(tr("Страна", &[]), field(details, "country").to_owned());
    rows.push((tr("Группа", &[]), field(details, "group").to_owned()));
    let mut optional = |label: String, value: String| {
        if !value.is_empty() {
            rows.push((label, value));
        }
    };
    optional(tr("Подписка", &[]), field(details, "subscription").to_owned());
    let ping_kind = match field(details, "pingKind") {
        "proxy" => "Через прокси",
        "tcping_endpoint" => "TCP до адреса",
        "icmp_endpoint" => "ICMP до адреса",
        "unavailable" => "Недоступно",
        _ => "",
    };
    if !ping_kind.is_empty() {
        optional(tr("Тип пинга", &[]), tr(ping_kind, &[]));
    }
    optional(
        tr("Последнее использование", &[]),
        subscription::time_label(&details["lastUsed"]),
    );
    optional(tr("Описание", &[]), field(details, "description").to_owned());
    for (label, value) in rows {
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 10.0;
            ui.allocate_ui_with_layout(
                egui::vec2(104.0, 0.0),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    ui.set_width(104.0);
                    ui.add(
                        egui::Label::new(RichText::new(label).size(small).color(theme.text_muted)).wrap(),
                    );
                },
            );
            let mut job = egui::text::LayoutJob::simple(
                value,
                egui::FontId::proportional(small),
                theme.text,
                ui.available_width(),
            );
            job.wrap.max_rows = 3;
            job.wrap.break_anywhere = true;
            ui.add(egui::Label::new(job).selectable(true));
        });
    }
}

fn property(ui: &mut Ui, label: &str, value: &str) {
    if !value.is_empty() {
        ui.add(
            egui::Label::new(format!("{} · {value}", tr(label, &[])))
                .selectable(true)
                .wrap(),
        );
    }
}
fn export_json(ui: &Ui, bridge: &mut AppBridge, id: &str, runtime: bool) {
    if let Some((payload, name)) = bridge.node_json_for_export(id, runtime) {
        if bridge.finish_json_export(
            win_native::save_json_file(&tr("Экспорт JSON", &[]), name),
            &payload,
        ) {
            ui.ctx().copy_text(payload);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn automatic_widths_match_qml_at_wide_and_narrow_sizes() {
        let default = COLUMNS.map(|column| column.3);
        let full = default.iter().sum::<f32>() + TABLE_LEFT_PAD + TABLE_RIGHT_PAD;
        let wide = fit_widths(full + 60.0, None, None);
        assert_eq!(wide[0], 260.0);
        assert_eq!(&wide[1..], &default[1..]);
        let reduced = fit_widths(full - 40.0, None, None);
        assert_eq!(reduced[0], 160.0);
        assert_eq!(&reduced[1..], &default[1..]);
        let minimum = fit_widths(530.0, None, None);
        assert_eq!(minimum, COLUMNS.map(|column| column.4));
    }

    #[test]
    fn resized_column_keeps_its_width_when_the_window_shrinks() {
        let mut widths = COLUMNS.map(|column| column.3);
        widths[1] = 240.0;
        let fitted = fit_widths(950.0, Some(&widths), Some(1));
        assert_eq!(fitted[1], 240.0);
        assert!(fitted[0] < widths[0]);
    }

    #[test]
    fn dragging_the_header_writes_the_manual_layout_on_release() {
        let path =
            std::env::temp_dir().join(format!("lumen-table-test-{}.enc", uuid::Uuid::new_v4()));
        let storage = lumen_core::storage::StateStorage::new(path);
        let controller = lumen_app::app_controller::AppController::with_storage(storage);
        let mut bridge = AppBridge::with_controller(controller);
        let mut page = NodesPage::default();
        let theme = Theme::resolve(&Default::default());
        let ctx = egui::Context::default();
        let mut geometry = (egui::Pos2::ZERO, 0.0);
        let mut frame = |events| {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1200.0, 480.0),
                )),
                events,
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |ui| {
                let origin = ui.next_widget_position();
                let widths = fit_widths(ui.available_width() - 2.0 * TABLE_CARD_MARGIN, None, None);
                let start = origin
                    + egui::vec2(
                        TABLE_CARD_MARGIN + TABLE_LEFT_PAD + widths[0] - 1.0,
                        TABLE_CARD_MARGIN + 15.0,
                    );
                geometry = (start, widths[0]);
                page.table(ui, &theme, &mut bridge, 340.0);
            });
            output.textures_delta.clear();
            geometry
        };
        let (from, original_width) = frame(vec![]);
        let to = from + egui::vec2(50.0, 0.0);
        let button = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        frame(vec![egui::Event::PointerMoved(from)]);
        frame(vec![button(from, true)]);
        frame(vec![egui::Event::PointerMoved(to)]);
        frame(vec![button(to, false)]);

        let controller = bridge.controller();
        let layout = &controller.lock().state.settings.node_table_layout;
        assert_eq!(layout["manual"], true);
        assert_eq!(
            layout["name"].as_f64().unwrap() as f32,
            original_width + 50.0
        );
    }
}
