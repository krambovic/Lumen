use egui::{
    Align2, Color32, ColorImage, CornerRadius, FontId, Id, Rect, Sense, Stroke, StrokeKind,
    TextureHandle, TextureOptions, Ui, Vec2, load::SizeHint, pos2,
};
use lumen_core::country_flags::{draw_flag, get_flag_emoji};

use crate::view_models::app_bridge::flag_svg;

/// QML `DashboardFlag`/`RowFlag`: the sprite is rasterized at the box's
/// physical pixel size (`sourceSize` + `smooth`), fitted with its aspect
/// ratio kept, and outlined by the 20% black rounded border.
pub(crate) fn show(ui: &mut Ui, country: &str, size: Vec2) {
    if country.is_empty() {
        return;
    }
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    if ui.is_rect_visible(rect) {
        paint(ui, rect, country);
    }
}

fn paint(ui: &Ui, rect: Rect, country: &str) {
    let country = country.to_ascii_lowercase();
    let ctx = ui.ctx();
    let ppp = ctx.pixels_per_point();
    let px = [
        (rect.width() * ppp).round().max(1.0) as u32,
        (rect.height() * ppp).round().max(1.0) as u32,
    ];
    let id = Id::new(("country-flag", &country, px));
    let texture = match ctx.data_mut(|data| data.get_temp::<Option<TextureHandle>>(id)) {
        Some(texture) => texture,
        None => {
            let texture = rasterize(&country, px).map(|image| {
                ctx.load_texture(
                    format!("flag-{country}-{}x{}", px[0], px[1]),
                    image,
                    TextureOptions::LINEAR,
                )
            });
            ctx.data_mut(|data| data.insert_temp(id, texture.clone()));
            texture
        }
    };
    let painter = ui.painter_at(rect);
    match texture {
        Some(texture) => {
            let size = texture.size_vec2() / ppp;
            // Snap to device pixels so the 1:1 texture is not resampled.
            let min = Rect::from_center_size(rect.center(), size).min;
            let min = pos2((min.x * ppp).round() / ppp, (min.y * ppp).round() / ppp);
            painter.image(
                texture.id(),
                Rect::from_min_size(min, size),
                Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                Color32::WHITE,
            );
        }
        None => {
            let emoji = get_flag_emoji(&country);
            if emoji.is_empty() {
                return;
            }
            painter.text(
                rect.center(),
                Align2::CENTER_CENTER,
                emoji,
                FontId::proportional((rect.height() * 0.9).round()),
                ui.visuals().text_color(),
            );
        }
    }
    let radius = if rect.height() >= 20.0 { 3 } else { 2 };
    painter.rect_stroke(
        rect,
        CornerRadius::same(radius),
        Stroke::new(1.0, Color32::from_black_alpha(51)),
        StrokeKind::Inside,
    );
}

fn rasterize(country: &str, px: [u32; 2]) -> Option<ColorImage> {
    if let Some(image) = flag_svg(country).and_then(|svg| {
        egui_extras::image::load_svg_bytes_with_size(
            svg.as_bytes(),
            SizeHint::Size {
                width: px[0],
                height: px[1],
                maintain_aspect_ratio: true,
            },
            &Default::default(),
        )
        .ok()
    }) {
        return Some(image);
    }
    let image = draw_flag(country)?;
    let scale = (px[0] as f32 / image.width() as f32).min(px[1] as f32 / image.height() as f32);
    let width = ((image.width() as f32 * scale).round() as u32).max(1);
    let height = ((image.height() as f32 * scale).round() as u32).max(1);
    let image = image::imageops::resize(&image, width, height, image::imageops::FilterType::Triangle);
    Some(ColorImage::from_rgba_unmultiplied(
        [image.width() as usize, image.height() as usize],
        image.as_raw(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_the_original_sprite_for_a_country_without_stripes() {
        assert!(draw_flag("AD").is_none());
        let flag = rasterize("ad", [66, 48]).unwrap();
        assert_eq!(flag.width(), 64);
        assert_eq!(flag.height(), 48);
        assert!(flag.pixels.iter().any(|pixel| pixel.r() > pixel.b()));
        assert!(flag.pixels.iter().any(|pixel| pixel.b() > pixel.r()));
        assert!(rasterize("zz", [22, 16]).is_none());
    }

    #[test]
    fn striped_fallback_fits_the_box() {
        let flag = rasterize("de", [22, 16]).unwrap();
        assert!(flag.width() <= 22 && flag.height() <= 16);
    }
}
