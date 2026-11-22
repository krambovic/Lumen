//! Project information and support dialogs from `AboutPage.qml`.

use egui::{
    Align, Color32, RichText, ScrollArea, TextureHandle,
    Ui, Vec2, vec2,
};
use lumen_core::i18n::tr;

use super::PageContext;
use super::fluent_button;
use crate::shell::theme::{Theme, semibold};

pub(in crate::shell) const DONATION_URL: &str =
    "https://www.donationalerts.com/r/studiobebraedition";
pub(in crate::shell) const USDT_ADDRESS: &str = "TWHsuUDru4pXBcGpfeKfzymbkfajyFnb2s";
const QR: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../windows/xray_fluent/qml_app/qml/assets/usdt-trc20-qr.png"
));

#[derive(Clone, Copy)]
enum Dialog {
    Support,
    Usdt,
}

#[derive(Default)]
pub(super) struct AboutPage {
    dialog: Option<Dialog>,
    qr: Option<TextureHandle>,
}

impl AboutPage {
    pub(super) fn render(&mut self, ui: &mut Ui, page: &mut PageContext<'_>) {
        let theme = page.theme;
        let bridge = &mut *page.bridge;
        ScrollArea::vertical().id_salt("about-page").auto_shrink([false, false]).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 16.0;
            ui.label(RichText::new(bridge.app_name()).font(semibold(theme.font_title as f32)));
            ui.label(RichText::new(format!("v{}", bridge.app_version())).size(theme.font_small as f32).color(theme.text_faint));
            divider(ui, theme);
            ui.label(RichText::new(tr("Lumen — самостоятельный Windows-клиент для VPN/TUN, системного прокси, маршрутизации, управления серверами и обхода DPI (DPI bypass) через zapret.", &[])).size(15.0));
            let description = tr("Проект предлагает графический интерфейс на базе QML с эффектами Mica/Acrylic и аппаратным ускорением рендеринга.", &[]).replace("QML", "egui");
            ui.label(RichText::new(description).color(theme.text_muted));
            divider(ui, theme);
            ui.label(RichText::new(tr("Ссылки", &[])).font(semibold(theme.font_strong as f32)));
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 12.0;
                fluent_button::style(ui);
                if ui.add(fluent_button::button(theme, "\u{e724}", &tr("Telegram канал", &[]), true)).clicked() { open_url(ui, "https://telegram.me/lumenkvn"); }
                if ui.add(fluent_button::button(theme, "\u{e734}", &tr("Рекомендуемый VPN", &[]), false)).clicked() { open_url(ui, bridge.recommended_vpn_url()); }
                if ui.add(fluent_button::button(theme, "\u{e8f1}", &tr("Поддержать проект", &[]), false)).clicked() { self.dialog = Some(Dialog::Support); }
                if ui.add(fluent_button::button(theme, "\u{e80f}", &tr("Показать приветствие", &[]), false)).clicked() { bridge.show_onboarding_again(); }
            });
        });
        let Some(dialog) = self.dialog else {
            return;
        };
        use crate::shell::fluent_dialog::{DialogAction, FluentDialog, body_text};
        use crate::shell::widgets::{ButtonKind, FluentButton};
        let mut next = Some(dialog);
        let action = match dialog {
            // supportDialog: 430 px, column spacing 12, full-width buttons.
            Dialog::Support => FluentDialog::new("about-support", tr("Поддержать проект", &[]), 430.0)
                .no_ok()
                .cancel(tr("Закрыть", &[]))
                .show(page.ctx, theme, |ui| {
                    body_text(
                        ui,
                        &tr("Выберите удобный способ поддержки:", &[]),
                        egui::FontId::proportional(theme.font_normal as f32),
                        theme.text_muted,
                    );
                    ui.add_space(12.0);
                    let width = ui.available_width();
                    if FluentButton::new(ButtonKind::Accent, "\u{e8f1}", "DonationAlerts")
                        .width(width)
                        .show(ui, theme)
                        .clicked()
                    {
                        next = None;
                        open_url(ui, DONATION_URL);
                    }
                    ui.add_space(12.0);
                    if FluentButton::new(ButtonKind::Ghost, "\u{e8c7}", "USDT (TRC20)")
                        .width(width)
                        .show(ui, theme)
                        .clicked()
                    {
                        next = Some(Dialog::Usdt);
                    }
                }),
            // usdtDialog: 420 px, a centred 250 px QR, the read-only address
            // and a centred accent copy button, 12 px apart.
            Dialog::Usdt => FluentDialog::new("about-usdt", "USDT (TRC20)", 420.0)
                .no_ok()
                .cancel(tr("Закрыть", &[]))
                .show(page.ctx, theme, |ui| {
                    if self.qr.is_none() {
                        self.qr = load_qr(page.ctx);
                    }
                    let width = ui.available_width();
                    if let Some(texture) = &self.qr {
                        let side = (page.ctx.content_rect().height() - 260.0).clamp(64.0, 250.0).min(width);
                        let (row, _) = ui.allocate_exact_size(vec2(width, side), egui::Sense::hover());
                        let rect = egui::Rect::from_center_size(row.center(), Vec2::splat(side));
                        ui.painter().image(
                            texture.id(),
                            rect,
                            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                            Color32::WHITE,
                        );
                    }
                    ui.add_space(12.0);
                    let mut address = USDT_ADDRESS;
                    ui.add_sized(
                        vec2(width, theme.control_height as f32),
                        egui::TextEdit::singleline(&mut address)
                            .background_color(theme.control_fill)
                            .horizontal_align(Align::Center)
                            .vertical_align(Align::Center),
                    );
                    ui.add_space(12.0);
                    let label = tr("Копировать адрес", &[]);
                    let button_width = ui
                        .painter()
                        .layout_no_wrap(label.clone(), crate::shell::theme::semibold(theme.font_normal as f32), theme.text)
                        .size()
                        .x
                        + ui.painter().layout_no_wrap("\u{e8c8}".into(), egui::FontId::proportional(theme.font_normal as f32), theme.text).size().x
                        + 8.0
                        + 28.0;
                    ui.horizontal(|ui| {
                        ui.add_space(((width - button_width) / 2.0).max(0.0));
                        if FluentButton::new(ButtonKind::Accent, "\u{e8c8}", &label).show(ui, theme).clicked()
                            && let Some(text) = bridge.copy_text(USDT_ADDRESS)
                        {
                            ui.ctx().copy_text(text);
                        }
                    });
                }),
        };
        if action != DialogAction::None {
            next = None;
        }
        self.dialog = next;
    }
}

fn divider(ui: &mut Ui, theme: &Theme) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), egui::Sense::hover());
    ui.painter().rect_filled(rect, 0.0, theme.divider);
}

pub(in crate::shell) fn open_url(_ui: &Ui, url: &str) {
    crate::shell::win_native::open_url(url);
}

fn load_qr(ctx: &egui::Context) -> Option<TextureHandle> {
    let image = image::load_from_memory_with_format(QR, image::ImageFormat::Png)
        .ok()?
        .to_rgba8();
    let size = [image.width() as usize, image.height() as usize];
    let image = egui::ColorImage::from_rgba_unmultiplied(size, image.as_raw());
    Some(ctx.load_texture("lumen-usdt-trc20", image, egui::TextureOptions::NEAREST))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_support_qr_loads_as_a_square_texture() {
        let texture = load_qr(&egui::Context::default()).unwrap();
        assert_eq!(texture.size()[0], texture.size()[1]);
        assert!(texture.size()[0] >= 200);
    }
}
