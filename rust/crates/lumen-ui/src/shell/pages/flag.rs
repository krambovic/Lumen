use egui::{
    ColorImage, Id, Image, TextureHandle, TextureOptions, Ui, Vec2,
    load::{SizeHint, SizedTexture},
};
use lumen_core::country_flags::{draw_flag, get_flag_emoji};

use crate::view_models::app_bridge::flag_svg;

pub(crate) fn show(ui: &mut Ui, country: &str, size: Vec2) {
    if country.is_empty() {
        return;
    }
    let country = country.to_ascii_lowercase();
    let id = Id::unique(("country-flag", &country));
    let cached = ui.ctx().data_mut(|data| data.get_temp::<TextureHandle>(id));
    let texture = cached.or_else(|| {
        let image = rasterize(&country)?;
        let texture = ui
            .ctx()
            .load_texture(&country, image, TextureOptions::LINEAR);
        ui.ctx()
            .data_mut(|data| data.insert_temp(id, texture.clone()));
        Some(texture)
    });
    if let Some(texture) = texture {
        ui.add(Image::from_texture(SizedTexture::from_handle(&texture)).fit_to_exact_size(size));
    } else {
        ui.label(get_flag_emoji(&country));
    }
}

fn rasterize(country: &str) -> Option<ColorImage> {
    if let Some(image) = flag_svg(country).and_then(|svg| {
        egui_extras::image::load_svg_bytes_with_size(
            svg.as_bytes(),
            SizeHint::Width(96),
            &Default::default(),
        )
        .ok()
    }) {
        return Some(image);
    }
    let image = draw_flag(country)?;
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
        let flag = rasterize("AD").unwrap();
        assert_eq!(flag.width(), 96);
        assert!(flag.pixels.iter().any(|pixel| pixel.r() > pixel.b()));
        assert!(flag.pixels.iter().any(|pixel| pixel.b() > pixel.r()));
        assert!(rasterize("zz").is_none());
    }
}
