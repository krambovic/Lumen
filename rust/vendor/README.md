# Vendored crates

`epaint` and `egui_glow` 0.37.0 are copied from crates.io and wired in through
`[patch.crates-io]` in `rust/Cargo.toml`. Both are dual-licensed MIT/Apache-2.0
(see their `Cargo.toml`). They carry one change: subpixel (ClearType-style)
text, so that Lumen's text and icon glyphs look like the Qt Quick client's on
low-DPI Windows screens. egui itself only renders grayscale text.

- `epaint/src/text/mod.rs`: `set_subpixel_text` / `subpixel_text`, a global
  switch that is off by default.
- `epaint/src/text/font_face.rs`: `rasterize_subpixel` fills glyph outlines
  at three times the horizontal resolution and applies FreeType's default
  five-tap LCD filter. Atlas pixels hold R, G and B coverage, and alpha holds
  their maximum.
- `egui_glow/src/painter.rs` and `src/shader/fragment_lcd.glsl`: on desktop
  OpenGL 3.3 or newer the painter links a GLSL 3.30 program with a second
  fragment output and blends with `ONE, ONE_MINUS_SRC1_COLOR` (dual-source
  blending), so each channel uses its own coverage. `u_lcd` is set only for
  meshes that use the font atlas; shapes sample its opaque white texel, where
  the result equals ordinary premultiplied blending. Partial coverage is
  raised to `LCD_DARK_EXPONENT` (0.85) for black text and down to
  `LCD_LIGHT_EXPONENT` (0.8) for white text; with linear coverage, dark text
  on the light settings page had 4-6% less ink than in QML. The lift follows the
  luminance of the premultiplied vertex colour, so translucent (muted) text
  is lifted less; with the straight colour it came out 7-9% heavier than in
  the QML client. Stroke weight now matches the QML client within about 2%
  on the onboarding and settings pages. If the program
  fails to compile, the painter keeps the stock shader and the switch stays
  off.

Text placement (`epaint/src/text/text_layout.rs`, `epaint/src/tessellator.rs`),
so labels sit where Qt Quick puts them:

- Glyphs are aligned against the unrounded row height. Rounding 18.6 px up to
  19 px first pushed every bottom-aligned glyph down before its baseline was
  rounded (14 px Segoe UI: baseline 16 instead of Qt's 15).
- A row made only of glyphs from one fallback face (an icon glyph drawn with
  the UI family) uses that face's own height, like a QML `Text` whose font is
  Segoe Fluent Icons; it used to be centred half a pixel high in a Segoe UI
  row.
- The galley's vertical position rounds exact half pixels down. A box
  centred in an odd height difference starts at n.5, which Qt's integer
  centring puts at n. Measured on the dashboard Connect button: label and
  icon now sit at the same offsets from the button centre as in Qt.

- `epaint/src/tessellator.rs`: colour glyph (emoji) vertices are marked with
  premultiplied white and alpha one lower, which tinted text never produces;
  the LCD shader blends those glyphs with their own colour.
- `epaint/src/text/mod.rs` `set_subpixel_grayscale` and `u_lcd == 2`: in a
  translucent window the shader averages the three coverages and writes a
  matching alpha, because the compositor adds the backdrop under one alpha.

Limitations: the stripe order is assumed to be RGB, which is the common case
and the ClearType default.

To upgrade egui, copy the new crate versions here and reapply the changes
marked `Lumen:`.
