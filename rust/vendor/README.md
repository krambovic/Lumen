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
  the result equals ordinary premultiplied blending. Light text gets a
  coverage exponent down to `LCD_LIGHT_EXPONENT` (0.8), which matched the
  QML client's stroke weight on the settings page within 3%. If the program
  fails to compile, the painter keeps the stock shader and the switch stays
  off.

Limitations: the stripe order is assumed to be RGB, which is the common case
and the ClearType default. Colour glyphs (emoji) in the font atlas are blended
with their colour as weights, which adds a faint halo.

To upgrade egui, copy the new crate versions here and reapply the changes
marked `Lumen:`.
