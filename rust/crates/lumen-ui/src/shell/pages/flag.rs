use egui::{
    Align2, Color32, ColorImage, CornerRadius, FontId, Id, Rect, Sense, Stroke, StrokeKind,
    TextureHandle, TextureOptions, Ui, Vec2, load::SizeHint, pos2,
};
use lumen_core::country_flags::{draw_flag, get_flag_emoji};

use crate::view_models::app_bridge::flag_svg;

/// The 20% black rounded border of the QML flag boxes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FlagBorder {
    /// The border is declared after an `Image { z: 1 }` (NodesPage row,
    /// `DashboardFlag`): the sprite covers it, so it only shows around the
    /// striped and emoji fallbacks.
    UnderSprite { radius: u8 },
    /// Declared after the image with no `z` (DashboardPage `RowFlag`): drawn
    /// over everything.
    Over { radius: u8 },
    /// No border (the onboarding country cards draw their own hairline).
    None,
}

/// QML `DashboardFlag`/`RowFlag`: the sprite is rasterized at the box's
/// physical pixel size (`sourceSize` + `smooth`) and fitted with its aspect
/// ratio kept.
pub(crate) fn show(ui: &mut Ui, country: &str, size: Vec2, border: FlagBorder) {
    if country.is_empty() {
        return;
    }
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    if ui.is_rect_visible(rect) {
        paint(ui, rect, country, border);
    }
}

fn paint(ui: &Ui, rect: Rect, country: &str, border: FlagBorder) {
    let country = country.to_ascii_lowercase();
    let ctx = ui.ctx();
    let ppp = ctx.pixels_per_point();
    let px = [
        (rect.width() * ppp).round().max(1.0) as u32,
        (rect.height() * ppp).round().max(1.0) as u32,
    ];
    let id = Id::unique(("country-flag", &country, px));
    let texture = match ctx.data_mut(|data| data.get_temp::<Option<(TextureHandle, bool)>>(id)) {
        Some(texture) => texture,
        None => {
            let texture = rasterize(&country, px).map(|(image, sprite)| {
                let handle = ctx.load_texture(
                    format!("flag-{country}-{}x{}", px[0], px[1]),
                    image,
                    TextureOptions::LINEAR,
                );
                (handle, sprite)
            });
            ctx.data_mut(|data| data.insert_temp(id, texture.clone()));
            texture
        }
    };
    let painter = ui.painter_at(rect);
    let sprite = texture.as_ref().is_some_and(|(_, sprite)| *sprite);
    match texture.map(|(texture, _)| texture) {
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
    let radius = match border {
        FlagBorder::UnderSprite { radius } if !sprite => radius,
        FlagBorder::Over { radius } => radius,
        _ => return,
    };
    painter.rect_stroke(
        rect,
        CornerRadius::same(radius),
        Stroke::new(1.0, Color32::from_black_alpha(51)),
        StrokeKind::Inside,
    );
}

/// resvg under-covers small shapes at flag sizes (the 50 stars of the US
/// canton came out much darker than QtSvg draws them), so the sprite is
/// rendered at this many times the box and box-averaged down: exact area
/// coverage, within about 1% of Qt's canton brightness at 24 x 18.
const SVG_SUPERSAMPLE: u32 = 4;

/// The flag image and whether it is the original sprite (`true`) or the
/// striped fallback.
fn rasterize(country: &str, px: [u32; 2]) -> Option<(ColorImage, bool)> {
    if let Some(image) = flag_svg(country).and_then(|svg| {
        egui_extras::image::load_svg_bytes_with_size(
            svg.as_bytes(),
            SizeHint::Size {
                width: px[0] * SVG_SUPERSAMPLE,
                height: px[1] * SVG_SUPERSAMPLE,
                maintain_aspect_ratio: true,
            },
            &Default::default(),
        )
        .ok()
    }) {
        return Some((box_downsample(&image, SVG_SUPERSAMPLE), true));
    }
    let image = draw_flag(country)?;
    let scale = (px[0] as f32 / image.width() as f32).min(px[1] as f32 / image.height() as f32);
    let width = ((image.width() as f32 * scale).round() as u32).max(1);
    let height = ((image.height() as f32 * scale).round() as u32).max(1);
    let image = image::imageops::resize(&image, width, height, image::imageops::FilterType::Triangle);
    Some((
        ColorImage::from_rgba_unmultiplied(
            [image.width() as usize, image.height() as usize],
            image.as_raw(),
        ),
        false,
    ))
}

/// Averages each `factor` x `factor` block of premultiplied pixels (blocks at
/// the right / bottom edge may be partial).
fn box_downsample(image: &ColorImage, factor: u32) -> ColorImage {
    let factor = factor.max(1) as usize;
    let [width, height] = image.size;
    let out_w = width.div_ceil(factor).max(1);
    let out_h = height.div_ceil(factor).max(1);
    let mut pixels = Vec::with_capacity(out_w * out_h);
    for y in 0..out_h {
        for x in 0..out_w {
            let mut sum = [0u32; 4];
            let mut count = 0u32;
            for sy in y * factor..((y + 1) * factor).min(height) {
                for sx in x * factor..((x + 1) * factor).min(width) {
                    let pixel = image.pixels[sy * width + sx].to_array();
                    for (total, channel) in sum.iter_mut().zip(pixel) {
                        *total += u32::from(channel);
                    }
                    count += 1;
                }
            }
            let [r, g, b, a] = sum.map(|total| ((total + count / 2) / count.max(1)) as u8);
            pixels.push(Color32::from_rgba_premultiplied(r, g, b, a));
        }
    }
    ColorImage::new([out_w, out_h], pixels)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supersampled_sprite_keeps_the_box_size_and_brightens_small_shapes() {
        let (flag, _) = rasterize("us", [24, 18]).unwrap();
        assert_eq!(flag.size, [24, 18]);
        // The canton (stars on blue) averages near QtSvg's ~0.33 luminance
        // instead of the 0.27 a single-sample render gave.
        let canton: Vec<f32> = (0..9)
            .flat_map(|y| (0..9).map(move |x| (x, y)))
            .map(|(x, y)| {
                let [r, g, b, _] = flag.pixels[y * 24 + x].to_array();
                (f32::from(r) + f32::from(g) + f32::from(b)) / (3.0 * 255.0)
            })
            .collect();
        let mean = canton.iter().sum::<f32>() / canton.len() as f32;
        assert!(mean > 0.3, "canton mean {mean}");
    }

    #[test]
    fn renders_the_original_sprite_for_a_country_without_stripes() {
        assert!(draw_flag("AD").is_none());
        let (flag, sprite) = rasterize("ad", [66, 48]).unwrap();
        assert!(sprite);
        assert_eq!(flag.width(), 64);
        assert_eq!(flag.height(), 48);
        assert!(flag.pixels.iter().any(|pixel| pixel.r() > pixel.b()));
        assert!(flag.pixels.iter().any(|pixel| pixel.b() > pixel.r()));
        assert!(rasterize("zz", [22, 16]).is_none());
    }

    #[test]
    fn striped_fallback_fits_the_box() {
        let (flag, _) = rasterize("de", [22, 16]).unwrap();
        assert!(flag.width() <= 22 && flag.height() <= 16);
    }
}
