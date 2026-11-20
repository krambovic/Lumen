//! Subscription traffic, expiration, provider presentation and properties.

use chrono::{DateTime, Local, Utc};
use egui::{Color32, RichText, Ui};
use lumen_core::i18n::tr;
use serde_json::Value;

use super::{copy, field, field_value, property};
use crate::shell::{pages::about::open_url, theme::Theme};
use crate::view_models::AppBridge;

pub(super) fn name(sub: &Value) -> &str {
    let name = field(sub, "name");
    if name.is_empty() { group(sub) } else { name }
}

pub(super) fn group(sub: &Value) -> &str {
    [field(sub, "group"), field(sub, "name")]
        .into_iter()
        .find(|text| !text.is_empty())
        .unwrap_or("Default")
}

pub(super) struct Status {
    used: Option<f64>,
    limit: Option<f64>,
    days: Option<i64>,
    state: String,
    active: bool,
}

impl Status {
    pub(super) fn new(sub: &Value) -> Self {
        let info = &sub["userinfo"];
        let limit = number(&info["trafficLimitBytes"]).or_else(|| number(&info["total"]));
        let used = number(&info["trafficUsedBytes"]).or_else(|| {
            let up = number(&info["upload"]);
            let down = number(&info["download"]);
            (limit.is_some() || up.is_some() || down.is_some())
                .then(|| up.unwrap_or(0.0) + down.unwrap_or(0.0))
        });
        let days = date(expiration(info))
            .map(|date| ((date - Utc::now()).num_seconds() as f64 / 86400.0).ceil() as i64);
        let mut active = days.is_none_or(|days| days > 0);
        let state = field(info, "userStatus");
        let state = if !state.is_empty() {
            let lower = state.to_lowercase();
            active = !["expired", "disabled", "limited", "inactive"]
                .iter()
                .any(|status| lower.contains(status));
            state.into()
        } else if info.get("isActive").is_some() || days.is_some() {
            active &= info["isActive"].as_bool().unwrap_or(true);
            tr(
                if active {
                    "Активна"
                } else {
                    "Неактивна"
                },
                &[],
            )
        } else {
            String::new()
        };
        Self {
            used,
            limit,
            days,
            state,
            active,
        }
    }

    fn fraction(&self) -> Option<f32> {
        let limit = self.limit.filter(|limit| *limit > 0.0)?;
        self.days
            .is_none_or(|days| days > 0)
            .then(|| (self.used.unwrap_or(0.0) / limit).clamp(0.0, 1.0) as f32)
    }

    pub(super) fn color(&self, theme: &Theme) -> Color32 {
        if !self.active || self.fraction().is_some_and(|fraction| fraction >= 0.9) {
            theme.danger
        } else if self.days.is_some_and(|days| days <= 3)
            || self.fraction().is_some_and(|fraction| fraction >= 0.75)
        {
            theme.warning
        } else {
            theme.success
        }
    }

    /// `computeSubState().lineHtml`: the traffic part in the traffic colour
    /// (green, yellow from 75%, red from 90%), the days left in red when
    /// expired, yellow within 3 days and muted otherwise, joined by a faint
    /// " · ".
    pub(super) fn segments(&self, theme: &Theme) -> Vec<(String, Color32)> {
        let mut segments = Vec::new();
        let expired = self.days.is_some_and(|days| days <= 0);
        if let Some(used) = self.used.filter(|_| !expired) {
            if let Some(limit) = self.limit.filter(|limit| *limit > 0.0) {
                let text = format!("{} {}", bytes(used), tr("из {total}", &[("total", &bytes(limit))]));
                segments.push((text, self.traffic_color(theme)));
            } else {
                segments.push((tr("Безлимитный трафик", &[]), theme.success));
            }
        }
        if let Some(days) = self.days {
            let (text, color) = if days <= 0 {
                (tr("Подписка истекла", &[]), theme.danger)
            } else {
                let text = tr("осталось {days} дн.", &[("days", &days.to_string())]);
                (text, if days <= 3 { theme.warning } else { theme.text_muted })
            };
            if !segments.is_empty() {
                segments.push((" · ".into(), theme.text_faint));
            }
            segments.push((text, color));
        }
        segments
    }

    /// `trafficColor`: what is left of a limited plan.
    pub(super) fn traffic_color(&self, theme: &Theme) -> Color32 {
        match self.fraction() {
            Some(fraction) if fraction >= 0.9 => theme.danger,
            Some(fraction) if fraction >= 0.75 => theme.warning,
            _ => theme.success,
        }
    }

    /// The usage bar of a limited, unexpired plan (`hasBar` / `fraction`).
    pub(super) fn bar(&self) -> Option<f32> {
        self.fraction()
    }
}

pub(super) fn inspector(ui: &mut Ui, theme: &Theme, sub: &Value) {
    let info = &sub["userinfo"];
    let presentation = &sub["presentation"];
    let title = field(info, "profileTitle");
    let state = Status::new(sub);
    ui.allocate_ui_with_layout(
        egui::vec2(ui.available_width(), 24.0),
        egui::Layout::right_to_left(egui::Align::Center),
        |ui| {
            if !state.state.is_empty() {
                let color = if state.active {
                    theme.success
                } else {
                    theme.danger
                };
                egui::Frame::new()
                    .fill(color.gamma_multiply(0.16))
                    .corner_radius(12)
                    .inner_margin(egui::Margin::symmetric(8, 3))
                    .show(ui, |ui| {
                        ui.label(
                            RichText::new(format!("● {}", state.state))
                                .size(theme.font_small as f32)
                                .color(color),
                        );
                    });
            }
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                let title = if title.is_empty() { name(sub) } else { title };
                ui.add(egui::Label::new(RichText::new(title).size(16.0).family(crate::shell::theme::semibold_family())).truncate());
            });
        },
    );
    let quota_visible = state.used.is_some() || state.limit.is_some();
    ui.allocate_ui_with_layout(
        egui::vec2(
            ui.available_width(),
            if quota_visible { 104.0 } else { 54.0 },
        ),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.spacing_mut().item_spacing.x = 14.0;
            if quota_visible {
                traffic_ring(ui, theme, &state);
            }
            let expiry = time_label(expiration(info));
            if !expiry.is_empty() {
                ui.allocate_ui_with_layout(
                    egui::vec2(ui.available_width(), 58.0),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        ui.spacing_mut().item_spacing.y = 2.0;
                        ui.label(
                            RichText::new(tr("Истекает", &[]))
                                .size(12.0)
                                .color(theme.text_muted),
                        );
                        ui.label(RichText::new(expiry).size(14.0));
                        if let Some(days) = state.days {
                            let text = if days <= 0 {
                                tr("Подписка истекла", &[])
                            } else {
                                tr("осталось {days} дн.", &[("days", &days.to_string())])
                            };
                            ui.label(RichText::new(text).size(12.0).color(state.color(theme)));
                        }
                    },
                );
            }
        },
    );
    let description = field(presentation, "description");
    if !description.is_empty() && !description.eq_ignore_ascii_case(name(sub)) {
        ui.label(
            RichText::new(description)
                .size(12.0)
                .color(theme.text_muted),
        );
    }
    for banner in presentation["banners"].as_array().into_iter().flatten() {
        egui::Frame::new()
            .fill(theme.accent_soft)
            .corner_radius(theme.radius_small as u8)
            .inner_margin(8)
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.label(RichText::new(field(banner, "text")).size(12.0));
                let url = field(banner, "url");
                let label = field(banner, "button");
                if !url.is_empty()
                    && ui
                        .button(if label.is_empty() {
                            tr("Открыть", &[])
                        } else {
                            label.into()
                        })
                        .clicked()
                {
                    open_url(ui, url);
                }
            });
    }
    let announcement = field(presentation, "announcement");
    if !announcement.is_empty() {
        egui::Frame::new()
            .fill(theme.accent.gamma_multiply(0.14))
            .corner_radius(theme.radius_small as u8)
            .inner_margin(8)
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.label(RichText::new(announcement).size(12.0));
            });
    }
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 14.0;
        for link in presentation["links"].as_array().into_iter().flatten() {
            // NodesPage.qml provider links: accent text, underlined in the
            // text colour on hover.
            let text = tr(field(link, "label"), &[]);
            let font = egui::FontId::proportional((theme.font_small + 1) as f32);
            let galley = ui.painter().layout_no_wrap(text, font, theme.accent);
            let (rect, response) = ui.allocate_exact_size(galley.size(), egui::Sense::click());
            let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
            let color = if response.hovered() { theme.text } else { theme.accent };
            ui.painter().galley_with_override_text_color(rect.min, galley, color);
            if response.hovered() {
                let y = rect.bottom() - 1.0;
                ui.painter().hline(rect.x_range(), y, egui::Stroke::new(1.0, color));
            }
            if response.clicked() {
                open_url(ui, field(link, "url"));
            }
        }
    });
}

fn traffic_ring(ui: &mut Ui, theme: &Theme, state: &Status) {
    let (rect, _) = ui.allocate_exact_size(egui::Vec2::splat(104.0), egui::Sense::hover());
    let center = rect.center();
    let radius = 46.0;
    let painter = ui.painter();
    // The Canvas arc is centred on `radius`; egui strokes circles outside.
    painter.circle_stroke(
        center,
        radius - 4.0,
        egui::Stroke::new(
            8.0,
            Color32::from_black_alpha(if theme.dark { 71 } else { 20 }),
        ),
    );
    // Unlimited plans draw the full ring (`limited ? fraction : 1`).
    let limited = state.limit.is_some_and(|limit| limit > 0.0);
    let fraction = if limited { state.fraction().unwrap_or(0.0) } else { 1.0 };
    if fraction > 0.0 {
        let color = state.color(theme);
        let steps = ((fraction.min(1.0) * 96.0).ceil() as usize).max(2);
        let points = (0..=steps)
            .map(|i| {
                let angle = std::f32::consts::TAU * fraction.min(1.0) * i as f32 / steps as f32
                    - std::f32::consts::FRAC_PI_2;
                center + egui::vec2(angle.cos(), angle.sin()) * radius
            })
            .collect::<Vec<_>>();
        painter.circle_filled(points[0], 4.0, color);
        painter.circle_filled(points[steps], 4.0, color);
        painter.add(egui::Shape::line(points, egui::Stroke::new(8.0, color)));
    }
    let used = state.used.map_or_else(|| "—".to_string(), bytes);
    let used = painter.layout_no_wrap(
        used,
        egui::FontId::new(
            (theme.font_normal + 2) as f32,
            crate::shell::theme::semibold_family(),
        ),
        theme.text,
    );
    let (limit, limit_color) = if limited {
        (
            tr("из {total}", &[("total", &bytes(state.limit.unwrap_or_default()))]),
            theme.text_muted,
        )
    } else {
        (tr("Безлимитный трафик", &[]), theme.success)
    };
    let mut job = egui::text::LayoutJob::simple(
        limit,
        egui::FontId::proportional(theme.font_small as f32),
        limit_color,
        84.0,
    );
    job.halign = egui::Align::Center;
    let limit = painter.layout_job(job);
    let top = center.y - (used.size().y + limit.size().y) / 2.0;
    painter.galley(
        egui::pos2(center.x - used.size().x / 2.0, top),
        used.clone(),
        theme.text,
    );
    painter.galley(
        egui::pos2(center.x, top + used.size().y),
        limit,
        limit_color,
    );
}

pub(super) fn details(ui: &mut Ui, theme: &Theme, bridge: &mut AppBridge, sub: &Value) {
    let info = &sub["userinfo"];
    inspector(ui, theme, sub);
    property(ui, "Обновлено", &time_label(&sub["updated_at"]));
    property(ui, "Серверов", &field_value(&sub["node_count"]));
    let url = field(sub, "url");
    if !url.is_empty() {
        if sub["presentation"]["hideUrl"].as_bool() == Some(true) {
            property(ui, "Ссылка подписки", &tr("Скрыта провайдером", &[]));
        } else {
            property(ui, "URL", url);
            if ui.button(tr("Копировать ссылку", &[])).clicked() {
                copy(ui, bridge.copy_text(url));
            }
        }
    }
    property(ui, "Группа", group(sub));
    for (label, key) in [
        ("Пользователь", "username"),
        ("Осталось дней", "daysLeft"),
        ("Сброс трафика", "trafficLimitStrategy"),
        ("Название профиля", "profileTitle"),
        ("Provider ID", "providerId"),
        ("Объявление", "announcement"),
        ("Интервал обновления", "profileUpdateInterval"),
        ("Описание", "profileDescription"),
        ("Баннер", "bannerText"),
        ("Кнопка баннера", "bannerButtonText"),
        ("Ссылка баннера", "bannerButtonUrl"),
        ("Цвет баннера", "bannerBgColor"),
        ("Premium", "premiumUrl"),
        ("Email", "supportEmail"),
        ("Поддержка", "supportUrl"),
        ("Сайт", "profileUrl"),
    ] {
        property(ui, label, &field_value(&info[key]));
    }
    if info.get("trafficUsed").is_some() || info.get("trafficLimit").is_some() {
        let used = field_value(&info["trafficUsed"]);
        let limit = field_value(&info["trafficLimit"]);
        property(
            ui,
            "Трафик",
            &format!(
                "{} / {}",
                if used.is_empty() { "?" } else { &used },
                if limit.is_empty() { "∞" } else { &limit }
            ),
        );
    }
    for feature in sub["presentation"]["featureRows"]
        .as_array()
        .into_iter()
        .flatten()
    {
        let key = field(feature, "key");
        property(
            ui,
            feature_name(key),
            &format!(
                "{} · {}",
                field_value(&feature["value"]),
                tr(field(feature, "status"), &[])
            ),
        );
    }
}

fn feature_name(key: &str) -> &str {
    match key {
        "new-url" => "Новый URL подписки",
        "new-domain" => "Новый домен подписки",
        "subscription-always-hwid-enable" => "Обязательный HWID",
        "notification-subs-expire" => "Уведомлять об окончании",
        "hide-settings" => "Скрыть настройки серверов",
        "server-address-resolve-enable" => "Резолвинг адресов серверов",
        "server-address-resolve-dns-domain" => "Домен DNS для резолвинга",
        "server-address-resolve-dns-ip" => "IP DNS для резолвинга",
        "subscription-autoconnect" => "Автоподключение",
        "subscription-autoconnect-type" => "Режим автоподключения",
        "subscription-ping-onopen-enabled" => "Автопинг при открытии",
        "subscription-auto-update-enable" => "Автообновление подписок",
        "fragmentation-enable" => "Фрагментация",
        "fragmentation-packets" => "Пакеты фрагментации",
        "fragmentation-length" => "Размер фрагментации",
        "fragmentation-interval" => "Интервал фрагментации",
        "ping-type" => "Тип пинга",
        "check-url-via-proxy" => "URL проверки через прокси",
        "change-user-agent" => "User-Agent",
        "app-auto-start" => "Автозапуск приложения",
        "subscription-auto-update-open-enable" => "Обновлять подписки при запуске",
        "per-app-proxy-mode" => "Режим прокси приложений",
        "per-app-proxy-list" => "Список приложений",
        "sniffing-enable" => "Sniffing",
        "subscriptions-collapse" => "Сворачивание подписки",
        "ping-result" => "Отображение пинга",
        "mux-enable" => "Mux",
        "mux-tcp-connections" => "Mux TCP",
        "mux-xudp-connections" => "Mux XUDP",
        "mux-quic" => "Mux QUIC",
        "exclude-routes" => "Исключённые маршруты",
        _ => key,
    }
}

fn number(value: &Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| value.as_str()?.parse().ok())
        .filter(|number| number.is_finite())
}
fn expiration(info: &Value) -> &Value {
    if field_value(&info["expiresAt"]).is_empty() {
        &info["expire"]
    } else {
        &info["expiresAt"]
    }
}

fn date(value: &Value) -> Option<DateTime<Utc>> {
    if let Some(raw) = number(value).filter(|number| *number > 0.0) {
        let millis = if raw > 32_000_000_000.0 {
            raw
        } else {
            raw * 1000.0
        };
        DateTime::from_timestamp_millis(millis as i64)
    } else {
        DateTime::parse_from_rfc3339(value.as_str()?)
            .ok()
            .map(|time| time.to_utc())
    }
}

pub(super) fn time_label(value: &Value) -> String {
    if let Some(date) = date(value) {
        date.with_timezone(&Local)
            .format("%d.%m.%Y %H:%M")
            .to_string()
    } else if value.as_i64() == Some(0) {
        String::new()
    } else {
        field_value(value)
    }
}

fn bytes(mut number: f64) -> String {
    if number < 0.0 {
        return number.to_string();
    }
    let units = ["Б", "КБ", "МБ", "ГБ", "ТБ", "ПБ"];
    let mut index = 0;
    while number >= 1024.0 && index + 1 < units.len() {
        number /= 1024.0;
        index += 1;
    }
    let precision = if index == 0 { 0 } else { 2 };
    format!("{number:.precision$} {}", tr(units[index], &[]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn subscription_accepts_epoch_milliseconds_and_limits_traffic() {
        assert_eq!(date(&json!(1700000000)), date(&json!(1700000000000_i64)));
        let status = Status::new(&json!({"userinfo":{"upload":8,"download":42,"total":100}}));
        assert_eq!(status.fraction(), Some(0.5));
    }
}
