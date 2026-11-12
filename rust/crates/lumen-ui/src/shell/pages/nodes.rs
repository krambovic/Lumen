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

use super::{PageContext, node_editor::NodeEditor};
use crate::shell::{
    onboarding::{import_clipboard, import_config_file},
    theme::Theme,
    win_native,
};
use crate::view_models::{AppBridge, app_bridge::UiEvent, node_list_model::NodeListModel};

const COLUMNS: [(&str, &str, &str, f32, f32); 10] = [
    ("name", "name", "Сервер", 200.0, 140.0),
    ("type", "scheme", "Тип", 80.0, 56.0),
    ("transport", "transport", "Транспорт", 92.0, 60.0),
    ("address", "address", "Адрес", 160.0, 110.0),
    ("port", "", "Порт", 60.0, 48.0),
    ("group", "group", "Группа", 120.0, 70.0),
    ("ping", "ping", "Пинг", 76.0, 56.0),
    ("speed", "speed", "Скорость", 100.0, 76.0),
    ("status", "status", "Статус", 64.0, 50.0),
    ("last", "last", "Последнее использование", 130.0, 70.0),
];

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
    last_widths: Vec<f32>,
    last_auto_width: Option<i32>,
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
        self.toolbar(ui, theme, bridge, &subscriptions, selected_sub);
        if let Some(error) = &self.file_error {
            ui.colored_label(theme.danger, error);
        }
        self.shortcuts(ui, bridge);
        if ui.available_width() >= 850.0 {
            let left = (ui.available_width() * 0.69).max(440.0);
            ui.horizontal_top(|ui| {
                ui.allocate_ui_with_layout(
                    egui::vec2(left, ui.available_height()),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| self.table(ui, theme, bridge, ui.available_height()),
                );
                self.inspector(ui, theme, bridge, selected_sub, ui.available_height());
            });
        } else {
            let height = ui.available_height();
            self.table(ui, theme, bridge, (height * 0.60).max(130.0));
            self.inspector(
                ui,
                theme,
                bridge,
                selected_sub,
                ui.available_height().max(70.0),
            );
        }
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
        ui.horizontal_wrapped(|ui| {
            ui.label(
                RichText::new(tr("Серверы", &[]))
                    .size(theme.font_title as f32)
                    .strong(),
            );
            let name = if let Some(sub) = selected_sub {
                subscription::name(sub).to_string()
            } else if group.is_empty() {
                tr("Все серверы", &[])
            } else {
                group.clone()
            };
            ui.menu_button(name, |ui| {
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
            ui.menu_button(tr("Добавить", &[]), |ui| {
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
            });
            if button(ui, "Пинг") {
                let targets = self.targets(bridge);
                if !targets.is_empty() {
                    bridge.ping_nodes(Some(&targets));
                }
            }
            ui.menu_button(tr("Ещё", &[]), |ui| {
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
        });
        ui.horizontal_wrapped(|ui| {
            if ui
                .add(
                    TextEdit::singleline(&mut self.query)
                        .desired_width(230.0)
                        .hint_text(tr("Поиск", &[])),
                )
                .changed()
            {
                bridge.set_node_filter(&group, &self.query);
                self.picked.clear();
            }
            let key = bridge.node_sort_key();
            let ascending = bridge.node_sort_ascending();
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
            ComboBox::from_id_salt("node-sort-order")
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
            if ui
                .add_enabled(
                    key != "manual",
                    Button::new(if ascending { "▲" } else { "▼" }),
                )
                .clicked()
            {
                bridge.set_node_sort(&key, !ascending);
            }
            if !self.picked.is_empty() {
                ui.label(tr(
                    "Выбрано: {count}",
                    &[("count", &self.picked.len().to_string())],
                ));
            }
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
        let width = ui.available_width();
        let auto_width = width.round() as i32;
        let auto_reflow = self.widths.is_none() && self.last_auto_width != Some(auto_width);
        self.last_auto_width = Some(auto_width);
        let preferred = self.widths.clone().unwrap_or_else(|| fit_widths(width));
        let table_width = preferred.iter().sum::<f32>() + 72.0;
        let mut sort_clicked = None;
        let mut measured = Vec::new();
        Frame::new()
            .fill(theme.card)
            .stroke(Stroke::new(1.0, theme.border_solid))
            .corner_radius(theme.radius as u8)
            .show(ui, |ui| {
                ScrollArea::horizontal()
                    .id_salt("nodes-columns")
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        ui.set_width(table_width.max(width - 4.0));
                        let mut table = TableBuilder::new(ui)
                            .id_salt("nodes-table")
                            .resizable(true)
                            .striped(true)
                            .sense(Sense::click_and_drag())
                            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                            .min_scrolled_height((height - 62.0).max(62.0))
                            .max_scroll_height((height - 62.0).max(62.0))
                            .auto_shrink([false, false]);
                        for (column, width) in COLUMNS.iter().zip(&preferred) {
                            table = table.column(
                                Column::initial(*width)
                                    .at_least(column.4)
                                    .at_most(2000.0)
                                    .clip(true),
                            );
                        }
                        if auto_reflow {
                            table.reset();
                        }
                        if let Some(index) = self.scroll_row.take() {
                            table = table.scroll_to_row(index, Some(egui::Align::Center));
                        }
                        table
                            .header(30.0, |mut row| {
                                for (_, key, label, _, _) in COLUMNS {
                                    row.col(|ui| {
                                        let caption = tr(label, &[]);
                                        if key.is_empty() {
                                            ui.label(caption);
                                        } else {
                                            let arrow = if key == sort {
                                                if ascending { " ▲" } else { " ▼" }
                                            } else {
                                                ""
                                            };
                                            if ui
                                                .add(
                                                    Button::new(format!("{caption}{arrow}"))
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
                                measured.extend_from_slice(body.widths());
                                body.rows(row_height, ids.len(), |mut row| {
                                    let index = row.index();
                                    let id = &ids[index];
                                    row.set_selected(self.picked.contains(id));
                                    for column in 0..COLUMNS.len() {
                                        row.col(|ui| {
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
                    });
            });
        if released {
            self.drag_start = None;
        }
        if ids.is_empty() {
            ui.label(tr("Серверов нет — импортируйте ссылку или подписку", &[]));
        }
        if let Some(key) = sort_clicked {
            if key == sort && !ascending {
                bridge.set_node_sort("manual", true);
            } else {
                bridge.set_node_sort(key, key != sort);
            }
        }
        if !auto_reflow
            && !self.last_widths.is_empty()
            && measured != self.last_widths
            && (released || ui.input(|input| input.pointer.primary_down()))
        {
            self.widths = Some(measured.clone());
            let mut layout = serde_json::Map::from_iter([("manual".into(), json!(true))]);
            for (column, value) in COLUMNS.iter().zip(&measured) {
                layout.insert(column.0.into(), json!(value.round()));
            }
            bridge.set_node_table_layout(&Value::Object(layout));
        }
        self.last_widths = measured;
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
        ScrollArea::vertical()
            .id_salt("nodes-inspector")
            .max_height(height)
            .show(ui, |ui| {
                if sub.is_some() {
                    ui.horizontal_wrapped(|ui| {
                        ui.selectable_value(&mut self.inspector_node, false, tr("Подписка", &[]));
                        ui.selectable_value(&mut self.inspector_node, true, tr("Сервер", &[]));
                    });
                }
                if let Some(sub) = sub.filter(|_| !self.inspector_node) {
                    subscription::inspector(ui, theme, sub);
                    let url = field(sub, "url");
                    ui.horizontal_wrapped(|ui| {
                        if button(ui, "Обновить") {
                            bridge.update_subscription(url);
                        }
                        if button(ui, "Свойства подписки") {
                            self.open_info(sub);
                        }
                        if sub["presentation"]["hideUrl"].as_bool() != Some(true)
                            && button(ui, "Копировать ссылку")
                        {
                            copy(ui, bridge.copy_text(url));
                        }
                        if button(ui, "Удалить подписку (с серверами)") {
                            bridge.remove_subscription(url, true);
                        }
                    });
                    return;
                }
                let selected = self
                    .anchor
                    .as_ref()
                    .filter(|id| self.picked.contains(*id))
                    .or_else(|| self.picked.iter().next())
                    .map(String::as_str)
                    .unwrap_or_else(|| bridge.selected_node_id());
                let id = selected.to_string();
                let details = bridge.node_details(&id);
                if details.get("id").is_none() {
                    ui.label(tr("Выберите сервер", &[]));
                    return;
                }
                ui.heading(field(&details, "name"));
                let port = details["port"].as_i64().unwrap_or(0);
                let address = if port > 0 {
                    format!("{}:{port}", field(&details, "server"))
                } else {
                    field(&details, "server").to_string()
                };
                property(ui, "Адрес", &address);
                for (label, key) in [
                    ("Протокол", "protocol"),
                    ("Транспорт", "transport"),
                    ("Безопасность", "security"),
                    ("SNI", "sni"),
                    ("Отпечаток", "fingerprint"),
                    ("Страна", "country"),
                    ("Группа", "group"),
                    ("Подписка", "subscription"),
                    ("Описание", "description"),
                ] {
                    property(ui, label, field(&details, key));
                }
                let ping_kind = match field(&details, "pingKind") {
                    "proxy" => "Через прокси",
                    "tcping_endpoint" => "TCP до адреса",
                    "icmp_endpoint" => "ICMP до адреса",
                    "unavailable" => "Недоступно",
                    _ => "",
                };
                property(ui, "Тип пинга", &tr(ping_kind, &[]));
                property(
                    ui,
                    "Последнее использование",
                    &subscription::time_label(&details["lastUsed"]),
                );
                let ping = details["ping"].as_i64().unwrap_or(-1);
                if ping >= 0 {
                    property(ui, "Пинг", &format!("{ping} ms"));
                }
                ui.horizontal_wrapped(|ui| {
                    if button(ui, "Установить как активный") {
                        bridge.select_node(&id);
                    }
                    if button(ui, "Редактировать") {
                        self.editor.edit(bridge, &id);
                    }
                    if button(ui, "Пинг") {
                        bridge.ping_nodes(Some(&HashSet::from([id.clone()])));
                    }
                    if button(ui, "Копировать ссылку") {
                        copy(ui, bridge.copy_node_link(&id));
                    }
                    if button(ui, "Экспорт в QR-код") {
                        bridge.show_node_qr(&id);
                    }
                });
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
                    .strong(),
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
                    ComboBox::from_id_salt("nodes-subscription-info")
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
                    ComboBox::from_id_salt("nodes-move-group")
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

fn fit_widths(available: f32) -> Vec<f32> {
    let content = (available - 72.0).max(0.0);
    let total = COLUMNS.iter().map(|column| column.3).sum::<f32>();
    let min = COLUMNS.iter().map(|column| column.4).sum::<f32>();
    let fraction = ((content - min) / (total - min)).clamp(0.0, 1.0);
    COLUMNS
        .iter()
        .enumerate()
        .map(|(index, column)| {
            column.4
                + (column.3 - column.4) * fraction
                + if index == 0 {
                    (content - total).max(0.0)
                } else {
                    0.0
                }
        })
        .collect()
}

fn render_cell(ui: &mut Ui, theme: &Theme, model: &NodeListModel, row: usize, column: usize) {
    let mut color = theme.text;
    let text = match column {
        0 => format!("{} {}", model.flag_emoji_at(row), model.name_at(row)),
        1 => model.protocol_at(row),
        2 => model.transport_at(row),
        3 => model.server_at(row),
        4 => model.port_at(row).to_string(),
        5 => model.group_at(row),
        6 => {
            let ping = model.ping_at(row);
            color = theme.ping_color((ping >= 0).then_some(ping));
            if model.pinging_at(row) {
                "…".into()
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
                format!("{progress}%")
            } else if speed >= 0.0 {
                format!("{speed:.1} MB/s")
            } else {
                "—".into()
            }
        }
        8 => {
            if !model.runtime_supported_at(row) {
                color = theme.warning;
                tr("Недоступно", &[])
            } else if !model.tested_at(row) {
                "—".into()
            } else if model.alive_at(row) {
                color = theme.success;
                tr("Доступен", &[])
            } else {
                color = theme.danger;
                tr("Недоступно", &[])
            }
        }
        _ => subscription::time_label(&json!(model.last_used_at(row))),
    };
    ui.add(egui::Label::new(RichText::new(&text).size(13.0).color(color)).truncate())
        .on_hover_text(text);
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
