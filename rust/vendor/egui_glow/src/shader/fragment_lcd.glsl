// Lumen: fragment shader with per-channel (subpixel) coverage for text.
//
// The font atlas holds RGB coverage for glyphs rasterized at three times the
// horizontal resolution (see epaint `text::subpixel_text`). Each channel is
// blended with its own coverage through dual-source blending:
//   dst.rgb = f_color.rgb + dst.rgb * (1 - f_blend.rgb)
// Shapes sample the opaque white texel of the atlas, where this reduces to
// ordinary premultiplied alpha blending, and every other texture uses the
// ordinary path (u_lcd == 0).

uniform sampler2D u_sampler;
uniform int u_lcd;

#ifndef LCD_LIGHT_EXPONENT
#define LCD_LIGHT_EXPONENT 0.8
#endif
#ifndef LCD_DARK_EXPONENT
#define LCD_DARK_EXPONENT 0.85
#endif

in vec4 v_rgba_in_gamma;
in vec2 v_tc;
layout(location = 0, index = 0) out vec4 f_color;
layout(location = 0, index = 1) out vec4 f_blend;

void main() {
    vec4 tex = texture(u_sampler, v_tc);
    vec4 color = v_rgba_in_gamma;
    if (u_lcd == 1) {
        vec3 coverage = tex.rgb;
        // DirectWrite raises text contrast, so linear coverage reads thinner
        // than the QML client: lift partial coverage, by LCD_DARK_EXPONENT
        // for dark text (4-6% less ink than QML on the light settings page
        // without it) up to LCD_LIGHT_EXPONENT for white text. Full
        // coverage (shapes, glyph cores) is unchanged.
        // The vertex colour is premultiplied, so translucent (muted) text
        // gets a smaller lift than opaque text of the same hue, as with
        // DirectWrite's contrast for the blended colour.
        float luminance = dot(color.rgb, vec3(0.299, 0.587, 0.114));
        coverage = pow(coverage, vec3(mix(LCD_DARK_EXPONENT, LCD_LIGHT_EXPONENT, luminance)));
        f_color = vec4(color.rgb * coverage, color.a * tex.a);
        f_blend = vec4(color.a * coverage, color.a * tex.a);
    } else {
        f_color = color * tex;
        f_blend = vec4(f_color.a);
    }
}
