//! Blurred backdrop of `OnboardingOverlay.qml`: `Theme.bg`, the heavily
//! blurred and slightly desaturated live UI (`MultiEffect`, blurMax 128 x
//! 1.6, saturation -0.25), and a `Theme.bg` tint on top.
//!
//! egui has no render-to-texture pass, so a paint callback copies what is
//! already in the framebuffer when the overlay is reached (everything below
//! it), builds its mipmaps and draws a small Gaussian over a coarse mip
//! level. It needs desktop OpenGL 3.3; when the program cannot be built the
//! caller paints a flat `Theme.bg` instead.

use std::sync::Arc;

use eframe::glow::{self, HasContext as _};
use egui::{Color32, Rect, Shape};
use parking_lot::Mutex;

/// Mip level the Gaussian samples: 64 px texels, about a 200 px blur.
const LOD: f32 = 6.0;

const VERTEX: &str = r"#version 330
void main() {
    vec2 corner = vec2(float((gl_VertexID << 1) & 2), float(gl_VertexID & 2));
    gl_Position = vec4(corner * 2.0 - 1.0, 0.0, 1.0);
}
";

const FRAGMENT: &str = r"#version 330
uniform sampler2D u_frame;
uniform vec2 u_screen;
uniform vec3 u_base;
uniform float u_tint;
uniform float u_opacity;
uniform float u_lod;
out vec4 f_color;

void main() {
    vec2 uv = gl_FragCoord.xy / u_screen;
    vec2 texel = exp2(u_lod) / u_screen;
    float weights[5] = float[5](1.0, 4.0, 6.0, 4.0, 1.0);
    vec4 sum = vec4(0.0);
    for (int y = 0; y < 5; y++) {
        for (int x = 0; x < 5; x++) {
            vec2 offset = vec2(float(x - 2), float(y - 2)) * texel;
            sum += textureLod(u_frame, uv + offset, u_lod) * (weights[x] * weights[y]);
        }
    }
    // The window is translucent: put the premultiplied copy on the opaque base.
    vec4 frame = sum / 256.0;
    vec3 color = frame.rgb + u_base * (1.0 - frame.a);
    float luma = dot(color, vec3(0.2126, 0.7152, 0.0722));
    color = mix(vec3(luma), color, 0.75);
    color = mix(color, u_base, u_tint);
    f_color = vec4(color * u_opacity, u_opacity);
}
";

struct Program {
    program: glow::Program,
    vertex_array: glow::VertexArray,
    texture: glow::Texture,
    size: [i32; 2],
    screen: glow::UniformLocation,
    base: glow::UniformLocation,
    tint: glow::UniformLocation,
    opacity: glow::UniformLocation,
    lod: glow::UniformLocation,
}

enum State {
    Untried,
    Ready(Program),
    Unsupported,
}

#[derive(Clone)]
pub(crate) struct BackdropBlur {
    state: Arc<Mutex<State>>,
}

impl Default for BackdropBlur {
    fn default() -> Self {
        Self { state: Arc::new(Mutex::new(State::Untried)) }
    }
}

impl BackdropBlur {
    /// Whether the GL program could not be built, so the caller must paint a
    /// flat backdrop. Unknown until the first callback has run.
    pub(crate) fn unsupported(&self) -> bool {
        matches!(*self.state.lock(), State::Unsupported)
    }

    /// The backdrop over `rect`: `base` under the blurred frame, mixed
    /// `tint` of the way back to `base`, at `opacity`.
    pub(crate) fn shape(&self, rect: Rect, base: Color32, tint: f32, opacity: f32) -> Shape {
        let state = self.state.clone();
        let base = [base.r(), base.g(), base.b()].map(|channel| f32::from(channel) / 255.0);
        let callback = eframe::egui_glow::CallbackFn::new(move |info, painter| {
            let mut state = state.lock();
            if matches!(*state, State::Untried) {
                *state = match unsafe { Program::new(painter.gl()) } {
                    Some(program) => State::Ready(program),
                    None => State::Unsupported,
                };
            }
            if let State::Ready(program) = &mut *state {
                let [width, height] = info.screen_size_px.map(|side| side as i32);
                unsafe { program.paint(painter.gl(), [width, height], base, tint, opacity) };
            }
        });
        Shape::Callback(egui::PaintCallback { rect, callback: Arc::new(callback) })
    }
}

impl Program {
    unsafe fn new(gl: &glow::Context) -> Option<Self> {
        let version = gl.version();
        if version.is_embedded || (version.major, version.minor) < (3, 3) {
            return None;
        }
        unsafe {
            let compile = |kind: u32, source: &str| {
                let shader = gl.create_shader(kind).ok()?;
                gl.shader_source(shader, source);
                gl.compile_shader(shader);
                if gl.get_shader_compile_status(shader) {
                    Some(shader)
                } else {
                    gl.delete_shader(shader);
                    None
                }
            };
            let vertex = compile(glow::VERTEX_SHADER, VERTEX)?;
            let Some(fragment) = compile(glow::FRAGMENT_SHADER, FRAGMENT) else {
                gl.delete_shader(vertex);
                return None;
            };
            let program = gl.create_program().ok()?;
            gl.attach_shader(program, vertex);
            gl.attach_shader(program, fragment);
            gl.link_program(program);
            gl.delete_shader(vertex);
            gl.delete_shader(fragment);
            if !gl.get_program_link_status(program) {
                gl.delete_program(program);
                return None;
            }
            let uniform = |name: &str| gl.get_uniform_location(program, name);
            let (Some(screen), Some(base), Some(tint), Some(opacity), Some(lod)) = (
                uniform("u_screen"),
                uniform("u_base"),
                uniform("u_tint"),
                uniform("u_opacity"),
                uniform("u_lod"),
            ) else {
                gl.delete_program(program);
                return None;
            };
            let vertex_array = gl.create_vertex_array().ok()?;
            let texture = gl.create_texture().ok()?;
            gl.bind_texture(glow::TEXTURE_2D, Some(texture));
            for (parameter, value) in [
                (glow::TEXTURE_MIN_FILTER, glow::LINEAR_MIPMAP_LINEAR),
                (glow::TEXTURE_MAG_FILTER, glow::LINEAR),
                (glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE),
                (glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE),
            ] {
                gl.tex_parameter_i32(glow::TEXTURE_2D, parameter, value as i32);
            }
            Some(Self { program, vertex_array, texture, size: [0, 0], screen, base, tint, opacity, lod })
        }
    }

    /// The painter restores its own program, blending and vertex array after
    /// a callback (`prepare_painting`), and binds a texture for every mesh.
    unsafe fn paint(&mut self, gl: &glow::Context, size: [i32; 2], base: [f32; 3], tint: f32, opacity: f32) {
        let [width, height] = size;
        if width <= 0 || height <= 0 {
            return;
        }
        unsafe {
            gl.bind_texture(glow::TEXTURE_2D, Some(self.texture));
            if self.size != size {
                gl.tex_image_2d(
                    glow::TEXTURE_2D,
                    0,
                    glow::RGBA8 as i32,
                    width,
                    height,
                    0,
                    glow::RGBA,
                    glow::UNSIGNED_BYTE,
                    glow::PixelUnpackData::Slice(None),
                );
                self.size = size;
            }
            gl.copy_tex_sub_image_2d(glow::TEXTURE_2D, 0, 0, 0, 0, 0, width, height);
            gl.generate_mipmap(glow::TEXTURE_2D);

            gl.use_program(Some(self.program));
            gl.uniform_2_f32(Some(&self.screen), width as f32, height as f32);
            gl.uniform_3_f32(Some(&self.base), base[0], base[1], base[2]);
            gl.uniform_1_f32(Some(&self.tint), tint);
            gl.uniform_1_f32(Some(&self.opacity), opacity);
            gl.uniform_1_f32(Some(&self.lod), LOD);
            // One output, premultiplied; the painter's own blend function
            // expects the second (subpixel) output.
            gl.blend_func_separate(
                glow::ONE,
                glow::ONE_MINUS_SRC_ALPHA,
                glow::ONE_MINUS_DST_ALPHA,
                glow::ONE,
            );
            gl.bind_vertex_array(Some(self.vertex_array));
            gl.draw_arrays(glow::TRIANGLES, 0, 3);
        }
    }
}
