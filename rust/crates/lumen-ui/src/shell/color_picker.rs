//! `ColorPickerDialog.qml`: a 340 px modal popup with 24 swatches, a
//! saturation / value square, a hue slider, an optional "darkness" (mute)
//! slider, a preview with a hex field and RGB readout, an optional system
//! accent button and Сбросить / Отмена / OK.
//!
//! Colours follow the QML maths: the picked colour is
//! `hsva(h, s * (1 - 0.80 * mute), v * (1 - 0.55 * mute))`. The gradients are
//! rendered into small textures per pixel, so they blend exactly like Qt's
//! stacked gradient rectangles and keep anti-aliased rounded corners.

use egui::{
    Align, Align2, Color32, ColorImage, Context, CursorIcon, FontId, Frame, Id, Margin, Modal,
    Pos2, Rect, Sense, Stroke, StrokeKind, TextEdit, TextureHandle, TextureOptions, Ui, pos2,
    vec2,
};
use lumen_core::i18n::tr;

use super::theme::{Theme, hex_color, semibold};
use super::widgets::{ButtonKind, FluentButton};

const SWATCHES: [&str; 24] = [
    "#0078D4", "#0099BC", "#00B294", "#107C10", "#498205", "#7A7574", "#515C6B", "#4C4A48",
    "#6B69D6", "#8764B8", "#744DA9", "#B146C2", "#E3008C", "#C30052", "#E81123", "#DA3B01",
    "#F7630C", "#FF8C00", "#FFB900", "#038387", "#567C73", "#847545", "#9D5D00", "#525E54",
];

const WIDTH: f32 = 340.0;
const PADDING: f32 = 20.0;

/// What the dialog did this frame.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum PickerResult {
    Open,
    Closed,
    /// `accepted(hex)`: the picked colour, plus the unmuted base colour and
    /// the mute level (`_baseHex()` and `_mute`, used by the base tint).
    Accepted { hex: String, base_hex: String, mute: f32 },
}

struct GradientTexture {
    key: u64,
    handle: TextureHandle,
}

pub(crate) struct ColorPicker {
    id: Id,
    heading: String,
    show_darkness: bool,
    show_system: bool,
    default_color: &'static str,
    h: f32,
    s: f32,
    v: f32,
    mute: f32,
    hex: String,
    hex_focused: bool,
    sv_texture: Option<GradientTexture>,
    hue_texture: Option<GradientTexture>,
    dark_texture: Option<GradientTexture>,
}

impl ColorPicker {
    /// `openWith(color, mute)`.
    pub(crate) fn open(
        id: &str,
        heading: impl Into<String>,
        color: &str,
        mute: f32,
        show_darkness: bool,
        show_system: bool,
    ) -> Self {
        let mut picker = Self {
            id: Id::unique(("lumen-color-picker", id)),
            heading: heading.into(),
            show_darkness,
            show_system,
            default_color: "#0078D4",
            h: 0.0,
            s: 0.0,
            v: 0.0,
            mute: mute.clamp(0.0, 1.0),
            hex: String::new(),
            hex_focused: false,
            sv_texture: None,
            hue_texture: None,
            dark_texture: None,
        };
        picker.load(color);
        picker.hex = picker.current_hex();
        picker
    }

    /// `_loadHsv`: an achromatic colour keeps the previous hue.
    fn load(&mut self, hex: &str) {
        let color = hex_color(hex).unwrap_or(Color32::from_rgb(0, 120, 212));
        let (hue, s, v) = rgb_to_hsv(color.r(), color.g(), color.b());
        if let Some(hue) = hue {
            self.h = hue;
        }
        self.s = s;
        self.v = v;
    }

    fn current(&self) -> [f32; 3] {
        hsv_to_rgb(self.h, self.s * (1.0 - 0.80 * self.mute), self.v * (1.0 - 0.55 * self.mute))
    }

    fn current_hex(&self) -> String {
        rgb_hex(self.current())
    }

    fn base_hex(&self) -> String {
        rgb_hex(hsv_to_rgb(self.h, self.s, self.v))
    }

    pub(crate) fn show(
        &mut self,
        ctx: &Context,
        theme: &Theme,
        system_accent: impl FnOnce() -> String,
    ) -> PickerResult {
        // Universal `Overlay.modal`: baseLowColor (20 % of the base colour).
        let backdrop = if theme.dark {
            Color32::from_white_alpha(0x33)
        } else {
            Color32::from_black_alpha(0x33)
        };
        let mut result = PickerResult::Open;
        let mut system_accent = Some(system_accent);
        let modal = Modal::new(self.id)
            .backdrop_color(backdrop)
            .frame(
                Frame::new()
                    .fill(theme.flyout)
                    .stroke(Stroke::new(1.0, theme.flyout_border))
                    .corner_radius(theme.radius as u8)
                    .inner_margin(Margin::same((PADDING - 1.0) as i8)),
            )
            .show(ctx, |ui| {
                let width = WIDTH - 2.0 * PADDING;
                ui.set_width(width);
                ui.spacing_mut().item_spacing = vec2(0.0, 14.0);
                self.heading(ui, theme);
                self.swatches(ui, theme, width);
                self.sv_square(ui, width);
                self.hue_slider(ui, width);
                if self.show_darkness {
                    self.dark_slider(ui, width);
                }
                self.preview(ui, theme, width);
                if self.show_system
                    && FluentButton::new(ButtonKind::Ghost, "", &tr("Системный", &[]))
                        .show(ui, theme)
                        .clicked()
                    && let Some(system_accent) = system_accent.take()
                {
                    self.load(&system_accent());
                    self.hex = self.current_hex();
                }
                // RowLayout spacing 8: Сбросить | fill | Отмена, OK.
                let (row, _) = ui.allocate_exact_size(vec2(width, theme.control_height as f32), Sense::hover());
                let mut left = ui.new_child(
                    egui::UiBuilder::new().max_rect(row).layout(egui::Layout::left_to_right(Align::Center)),
                );
                if FluentButton::new(ButtonKind::Ghost, "", &tr("Сбросить", &[]))
                    .show(&mut left, theme)
                    .clicked()
                {
                    self.load(self.default_color);
                    self.hex = self.current_hex();
                }
                let mut right = ui.new_child(
                    egui::UiBuilder::new().max_rect(row).layout(egui::Layout::right_to_left(Align::Center)),
                );
                right.spacing_mut().item_spacing.x = 8.0;
                if FluentButton::new(ButtonKind::Accent, "", "OK").show(&mut right, theme).clicked() {
                    result = PickerResult::Accepted {
                        hex: self.current_hex(),
                        base_hex: self.base_hex(),
                        mute: self.mute,
                    };
                }
                if FluentButton::new(ButtonKind::Ghost, "", &tr("Отмена", &[]))
                    .show(&mut right, theme)
                    .clicked()
                {
                    result = PickerResult::Closed;
                }
            });
        if result == PickerResult::Open && modal.should_close() {
            result = PickerResult::Closed;
        }
        result
    }

    fn heading(&self, ui: &mut Ui, theme: &Theme) {
        let galley = ui.painter().layout_no_wrap(
            self.heading.clone(),
            semibold(theme.font_strong as f32),
            theme.text,
        );
        let (rect, _) = ui.allocate_exact_size(galley.size(), Sense::hover());
        ui.painter().galley(rect.min, galley, theme.text);
    }

    /// GridLayout: 8 columns, 8 px spacing, square cells, radius 6.
    fn swatches(&mut self, ui: &mut Ui, theme: &Theme, width: f32) {
        let cell = (width - 7.0 * 8.0) / 8.0;
        let rows = SWATCHES.len().div_ceil(8);
        let height = rows as f32 * cell + (rows - 1) as f32 * 8.0;
        let (area, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
        let current = self.current_hex();
        for (index, hex) in SWATCHES.iter().enumerate() {
            let column = (index % 8) as f32;
            let row = (index / 8) as f32;
            let rect = Rect::from_min_size(
                area.min + vec2(column * (cell + 8.0), row * (cell + 8.0)),
                vec2(cell, cell),
            );
            let response = ui
                .interact(rect, self.id.with(("swatch", index)), Sense::click())
                .on_hover_cursor(CursorIcon::PointingHand);
            let selected = current == *hex;
            let fill = hex_color(hex).unwrap_or_default();
            let stroke = if selected {
                Stroke::new(2.0, theme.text)
            } else {
                Stroke::new(1.0, theme.divider)
            };
            ui.painter().rect(rect, 6.0, fill, stroke, StrokeKind::Inside);
            if response.clicked() {
                // `_loadHsv(modelData)`: the mute level stays.
                self.load(hex);
                self.hex = self.current_hex();
            }
        }
    }

    fn sv_square(&mut self, ui: &mut Ui, width: f32) {
        let (rect, response) = ui.allocate_exact_size(vec2(width, 180.0), Sense::click_and_drag());
        let ppp = ui.ctx().pixels_per_point();
        let hue = hsv_to_rgb(self.h, 1.0, 1.0);
        let key = hash_key(&[self.h.to_bits(), rect.width().to_bits(), ppp.to_bits()]);
        let texture = gradient_texture(ui.ctx(), &mut self.sv_texture, "color-picker-sv", key, rect.size(), ppp, 6.0, |x, y| {
            // Hue fill, white -> transparent left to right, transparent ->
            // black top to bottom (each layer composited in sRGB).
            let mixed = [0, 1, 2].map(|c| (hue[c] * x + (1.0 - x)) * (1.0 - y));
            mixed
        });
        ui.painter().image(texture, rect, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
        if (response.is_pointer_button_down_on() || response.dragged())
            && let Some(pointer) = response.interact_pointer_pos()
        {
            self.s = ((pointer.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
            self.v = (1.0 - (pointer.y - rect.top()) / rect.height()).clamp(0.0, 1.0);
            self.sync_hex();
        }
        // An 18 px ring with a 3 px #80000000 border around a 14 px ring with
        // a 2 px white border. Qt borders are drawn inside the rectangle and
        // egui strokes a circle outside its radius, hence radius - width.
        let center = pos2(rect.left() + self.s * rect.width(), rect.top() + (1.0 - self.v) * rect.height());
        let painter = ui.painter().with_clip_rect(ui.clip_rect());
        painter.circle_stroke(center, 6.0, Stroke::new(3.0, Color32::from_black_alpha(0x80)));
        painter.circle_stroke(center, 5.0, Stroke::new(2.0, Color32::WHITE));
    }

    fn hue_slider(&mut self, ui: &mut Ui, width: f32) {
        let (rect, response) = ui.allocate_exact_size(vec2(width, 16.0), Sense::click_and_drag());
        let ppp = ui.ctx().pixels_per_point();
        let key = hash_key(&[rect.width().to_bits(), ppp.to_bits()]);
        let texture = gradient_texture(ui.ctx(), &mut self.hue_texture, "color-picker-hue", key, rect.size(), ppp, 4.0, |x, _| {
            hsv_to_rgb(x, 1.0, 1.0)
        });
        ui.painter().image(texture, rect, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
        if let Some(x) = drag_fraction(&response, rect) {
            self.h = x;
            self.sync_hex();
        }
        slider_handle(ui, rect, self.h);
    }

    fn dark_slider(&mut self, ui: &mut Ui, width: f32) {
        let (rect, response) = ui.allocate_exact_size(vec2(width, 16.0), Sense::click_and_drag());
        let ppp = ui.ctx().pixels_per_point();
        let from = hsv_to_rgb(self.h, self.s, self.v);
        let to = hsv_to_rgb(self.h, self.s * 0.20, self.v * 0.45);
        let key = hash_key(&[
            self.h.to_bits(),
            self.s.to_bits(),
            self.v.to_bits(),
            rect.width().to_bits(),
            ppp.to_bits(),
        ]);
        let texture = gradient_texture(ui.ctx(), &mut self.dark_texture, "color-picker-dark", key, rect.size(), ppp, 4.0, |x, _| {
            [0, 1, 2].map(|c| from[c] + (to[c] - from[c]) * x)
        });
        ui.painter().image(texture, rect, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
        if let Some(x) = drag_fraction(&response, rect) {
            self.mute = x;
            self.sync_hex();
        }
        slider_handle(ui, rect, self.mute);
    }

    /// `onCurrentChanged: if (!hexField.activeFocus) hexField.text = _hex(current)`.
    fn sync_hex(&mut self) {
        if !self.hex_focused {
            self.hex = self.current_hex();
        }
    }

    /// Preview square (46, radius 6) | hex field over the RGB readout.
    fn preview(&mut self, ui: &mut Ui, theme: &Theme, width: f32) {
        let strong = FontId::proportional(theme.font_strong as f32);
        let small = FontId::proportional(theme.font_small as f32);
        let field_height = ui.fonts_mut(|fonts| fonts.row_height(&strong)) + 14.0;
        let small_height = ui.fonts_mut(|fonts| fonts.row_height(&small));
        let column_height = field_height + 6.0 + small_height;
        let height = column_height.max(46.0);
        let (row, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
        let current = self.current();
        let swatch = Rect::from_min_size(pos2(row.left(), row.center().y - 23.0), vec2(46.0, 46.0));
        ui.painter().rect(swatch, 6.0, rgb_color(current), Stroke::new(1.0, theme.divider), StrokeKind::Inside);

        let column_top = row.center().y - column_height / 2.0;
        let field = Rect::from_min_size(
            pos2(swatch.right() + 12.0, column_top),
            vec2(row.right() - swatch.right() - 12.0, field_height),
        );
        let background = ui.painter().add(egui::Shape::Noop);
        let response = ui.place(
            field.shrink2(vec2(10.0, 0.0)),
            TextEdit::singleline(&mut self.hex)
                .id(self.id.with("hex"))
                .font(strong)
                .text_color(theme.text)
                .frame(Frame::NONE)
                .margin(Margin::ZERO)
                .vertical_align(Align::Center),
        );
        self.hex_focused = response.has_focus();
        // `onEditingFinished: _fromHex(text)`: valid hex resets the mute.
        let finished = response.lost_focus()
            || (response.has_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter)));
        if finished {
            let text = self.hex.trim().trim_start_matches('#').to_owned();
            if text.len() == 6 && text.chars().all(|c| c.is_ascii_hexdigit()) {
                self.load(&format!("#{text}"));
                self.mute = 0.0;
            }
            if !response.has_focus() {
                self.hex = self.current_hex();
            }
        }
        let fill = if theme.dark {
            Color32::from_white_alpha(13)
        } else {
            Color32::from_black_alpha(10)
        };
        let border = if response.has_focus() { theme.accent } else { theme.divider };
        ui.painter().set(
            background,
            egui::Shape::rect_filled(field, theme.radius as f32, fill),
        );
        ui.painter().rect_stroke(field, theme.radius as f32, Stroke::new(1.0, border), StrokeKind::Inside);

        let rgb = format!(
            "R {}    G {}    B {}",
            channel(current[0]),
            channel(current[1]),
            channel(current[2])
        );
        ui.painter().text(
            pos2(field.left(), field.bottom() + 6.0),
            Align2::LEFT_TOP,
            rgb,
            small,
            theme.text_faint,
        );
    }
}

/// White 8 x 24 handle (radius 4, 1 px #80000000 border) centred on `x`.
fn slider_handle(ui: &Ui, rect: Rect, fraction: f32) {
    let x = rect.left() + fraction * rect.width();
    let handle = Rect::from_min_size(pos2(x - 4.0, rect.top() - 4.0), vec2(8.0, rect.height() + 8.0));
    ui.painter().rect(
        handle,
        4.0,
        Color32::WHITE,
        Stroke::new(1.0, Color32::from_black_alpha(0x80)),
        StrokeKind::Inside,
    );
}

fn drag_fraction(response: &egui::Response, rect: Rect) -> Option<f32> {
    if !(response.is_pointer_button_down_on() || response.dragged()) {
        return None;
    }
    let pointer: Pos2 = response.interact_pointer_pos()?;
    Some(((pointer.x - rect.left()) / rect.width()).clamp(0.0, 1.0))
}

fn hash_key(parts: &[u32]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    parts.hash(&mut hasher);
    hasher.finish()
}

/// Renders `color(x, y)` (sRGB, 0..1, at pixel centres) into a texture the
/// size of `size` points with anti-aliased corners of `radius` points.
#[allow(clippy::too_many_arguments)]
fn gradient_texture(
    ctx: &Context,
    slot: &mut Option<GradientTexture>,
    name: &str,
    key: u64,
    size: egui::Vec2,
    ppp: f32,
    radius: f32,
    color: impl Fn(f32, f32) -> [f32; 3],
) -> egui::TextureId {
    if let Some(texture) = slot.as_ref().filter(|texture| texture.key == key) {
        return texture.handle.id();
    }
    let width = (size.x * ppp).round().max(1.0) as usize;
    let height = (size.y * ppp).round().max(1.0) as usize;
    let radius = radius * ppp;
    let mut pixels = Vec::with_capacity(width * height);
    for row in 0..height {
        for column in 0..width {
            let px = column as f32 + 0.5;
            let py = row as f32 + 0.5;
            let rgb = color(px / width as f32, py / height as f32);
            let alpha = rounded_coverage(px, py, width as f32, height as f32, radius);
            pixels.push(Color32::from_rgba_unmultiplied(
                channel(rgb[0]),
                channel(rgb[1]),
                channel(rgb[2]),
                (alpha * 255.0).round() as u8,
            ));
        }
    }
    let image = ColorImage::new([width, height], pixels);
    let handle = ctx.load_texture(name, image, TextureOptions::LINEAR);
    let id = handle.id();
    *slot = Some(GradientTexture { key, handle });
    id
}

/// Coverage of pixel centre (x, y) by a `w` x `h` rectangle with corner
/// `radius`, with a one pixel anti-aliased edge.
fn rounded_coverage(x: f32, y: f32, w: f32, h: f32, radius: f32) -> f32 {
    let dx = (x - w / 2.0).abs() - (w / 2.0 - radius);
    let dy = (y - h / 2.0).abs() - (h / 2.0 - radius);
    let outside = vec2(dx.max(0.0), dy.max(0.0)).length();
    let distance = outside + dx.max(dy).min(0.0) - radius;
    (0.5 - distance).clamp(0.0, 1.0)
}

fn channel(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

fn rgb_color(rgb: [f32; 3]) -> Color32 {
    Color32::from_rgb(channel(rgb[0]), channel(rgb[1]), channel(rgb[2]))
}

fn rgb_hex(rgb: [f32; 3]) -> String {
    format!("#{:02X}{:02X}{:02X}", channel(rgb[0]), channel(rgb[1]), channel(rgb[2]))
}

/// `Qt.hsva(h, s, v, 1)` with every component in 0..1.
fn hsv_to_rgb(h: f32, s: f32, v: f32) -> [f32; 3] {
    let h = (h.clamp(0.0, 1.0) * 6.0) % 6.0;
    let sector = h.floor();
    let f = h - sector;
    let p = v * (1.0 - s);
    let q = v * (1.0 - s * f);
    let t = v * (1.0 - s * (1.0 - f));
    match sector as u8 {
        0 => [v, t, p],
        1 => [q, v, p],
        2 => [p, v, t],
        3 => [p, q, v],
        4 => [t, p, v],
        _ => [v, p, q],
    }
}

/// `hsvHue` (None for grey, as Qt's -1), `hsvSaturation`, `hsvValue`.
fn rgb_to_hsv(r: u8, g: u8, b: u8) -> (Option<f32>, f32, f32) {
    let [r, g, b] = [r, g, b].map(|c| f32::from(c) / 255.0);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;
    let s = if max > 0.0 { delta / max } else { 0.0 };
    if delta <= 0.0 {
        return (None, s, max);
    }
    let hue = if max == r {
        ((g - b) / delta).rem_euclid(6.0)
    } else if max == g {
        (b - r) / delta + 2.0
    } else {
        (r - g) / delta + 4.0
    };
    (Some(hue / 6.0), s, max)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_round_trips_through_hsv() {
        for hex in SWATCHES {
            let picker = ColorPicker::open(hex, "", hex, 0.0, false, false);
            assert_eq!(picker.current_hex(), hex);
        }
    }

    #[test]
    fn mute_darkens_like_qml() {
        // hsva(h, s * 0.2, v * 0.45) at full mute.
        let picker = ColorPicker::open("m", "", "#FF0000", 1.0, true, false);
        assert_eq!(picker.base_hex(), "#FF0000");
        let expected = rgb_hex(hsv_to_rgb(0.0, 0.2, 0.45));
        assert_eq!(picker.current_hex(), expected);
    }

    #[test]
    fn grey_keeps_the_previous_hue() {
        let mut picker = ColorPicker::open("g", "", "#00FF00", 0.0, false, false);
        let hue = picker.h;
        picker.load("#808080");
        assert_eq!(picker.h, hue);
        assert_eq!(picker.s, 0.0);
    }
}
