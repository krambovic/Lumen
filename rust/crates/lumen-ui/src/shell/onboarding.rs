//! First-run setup and upgrade notes from `OnboardingOverlay.qml`.
//!
//! A modal "window inside the window": a scrim below the title bar with a
//! centred card, a 272pt side panel with the step stepper (wide layout) or
//! compact progress pills (narrow), and the per-step pages above a 76pt
//! action bar. Every grid and flow is laid out with explicit rectangles.

#[allow(unused_imports)]
use crate::shell::widgets::FluentTip;
use std::path::Path;
use std::sync::Arc;

use egui::{
    Align, Color32, Context, CursorIcon, FontId, Galley, Id, Key, Layout, Modifiers, Painter, Pos2,
    Rect, ScrollArea, Sense, Shape, Stroke, StrokeKind, TextEdit, Ui, Vec2, pos2, text::LayoutJob,
    vec2,
};
use lumen_core::{constants::APP_VERSION, i18n::tr};

use super::backdrop_blur::BackdropBlur;
use super::motion;
use super::pages::about::{DONATION_URL, USDT_ADDRESS, open_url};
use super::pages::flag;
use super::theme::{
    ACCENT_SWATCHES, Theme, dark_preset_map, hex_color, light_preset_map, semibold,
};
use super::widgets::{ButtonKind, FluentButton, card_frame, elided};
use super::{TITLE_BAR_HEIGHT, has_supported_config_drop, win_native};
use crate::view_models::AppBridge;

const PALETTES: [(&str, &str); 12] = [
    ("default", "Default"),
    ("midnight", "Midnight"),
    ("catppuccin", "Catppuccin"),
    ("dracula", "Dracula"),
    ("nord", "Nord"),
    ("tokyo-night", "Tokyo Night"),
    ("github", "GitHub"),
    ("rose-pine", "Rose Pine"),
    ("gruvbox", "Gruvbox"),
    ("everforest", "Everforest"),
    ("solarized", "Solarized"),
    ("vercel", "Vercel"),
];

/// QML `paletteAccents`: the accent pill drawn inside each swatch.
const PALETTE_ACCENTS: [(&str, &str); 12] = [
    ("default", "#0078D4"),
    ("midnight", "#2563EB"),
    ("catppuccin", "#CBA6F7"),
    ("dracula", "#BD93F9"),
    ("nord", "#5E81AC"),
    ("tokyo-night", "#34548A"),
    ("github", "#0969DA"),
    ("rose-pine", "#907AA9"),
    ("gruvbox", "#B57614"),
    ("everforest", "#4F8A10"),
    ("solarized", "#268BD2"),
    ("vercel", "#111111"),
];

const REPO_URL: &str = "https://github.com/krambovic/Lumen";
const STEP_ROW: f32 = 52.0;
const BAR_HEIGHT: f32 = 76.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Step {
    News,
    Welcome,
    Look,
    Route,
    Configs,
    System,
    Done,
}

impl Step {
    fn title(self) -> &'static str {
        match self {
            Self::News => "Что нового",
            Self::Welcome => "Знакомство",
            Self::Look => "Оформление",
            Self::Route => "Регион и маршруты",
            Self::Configs => "Конфигурации",
            Self::System => "Обновления и запуск",
            Self::Done => "Готово",
        }
    }

    fn glyph(self) -> &'static str {
        match self {
            Self::News => "\u{e8bc}",
            Self::Welcome => "\u{e80f}",
            Self::Look => "\u{e790}",
            Self::Route => "\u{e909}",
            Self::Configs => "\u{ec05}",
            Self::System => "\u{e713}",
            Self::Done => "\u{e73e}",
        }
    }
}

fn steps(setup: bool, news: bool) -> Vec<Step> {
    let mut flow = vec![if news { Step::News } else { Step::Welcome }];
    if setup || !news {
        flow.extend([
            Step::Look,
            Step::Route,
            Step::Configs,
            Step::System,
            Step::Done,
        ]);
    }
    flow
}


#[derive(Default)]
pub(super) struct Onboarding {
    flow: Vec<Step>,
    step: usize,
    last_step: usize,
    step_dir: f32,
    step_changed_at: f64,
    opened_at: Option<f64>,
    /// Animation clock (seconds) and the wall time it last advanced at.
    clock: f64,
    clock_wall: Option<f64>,
    view_height: f32,
    config_choice: Option<bool>,
    subscription: String,
    import_error: Option<String>,
    windows_accent: String,
    show_support: bool,
    /// `onboardingAccentDlg` (ColorPickerDialog, `showDarkness: true`).
    color_draft: Option<super::color_picker::ColorPicker>,
    typing: bool,
    finished: bool,
    /// `closing`: when `exitAnim` started.
    closing_at: Option<f64>,
    blur: BackdropBlur,
}

impl Onboarding {
    /// `title_layer` is the title bar's layer: its caption and buttons are
    /// repainted above the backdrop, which covers the whole window.
    pub(super) fn render(
        &mut self,
        ctx: &Context,
        theme: &Theme,
        bridge: &mut AppBridge,
        title_layer: egui::LayerId,
        window_shown: bool,
    ) -> bool {
        // The flow's own clock advances at most 50 ms per frame. Opening
        // the flow or a step builds and rasterizes a whole page in one
        // frame (about 0.3 s in a debug build); on the wall clock the card
        // would appear already 80% faded in.
        let wall = ctx.input(|input| input.time);
        let step = self.clock_wall.map_or(0.0, |last| (wall - last).clamp(0.0, 0.05));
        self.clock_wall = Some(wall);
        self.clock += step;
        let now = self.clock;
        let animated = theme.anim_fast > 0;
        // `exitAnim` runs for 600 ms after `completeOnboarding()`.
        let closing = self.closing_at.map(|start| (now - start) as f32);
        if let Some(elapsed) = closing {
            if !animated || elapsed >= 0.6 {
                *self = Self { finished: true, blur: self.blur.clone(), ..Self::default() };
                return false;
            }
            ctx.request_repaint();
        } else {
            if !bridge.onboarding_required() {
                *self = Self { blur: self.blur.clone(), ..Self::default() };
                return false;
            }
            if bridge.locked() || self.finished {
                return false;
            }
        }
        if self.flow.is_empty() {
            // `open()` runs from Component.onCompleted, but Qt only advances
            // the enter animation once the window is exposed: the dashboard
            // shows first and the backdrop and card animate in on screen.
            // The window here is revealed after its first painted frame, so
            // the flow opens once it is actually visible.
            if !window_shown {
                return false;
            }
            self.flow = steps(
                bridge.onboarding_setup_required(),
                bridge.onboarding_show_whats_new(),
            );
            let returning =
                bridge.onboarding_returning_user() || bridge.onboarding_server_count() > 0;
            self.config_choice = returning.then_some(true);
            self.windows_accent = bridge.system_accent();
            if !returning
                && !self.windows_accent.is_empty()
                && bridge.accent_color().eq_ignore_ascii_case("#0078D4")
            {
                bridge.apply_system_accent_quietly();
            }
            self.opened_at = Some(now);
            // `revealAnim` also runs for the first page.
            self.step_changed_at = now;
        }
        self.step = self.step.min(self.flow.len() - 1);
        if self.step != self.last_step {
            self.step_dir = if self.step > self.last_step { 1.0 } else { -1.0 };
            self.last_step = self.step;
            self.step_changed_at = now;
        }
        let current = self.flow[self.step];
        let last = self.step + 1 == self.flow.len();
        let mut finish = false;

        // The overlay lives below the title bar so the window stays draggable
        // and closable; the shield below blocks the page content behind it.
        let screen = ctx.viewport_rect();
        let maximized = ctx.input(|input| input.viewport().maximized.unwrap_or(false));
        let title_height = TITLE_BAR_HEIGHT + if maximized { 2.0 } else { 0.0 };
        let region = Rect::from_min_max(pos2(screen.left(), screen.top() + title_height), screen.max);

        let open_elapsed = (now - self.opened_at.unwrap_or(now)) as f32;
        let mut backdrop_level =
            motion::progress(ctx, theme, open_elapsed, 420.0, motion::ease_out_cubic);
        let card_level = motion::progress(
            ctx,
            theme,
            (open_elapsed - 0.11).max(0.0),
            620.0,
            motion::ease_out_quint,
        );
        // enterAnim: opacity 0 -> 1, scale 0.94 -> 1, y 28 -> 0 (620 ms, OutQuint).
        let mut card_opacity = card_level;
        let mut card_scale = 0.94 + 0.06 * card_level;
        let mut card_offset = 28.0 * (1.0 - card_level);
        if let Some(elapsed) = closing {
            // exitAnim: the card leaves in 380 ms (InCubic); the backdrop
            // follows after 140 ms over 460 ms (InOutCubic).
            let out = motion::ease_in_cubic(elapsed / 0.38);
            card_opacity = 1.0 - out;
            card_scale = 1.0 - 0.06 * out;
            card_offset = 28.0 * out;
            backdrop_level = 1.0 - motion::ease_in_out_cubic((elapsed - 0.14) / 0.46);
        }
        // StepPage `revealAnim`: 0 -> 1 linearly over 1100 ms.
        let reveal = if animated {
            ((now - self.step_changed_at) as f32 / 1.1).clamp(0.0, 1.0)
        } else {
            1.0
        };
        if reveal < 1.0 {
            ctx.request_repaint();
        }
        self.typing = false;

        egui::Area::new(Id::unique("lumen-onboarding"))
            .order(egui::Order::Foreground)
            // `enabled: shown && !closing`.
            .interactable(closing.is_none())
            .movable(false)
            .fixed_pos(region.min)
            .show(ctx, |ui| {
                ui.set_clip_rect(region.intersect(ui.clip_rect()));
                ui.allocate_rect(region, Sense::click_and_drag());
                let layer_id = ui.layer_id();
                // The backdrop fills the window, title strip included; the
                // overlay's own clip rect starts below the title bar.
                let backdrop = ctx.layer_painter(layer_id);
                if self.blur.unsupported() {
                    backdrop.rect_filled(screen, 0.0, theme.bg.gamma_multiply(backdrop_level));
                } else {
                    let tint = if theme.dark { 0.62 } else { 0.56 };
                    backdrop.add(self.blur.shape(screen, theme.bg, tint, backdrop_level));
                }
                // TitleBar.qml (z 50) lies above the overlay (z 40): its caption
                // and buttons go back on top of the backdrop. The strip itself
                // measures the same as the backdrop in the QML client.
                let caption: Vec<_> = ctx.graphics(|graphics| {
                    graphics
                        .get(title_layer)
                        .map(|list| list.all_entries().cloned().collect())
                        .unwrap_or_default()
                });
                ctx.graphics_mut(|graphics| {
                    let list = graphics.entry(layer_id);
                    for clipped in caption {
                        list.add(clipped.clip_rect, clipped.shape);
                    }
                });

                let mut layer = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(region)
                        .layout(Layout::top_down(Align::LEFT)),
                );
                layer.set_opacity(card_opacity);
                let ui = &mut layer;
                // Everything from here on belongs to `cardLayer` and is scaled below.
                let card_shapes = ui.painter().add(egui::Shape::Noop);

                let width = (region.width() - 48.0).min(980.0).max(0.0);
                let height = (region.height() - 40.0).min(640.0).max(0.0);
                let card = Rect::from_min_size(
                    pos2(
                        region.left() + ((region.width() - width) / 2.0).round(),
                        region.top()
                            + ((region.height() - height) / 2.0).round()
                            + card_offset,
                    ),
                    vec2(width, height),
                );
                let radius = (theme.radius as f32 + 6.0).max(10.0);
                let shadow = egui::Shadow {
                    offset: [0, 22],
                    blur: 60,
                    spread: 0,
                    color: Color32::from_black_alpha(if theme.dark { 140 } else { 51 }),
                };
                ui.painter().add(shadow.as_shape(card, radius));
                ui.painter().rect_filled(card, radius, theme.flyout);

                let wide = card.width() >= 760.0;
                let side_width = if wide { 272.0 } else { 0.0 };
                if wide {
                    self.side_panel(
                        ui,
                        theme,
                        Rect::from_min_size(card.min, vec2(side_width, card.height())),
                        card_level,
                    );
                }

                let main_left = card.left() + side_width;
                let margin = if wide { 40.0 } else { 24.0 };
                let top_margin = if wide { 34.0 } else { 38.0 };
                if !wide {
                    self.progress_pills(
                        ui,
                        theme,
                        Rect::from_min_max(pos2(main_left, card.top()), card.max),
                    );
                }

                let pages_rect = Rect::from_min_max(
                    pos2(main_left + margin, card.top() + top_margin),
                    pos2(card.right() - margin, card.bottom() - BAR_HEIGHT),
                );
                self.view_height = pages_rect.height();
                // StepPage: `opacity: current ? 1 : 0` (300 ms, OutCubic) and an
                // x offset of -56 / +56 for pages before / after the current one
                // (520 ms, OutQuint). A page stays visible while it fades, so a
                // step change cross-fades the two pages.
                for index in 0..self.flow.len() {
                    let is_current = index == self.step;
                    let step_id = Id::unique(("onboarding-step-motion", index));
                    let opacity = motion::follow(
                        ctx,
                        theme,
                        step_id.with("opacity"),
                        if is_current { 1.0 } else { 0.0 },
                        300.0,
                        motion::ease_out_cubic,
                    );
                    let offset = motion::follow(
                        ctx,
                        theme,
                        step_id.with("x"),
                        if is_current {
                            0.0
                        } else if index < self.step {
                            -56.0
                        } else {
                            56.0
                        },
                        520.0,
                        motion::ease_out_quint,
                    );
                    if !is_current && opacity <= 0.001 {
                        continue;
                    }
                    let mut pages_ui = ui.new_child(
                        egui::UiBuilder::new()
                            .max_rect(pages_rect.translate(vec2(offset, 0.0)))
                            .layout(Layout::top_down(Align::LEFT)),
                    );
                    pages_ui.set_clip_rect(pages_rect.intersect(ui.clip_rect()));
                    pages_ui.multiply_opacity(opacity);
                    if !is_current {
                        // `enabled: current`, without the disabled tint.
                        pages_ui.visuals_mut().disabled_alpha = 1.0;
                        pages_ui.disable();
                    }
                    let step = self.flow[index];
                    ScrollArea::vertical()
                        .id_salt(("onboarding-step", index))
                        .auto_shrink([false, false])
                        .show(&mut pages_ui, |ui| {
                            ui.spacing_mut().item_spacing = Vec2::ZERO;
                            match step {
                                Step::News => self.news(ui, theme, bridge),
                                Step::Welcome => welcome(ui, theme, bridge),
                                Step::Look => self.look(ui, theme, bridge),
                                Step::Route => route(ui, theme, bridge),
                                Step::Configs => self.configs(ui, theme, bridge),
                                Step::System => system(ui, theme, bridge),
                                Step::Done => self.done(ui, theme, bridge),
                            }
                        });
                    apply_reveal(ctx, layer_id, is_current.then_some(reveal));
                }

                // ── action bar ──
                let bar = Rect::from_min_max(pos2(main_left, card.bottom() - BAR_HEIGHT), card.max);
                // A fixed id scope: one or two pages render above this, and
                // auto ids that shifted with them restarted the buttons' colour
                // animations, so they flashed at every step change.
                let mut bar_ui = ui.new_child(
                    egui::UiBuilder::new()
                        .id_salt("onboarding-action-bar")
                        .max_rect(bar)
                        .layout(Layout::top_down(Align::Min)),
                );
                let ui = &mut bar_ui;
                ui.painter().rect_filled(
                    Rect::from_min_size(bar.min, vec2(bar.width(), 1.0)),
                    0.0,
                    theme.divider,
                );
                let keyboard =
                    closing.is_none() && !ctx.egui_wants_keyboard_input() && !self.typing;
                let next_key = keyboard
                    && ctx.input_mut(|input| {
                        input.consume_key(Modifiers::NONE, Key::Enter)
                            || (!last && input.consume_key(Modifiers::NONE, Key::ArrowRight))
                    });
                let back_key = keyboard
                    && ctx.input_mut(|input| input.consume_key(Modifiers::NONE, Key::ArrowLeft));

                let control = theme.control_height as f32;
                let center_y = bar.center().y;
                let row_left = bar.left() + margin;
                let row_right = bar.right() - margin;
                let next_label = tr(
                    if last {
                        "Открыть Lumen"
                    } else if current == Step::Welcome && self.flow[0] == Step::Welcome {
                        "Начать"
                    } else {
                        "Далее"
                    },
                    &[],
                );
                let next_glyph = if last { "\u{e73e}" } else { "" };
                let next_width =
                    button_width(ui, theme, next_glyph, &next_label, ButtonKind::Accent).max(132.0);
                let next_rect = Rect::from_min_size(
                    pos2(row_right - next_width, center_y - control / 2.0),
                    vec2(next_width, control),
                );
                if put_button(
                    ui,
                    theme,
                    next_rect,
                    FluentButton::new(ButtonKind::Accent, next_glyph, &next_label)
                        .width(next_width),
                )
                .clicked()
                    || next_key
                {
                    if last {
                        finish = true;
                    } else {
                        self.step += 1;
                    }
                }

                let back_label = tr("Назад", &[]);
                let back_width = button_width(ui, theme, "\u{e72b}", &back_label, ButtonKind::Ghost);
                let back_rect = Rect::from_min_size(
                    pos2(next_rect.left() - 10.0 - back_width, center_y - control / 2.0),
                    vec2(back_width, control),
                );
                let back_level = motion::follow(
                    ctx,
                    theme,
                    Id::unique("onboarding-back-opacity"),
                    if self.step > 0 { 1.0 } else { 0.0 },
                    200.0,
                    motion::linear,
                );
                if back_level > 0.001 {
                    let mut child = ui.new_child(
                        egui::UiBuilder::new()
                            .max_rect(back_rect)
                            .layout(Layout::top_down(Align::Min)),
                    );
                    child.multiply_opacity(back_level);
                    let clicked = FluentButton::new(ButtonKind::Ghost, "\u{e72b}", &back_label)
                        .enabled(self.step > 0)
                        .width(back_width)
                        .show(&mut child, theme)
                        .clicked();
                    if (clicked || back_key) && self.step > 0 && !next_key {
                        self.step -= 1;
                    }
                }

                if !last {
                    let skip_label = tr("Пропустить", &[]);
                    let skip_width = button_width(ui, theme, "", &skip_label, ButtonKind::Ghost);
                    let skip_rect = Rect::from_min_size(
                        pos2(row_left, center_y - control / 2.0),
                        vec2(skip_width, control),
                    );
                    if put_button(
                        ui,
                        theme,
                        skip_rect,
                        FluentButton::new(ButtonKind::Ghost, "", &skip_label).width(skip_width),
                    )
                    .clicked()
                    {
                        finish = true;
                    }
                }

                // The card border sits above everything (`cardBorder`). It is
                // a child of `cardLayer`, so it fades with the card: the
                // layer painter would ignore `card_opacity` and leave a bright
                // outline while the card flies in and out.
                ui.painter().with_clip_rect(region).rect_stroke(
                    card,
                    radius,
                    Stroke::new(1.0, theme.flyout_border),
                    StrokeKind::Inside,
                );
                if (card_scale - 1.0).abs() > 0.0005 {
                    // `cardLayer.scale` around its centre.
                    let center = card.center().to_vec2();
                    let transform =
                        egui::emath::TSTransform::new(center * (1.0 - card_scale), card_scale);
                    ctx.graphics_mut(|graphics| {
                        let list = graphics.entry(layer_id);
                        let end = list.next_idx();
                        list.transform_range(card_shapes, end, transform);
                    });
                }
            });
        self.color_dialog(ctx, theme, bridge);
        if self.step != self.last_step {
            ctx.request_repaint();
        }
        let finish = finish && closing.is_none();
        if finish {
            // `finish()`: the profile is completed at once, the card leaves
            // while the dashboard settles in behind it.
            self.closing_at = Some(now);
            bridge.complete_onboarding();
            ctx.request_repaint();
        }
        finish
    }

    /// Brand + stepper + bottom hint (QML `side` panel).
    fn side_panel(&mut self, ui: &mut Ui, theme: &Theme, side: Rect, level: f32) {
        let ctx = ui.ctx().clone();
        let painter = ui.painter().clone();
        painter.rect_filled(
            Rect::from_min_size(pos2(side.right() - 1.0, side.top()), vec2(1.0, side.height())),
            0.0,
            theme.divider,
        );
        let inner = side.shrink(28.0);

        let name = lay(ui, "Lumen", semibold(px(theme, 20.0)), theme.text);
        let version = lay(
            ui,
            &format!("v{APP_VERSION}"),
            FontId::proportional(theme.font_small as f32),
            theme.text_faint,
        );
        let text_height = name.size().y + version.size().y;
        let brand_height = text_height.max(34.0);
        lumen_mark(
            &painter,
            Rect::from_min_size(
                pos2(inner.left(), inner.top() + (brand_height - 34.0) / 2.0),
                vec2(34.0, 34.0),
            ),
            theme.text,
            level,
        );
        let text_x = inner.left() + 34.0 + 12.0;
        let text_y = inner.top() + (brand_height - text_height) / 2.0;
        let version_y = text_y + name.size().y;
        painter.galley(pos2(text_x, text_y), name, theme.text);
        painter.galley(pos2(text_x, version_y), version, theme.text_faint);

        let stepper = Rect::from_min_size(
            pos2(inner.left(), inner.top() + brand_height + 40.0),
            vec2(inner.width(), self.flow.len() as f32 * STEP_ROW),
        );
        let count = self.flow.len();
        let rail_top = stepper.top() + 26.0;
        painter.rect_filled(
            Rect::from_min_size(
                pos2(stepper.left() + 15.0, rail_top),
                vec2(2.0, (count - 1) as f32 * STEP_ROW),
            ),
            1.0,
            theme.divider,
        );
        let fill = motion::follow(
            &ctx,
            theme,
            Id::unique("onboarding-rail-fill"),
            self.step as f32 * STEP_ROW,
            520.0,
            motion::ease_out_quint,
        );
        painter.rect_filled(
            Rect::from_min_size(pos2(stepper.left() + 15.0, rail_top), vec2(2.0, fill)),
            1.0,
            theme.accent,
        );

        let mut jump = None;
        for (index, step) in self.flow.iter().enumerate() {
            let done = index < self.step;
            let active = index == self.step;
            let row = Rect::from_min_size(
                pos2(stepper.left(), stepper.top() + STEP_ROW * index as f32),
                vec2(stepper.width(), STEP_ROW),
            );
            let key = Id::unique(("onboarding-step-row", index));
            let lit = active || done;
            let scale = motion::follow(
                &ctx,
                theme,
                key.with("scale"),
                if active { 1.0 } else { 0.86 },
                420.0,
                ease_out_back,
            );
            let dot_fill = motion::follow_color(
                &ctx,
                theme,
                key.with("fill"),
                if lit { theme.accent } else { theme.flyout },
                260.0,
                motion::linear,
            );
            let center = pos2(row.left() + 16.0, row.top() + 10.0 + 16.0);
            let radius = 16.0 * scale;
            painter.circle_filled(center, radius, dot_fill);
            if !lit {
                painter.circle_stroke(
                    center,
                    radius - 1.0,
                    Stroke::new(1.0, theme.flyout_border),
                );
            }
            let glyph_color = if lit { theme.accent_text } else { theme.text_muted };
            let glyph = lay(
                ui,
                if done { "\u{e73e}" } else { step.glyph() },
                FontId::proportional(14.0 * scale),
                glyph_color,
            );
            painter.galley(center - glyph.size() / 2.0, glyph, glyph_color);

            let label_color = motion::follow_color(
                &ctx,
                theme,
                key.with("label"),
                if active {
                    theme.text
                } else if done {
                    theme.text_muted
                } else {
                    theme.text_faint
                },
                220.0,
                motion::linear,
            );
            let font = if active {
                semibold(theme.font_normal as f32)
            } else {
                FontId::proportional(theme.font_normal as f32)
            };
            let label = lay(ui, &tr(step.title(), &[]), font, label_color);
            painter.galley(
                pos2(row.left() + 32.0 + 14.0, center.y - label.size().y / 2.0),
                label,
                label_color,
            );
            if done {
                let response = ui.interact(row, key, Sense::click());
                if response.hovered() {
                    ctx.set_cursor_icon(CursorIcon::PointingHand);
                }
                if response.clicked() {
                    jump = Some(index);
                }
            }
        }
        if let Some(index) = jump {
            self.step = index;
        }

        // Hint: shrinks to fit (QML `HorizontalFit`, minimum 10 px), then elides.
        let hint = tr("Всё можно изменить позже в Настройках", &[]);
        let mut size = theme.font_small as f32;
        let mut galley = lay(ui, &hint, FontId::proportional(size), theme.text_faint);
        if galley.size().x > inner.width() {
            size = (size * inner.width() / galley.size().x).max(10.0);
            galley = lay(ui, &hint, FontId::proportional(size), theme.text_faint);
        }
        if galley.size().x > inner.width() {
            galley = elided(
                ui,
                &hint,
                FontId::proportional(size),
                theme.text_faint,
                inner.width(),
            );
        }
        painter.galley(
            pos2(inner.left(), inner.bottom() - galley.size().y),
            galley,
            theme.text_faint,
        );
    }

    /// Compact progress pills of the narrow layout.
    fn progress_pills(&self, ui: &mut Ui, theme: &Theme, main: Rect) {
        let ctx = ui.ctx().clone();
        let widths: Vec<f32> = (0..self.flow.len())
            .map(|index| {
                motion::follow(
                    &ctx,
                    theme,
                    Id::unique(("onboarding-pill", index)),
                    if index == self.step { 26.0 } else { 6.0 },
                    360.0,
                    motion::ease_out_quint,
                )
            })
            .collect();
        let total = widths.iter().sum::<f32>() + 6.0 * (widths.len().saturating_sub(1)) as f32;
        let mut x = main.center().x - total / 2.0;
        for (index, width) in widths.into_iter().enumerate() {
            let color = motion::follow_color(
                &ctx,
                theme,
                Id::unique(("onboarding-pill-color", index)),
                if index <= self.step { theme.accent } else { theme.divider },
                240.0,
                motion::linear,
            );
            ui.painter().rect_filled(
                Rect::from_min_size(pos2(x, main.top() + 18.0), vec2(width, 6.0)),
                3.0,
                color,
            );
            x += width + 6.0;
        }
    }

    fn news(&self, ui: &mut Ui, theme: &Theme, bridge: &mut AppBridge) {
        let version = bridge.app_version();
        let previous = bridge.onboarding_previous_version();
        let full_setup = self.flow.len() > 1;
        let key = match (full_setup, previous.is_empty()) {
            (true, false) => {
                "Предыдущая версия — {version}. Посмотрите, что изменилось, а затем проверим основные настройки."
            }
            (true, true) => "Посмотрите, что изменилось, а затем проверим основные настройки.",
            (false, false) => "Предыдущая версия — {version}. Вот что изменилось.",
            (false, true) => "Вот что изменилось в этой версии.",
        };
        hero_header(
            ui,
            theme,
            &tr("Lumen обновлён до версии {version}", &[("version", version)]),
            &tr(key, &[("version", &previous)]),
            28.0,
            8.0,
            0.0,
            16.0,
        );
        for (index, group) in bridge.onboarding_whats_new().into_iter().enumerate() {
            let version = group["version"].as_str().unwrap_or_default().to_owned();
            rv(ui, 3.0 + index as f32, 16.0, |ui| {
                section_label(ui, theme, &tr("Что нового в {version}", &[("version", &version)]));
                group_card(ui, theme, &group);
            });
        }
        ui.add_space(8.0);
    }

    fn look(&mut self, ui: &mut Ui, theme: &Theme, bridge: &mut AppBridge) {
        header(
            ui,
            theme,
            &tr("Оформление", &[]),
            &tr("Выберите внешний вид — изменения применяются сразу", &[]),
        );
        // Upgraded profiles pick the language here; fresh installs did on welcome.
        if self.flow[0] != Step::Welcome {
            rv(ui, 1.0, 16.0, |ui| language_picker(ui, theme, bridge));
        }

        rv(ui, 2.0, 0.0, |ui| section_label(ui, theme, &tr("Тема", &[])));
        rv(ui, 2.0, 16.0, |ui| {
            let rects = grid(ui, 3, 10.0, 3, 112.0);
            let active_theme = bridge.theme_name().to_owned();
            for (index, key) in ["system", "light", "dark"].into_iter().enumerate() {
                let label = tr(theme_label(key), &[]);
                let id = Id::unique(("onboarding-theme", index));
                if theme_card(ui, theme, rects[index], id, key, &label, active_theme == key) {
                    bridge.set_theme(key);
                }
            }
        });

        rv(ui, 3.0, 0.0, |ui| section_label(ui, theme, &tr("Палитра", &[])));
        let palette_start = ui.painter().add(egui::Shape::Noop).0;
        let dark_map = dark_preset_map();
        let light_map = light_preset_map();
        let preset = bridge.settings().ui_theme_preset.clone();
        let origin = ui.cursor().min;
        let sizes = vec![vec2(92.0, 84.0); PALETTES.len()];
        let (rects, total) = flow_layout(origin, ui.available_width(), 10.0, &sizes);
        ui.allocate_exact_size(vec2(ui.available_width(), total), Sense::hover());
        for (index, (key, label)) in PALETTES.into_iter().enumerate() {
            let (base, surface) = if theme.dark {
                dark_map.get(key).map_or((theme.bg, theme.card), |palette| {
                    (
                        hex_color(palette.d0).unwrap_or(theme.bg),
                        hex_color(palette.d1).unwrap_or(theme.card),
                    )
                })
            } else {
                light_map.get(key).map_or((theme.bg, theme.card), |palette| {
                    (
                        hex_color(palette.bg).unwrap_or(theme.bg),
                        hex_color(palette.layer).unwrap_or(theme.card),
                    )
                })
            };
            let tint = PALETTE_ACCENTS
                .iter()
                .find(|(k, _)| *k == key)
                .and_then(|(_, hex)| hex_color(hex))
                .unwrap_or(theme.accent);
            let swatch = Swatch {
                label,
                base,
                surface,
                tint,
                selected: preset == key,
            };
            if swatch.show(
                ui,
                theme,
                rects[index],
                Id::unique(("onboarding-palette", index)),
            ) {
                bridge.set_ui_theme_preset(key);
            }
        }
        let palette_end = ui.painter().add(egui::Shape::Noop).0;
        REVEAL_BLOCKS.with(|blocks| blocks.borrow_mut().push((palette_start, palette_end, 3.0, 16.0)));

        rv(ui, 4.0, 0.0, |ui| section_label(ui, theme, &tr("Цвет акцента", &[])));
        rv(ui, 4.0, 16.0, |ui| self.accent_flow(ui, theme, bridge));
        ui.add_space(8.0);
    }

    /// System accent first, then the swatches, then the custom colour dot.
    fn accent_flow(&mut self, ui: &mut Ui, theme: &Theme, bridge: &mut AppBridge) {
        enum Item {
            Swatch(String),
            Divider,
            Custom,
        }
        let current = bridge.accent_color().to_owned();
        let windows = self.windows_accent.clone();
        let mut items = Vec::new();
        if !windows.is_empty() {
            items.push(Item::Swatch(windows.clone()));
            items.push(Item::Divider);
        }
        let mut custom_selected = windows.is_empty() || !current.eq_ignore_ascii_case(&windows);
        for hex in ACCENT_SWATCHES.iter().take(10) {
            if windows.is_empty() || !hex.eq_ignore_ascii_case(&windows) {
                items.push(Item::Swatch((*hex).to_owned()));
                if current.eq_ignore_ascii_case(hex) {
                    custom_selected = false;
                }
            }
        }
        items.push(Item::Custom);
        let sizes: Vec<Vec2> = items
            .iter()
            .map(|item| match item {
                Item::Divider => vec2(9.0, 34.0),
                _ => vec2(34.0, 34.0),
            })
            .collect();
        let origin = ui.cursor().min;
        let (rects, total) = flow_layout(origin, ui.available_width(), 8.0, &sizes);
        ui.allocate_exact_size(vec2(ui.available_width(), total), Sense::hover());
        for (index, item) in items.iter().enumerate() {
            let rect = rects[index];
            let id = Id::unique(("onboarding-accent", index));
            match item {
                Item::Swatch(hex) => {
                    let Some(tint) = hex_color(hex) else { continue };
                    if accent_dot(ui, theme, rect, id, tint, current.eq_ignore_ascii_case(hex), false)
                        .clicked()
                    {
                        bridge.set_accent(hex);
                    }
                }
                Item::Divider => {
                    ui.painter().rect_filled(
                        Rect::from_center_size(rect.center(), vec2(1.0, 20.0)),
                        0.0,
                        theme.divider,
                    );
                }
                Item::Custom => {
                    let tint = hex_color(&current).unwrap_or(theme.accent);
                    let response = accent_dot(ui, theme, rect, id, tint, custom_selected, true)
                        .fluent_tip(tr("Свой цвет", &[]));
                    if response.clicked() {
                        self.color_draft = Some(super::color_picker::ColorPicker::open(
                            "onboarding-accent",
                            tr("Цвет акцента", &[]),
                            &current,
                            0.0,
                            true,
                            false,
                        ));
                    }
                }
            }
        }
    }

    fn color_dialog(&mut self, ctx: &Context, theme: &Theme, bridge: &mut AppBridge) {
        use super::color_picker::PickerResult;
        let Some(picker) = self.color_draft.as_mut() else {
            return;
        };
        match picker.show(ctx, theme, || bridge.system_accent()) {
            PickerResult::Open => {}
            PickerResult::Closed => self.color_draft = None,
            PickerResult::Accepted { hex, .. } => {
                bridge.set_accent(&hex);
                self.color_draft = None;
            }
        }
    }

    fn configs(&mut self, ui: &mut Ui, theme: &Theme, bridge: &mut AppBridge) {
        let count = bridge.onboarding_server_count().to_string();
        let description = if count == "0" {
            tr(
                "Ключ, ссылка на подписку или файл конфигурации — Lumen поймёт любой из них",
                &[],
            )
        } else {
            tr(
                "Серверов в Lumen: {count}. Можно добавить ещё или перейти дальше.",
                &[("count", &count)],
            )
        };
        header(
            ui,
            theme,
            &tr("У вас уже есть VPN-конфигурация?", &[]),
            &description,
        );

        ui.add_space(18.0);
        let answers_start = ui.painter().add(egui::Shape::Noop).0;
        let width = ui.available_width();
        let cols = if width >= 520.0 { 2 } else { 1 };
        let cell = grid_cell_width(width, cols, 10.0);
        let yes_title = tr("Да, есть", &[]);
        let yes_sub = tr("Импортирую ключ, подписку или файл", &[]);
        let no_title = tr("Нет, пока нет", &[]);
        let no_sub = tr("Получу бесплатный WARP прямо здесь", &[]);
        let cards = [
            Choice {
                glyph: "\u{e8b7}",
                flag: None,
                title: &yes_title,
                subtitle: &yes_sub,
                recommended: false,
                selected: self.config_choice == Some(true),
                enabled: true,
            },
            Choice {
                glyph: "\u{ea18}",
                flag: None,
                title: &no_title,
                subtitle: &no_sub,
                recommended: false,
                selected: self.config_choice == Some(false),
                enabled: true,
            },
        ];
        let cell_height = cards
            .iter()
            .map(|card| card.height(ui, theme, cell))
            .fold(0.0, f32::max);
        let rects = grid(ui, cols, 10.0, cards.len(), cell_height);
        for (index, card) in cards.iter().enumerate() {
            if card.show(ui, theme, rects[index], Id::unique(("onboarding-answer", index))) {
                self.config_choice = Some(index == 0);
            }
        }
        let answers_end = ui.painter().add(egui::Shape::Noop).0;
        REVEAL_BLOCKS.with(|blocks| blocks.borrow_mut().push((answers_start, answers_end, 1.0, 16.0)));

        let importing = bridge.subscription_importing();
        reveal(
            ui,
            theme,
            Id::unique("onboarding-import-panel"),
            self.config_choice == Some(true),
            16.0,
            |ui| {
                card_frame(theme, 18.0).hoverable(false).show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.spacing_mut().item_spacing = Vec2::ZERO;
                    let paste = tr("Вставить из буфера", &[]);
                    let open = tr("Открыть файл", &[]);
                    match button_flow(
                        ui,
                        theme,
                        &[
                            Btn { kind: ButtonKind::Accent, glyph: "\u{e77f}", text: &paste, enabled: true },
                            Btn { kind: ButtonKind::Ghost, glyph: "\u{e8e5}", text: &open, enabled: true },
                        ],
                    ) {
                        Some(0) => import_clipboard(bridge),
                        Some(1) => {
                            self.import_error = import_config_file(bridge)
                                .err()
                                .map(|error| error.to_string());
                        }
                        _ => {}
                    }
                    if let Some(error) = &self.import_error {
                        ui.add_space(12.0);
                        paragraph(
                            ui,
                            error,
                            FontId::proportional(theme.font_small as f32),
                            theme.danger,
                            1.0,
                        );
                    }
                    ui.add_space(12.0);
                    let add_label = tr("Добавить", &[]);
                    let add_enabled = !self.subscription.trim().is_empty() && !importing;
                    let mut submit = false;
                    ui.allocate_ui_with_layout(
                        vec2(ui.available_width(), theme.control_height as f32),
                        Layout::right_to_left(Align::Center),
                        |ui| {
                            submit = FluentButton::new(ButtonKind::Ghost, "\u{e71b}", &add_label)
                                .enabled(add_enabled)
                                .show(ui, theme)
                                .clicked();
                            ui.add_space(8.0);
                            let response = ui.add_enabled(
                                !importing,
                                TextEdit::singleline(&mut self.subscription)
                                    .desired_width(ui.available_width())
                                    .min_size(vec2(0.0, theme.control_height as f32))
                                    .align(egui::Align2::LEFT_CENTER)
                                    .margin(egui::Margin {
                                        left: 34,
                                        right: 10,
                                        top: 4,
                                        bottom: 4,
                                    })
                                    .hint_text(tr("Ссылка на подписку (https://…)", &[])),
                            );
                            let glyph = lay(
                                ui,
                                "\u{e71b}",
                                FontId::proportional(14.0),
                                theme.text_faint,
                            );
                            ui.painter().galley(
                                pos2(
                                    response.rect.left() + 12.0,
                                    response.rect.center().y - glyph.size().y / 2.0,
                                ),
                                glyph,
                                theme.text_faint,
                            );
                            if response.has_focus() || response.lost_focus() {
                                self.typing = true;
                            }
                            if response.lost_focus()
                                && ui.input(|input| input.key_pressed(Key::Enter))
                                && add_enabled
                            {
                                submit = true;
                            }
                        },
                    );
                    if submit && add_enabled {
                        bridge.import_subscription(self.subscription.trim(), "");
                        self.subscription.clear();
                    }
                    ui.add_space(12.0);
                    let has_servers = count != "0";
                    let status = if importing {
                        tr("Загрузка подписки...", &[])
                    } else if has_servers {
                        tr("Серверов в Lumen: {count}", &[("count", &count)])
                    } else {
                        tr(
                            "Файлы .conf, .ovpn, .json и .yaml можно просто перетащить в окно",
                            &[],
                        )
                    };
                    status_row(
                        ui,
                        theme,
                        importing,
                        has_servers && !importing,
                        &status,
                        if has_servers { theme.text } else { theme.text_faint },
                    );
                });
            },
        );

        let warp_busy = bridge.warp_generating();
        let warp_done = bridge.warp_generated_count() > 0;
        reveal(
            ui,
            theme,
            Id::unique("onboarding-warp-panel"),
            self.config_choice == Some(false),
            16.0,
            |ui| {
                card_frame(theme, 18.0).hoverable(false).show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.spacing_mut().item_spacing = Vec2::ZERO;
                    paragraph(
                        ui,
                        &tr("Бесплатный Cloudflare WARP", &[]),
                        semibold(px(theme, 16.0)),
                        theme.text,
                        1.0,
                    );
                    ui.add_space(6.0);
                    paragraph(
                        ui,
                        &tr("Lumen зарегистрирует бесплатный профиль WARP и добавит готовые конфигурации AmneziaWG и MASQUE в группу «WARP».", &[]),
                        FontId::proportional(theme.font_normal as f32),
                        theme.text_muted,
                        1.15,
                    );
                    ui.add_space(6.0 + 8.0);
                    let label = tr(
                        if warp_busy {
                            "Получаем профили…"
                        } else if warp_done {
                            "Профили WARP добавлены"
                        } else {
                            "Получить WARP"
                        },
                        &[],
                    );
                    let kind = if warp_done { ButtonKind::Ghost } else { ButtonKind::Accent };
                    let glyph = if warp_done { "\u{e73e}" } else { "\u{e753}" };
                    let button_w = button_width(ui, theme, glyph, &label, kind);
                    let status = tr(
                        if warp_busy {
                            "Обычно это занимает несколько секунд"
                        } else if warp_done {
                            "Серверов в Lumen: {count}"
                        } else {
                            "AmneziaWG ×3 и MASQUE ×2 — выберите любой на панели"
                        },
                        &[("count", &count)],
                    );
                    let status_color = if warp_done && !warp_busy {
                        theme.text
                    } else {
                        theme.text_faint
                    };
                    let row_w = ui.available_width();
                    let mut x = 0.0;
                    x += button_w + 10.0;
                    if warp_busy {
                        x += 18.0 + 10.0;
                    }
                    let text = lay_wrapped(
                        ui,
                        &status,
                        FontId::proportional(theme.font_small as f32),
                        status_color,
                        (row_w - x).max(1.0),
                        1.0,
                    );
                    let row_h = (theme.control_height as f32).max(text.size().y);
                    let (row, _) = ui.allocate_exact_size(vec2(row_w, row_h), Sense::hover());
                    let button_rect = Rect::from_min_size(
                        pos2(row.left(), row.center().y - theme.control_height as f32 / 2.0),
                        vec2(button_w, theme.control_height as f32),
                    );
                    if put_button(
                        ui,
                        theme,
                        button_rect,
                        FluentButton::new(kind, glyph, &label)
                            .width(button_w)
                            .enabled(!warp_busy && !warp_done),
                    )
                    .clicked()
                    {
                        bridge.generate_warp_configs();
                    }
                    let mut text_x = button_rect.right() + 10.0;
                    if warp_busy {
                        crate::shell::widgets::busy_indicator(
                            ui,
                            Rect::from_min_size(
                                pos2(text_x, row.center().y - 9.0),
                                vec2(18.0, 18.0),
                            ),
                            theme.accent,
                        );
                        text_x += 18.0 + 10.0;
                    }
                    ui.painter().galley(
                        pos2(text_x, row.center().y - text.size().y / 2.0),
                        text,
                        status_color,
                    );
                });
            },
        );
        ui.add_space(8.0);
    }

    fn done(&mut self, ui: &mut Ui, theme: &Theme, bridge: &mut AppBridge) {
        let top = ui.cursor().min.y;
        ui.add_space(6.0);
        hero_header(
            ui,
            theme,
            &tr("Всё готово", &[]),
            &tr(
                if bridge.onboarding_server_count() > 0 {
                    "Lumen настроен. Выберите сервер на панели и нажмите «Подключить»."
                } else {
                    "Lumen настроен. Добавьте сервер, когда он появится, — и можно подключаться."
                },
                &[],
            ),
            30.0,
            8.0,
            1.0,
            16.0,
        );
        ui.add_space(22.0);
        let languages = bridge.available_languages();
        let current_language = bridge.language().to_owned();
        let language = languages
            .iter()
            .position(|code| *code == current_language)
            .and_then(|index| bridge.language_labels().get(index).cloned())
            .unwrap_or(current_language);
        let preset = bridge.active_routing_preset_id();
        let route = bridge
            .regional_routing_presets()
            .into_iter()
            .find(|row| row["id"].as_str() == Some(&preset))
            .and_then(|row| row["name"].as_str().map(str::to_owned))
            .unwrap_or_else(|| tr("Свои правила", &[]));
        let ui_preset = bridge.settings().ui_theme_preset.clone();
        let palette = PALETTES
            .iter()
            .find(|(key, _)| *key == ui_preset)
            .map_or_else(
                || {
                    let mut chars = ui_preset.chars();
                    match chars.next() {
                        Some(first) => first.to_uppercase().chain(chars).collect(),
                        None => "Default".to_owned(),
                    }
                },
                |(_, title)| (*title).to_owned(),
            );
        let updates = if bridge.app_auto_update() {
            "Автоматически"
        } else if bridge.check_updates() {
            "Только проверка"
        } else {
            "Выкл"
        };
        let startup = if !bridge.launch_on_startup() {
            "Выкл"
        } else if bridge.launch_in_tray_on_startup() {
            "Вместе с Windows, в трее"
        } else {
            "Вместе с Windows"
        };
        let look = self.flow.iter().position(|&step| step == Step::Look);
        let route_step = self.flow.iter().position(|&step| step == Step::Route);
        let system_step = self.flow.iter().position(|&step| step == Step::System);
        let rows = [
            (
                "\u{e774}",
                tr("Язык", &[]),
                language,
                if self.flow[0] == Step::Welcome { Some(0) } else { look },
            ),
            (
                "\u{e790}",
                tr("Тема", &[]),
                format!("{} · {palette}", tr(theme_label(bridge.theme_name()), &[])),
                look,
            ),
            (
                "\u{e909}",
                tr("Регион", &[]),
                tr(region_label(&bridge.regional_preset()), &[]),
                route_step,
            ),
            ("\u{e72e}", tr("Маршрутизация", &[]), route, route_step),
            ("\u{e777}", tr("Обновления", &[]), tr(updates, &[]), system_step),
            ("\u{e7e8}", tr("Автозапуск", &[]), tr(startup, &[]), system_step),
        ];
        let cols = if ui.available_width() >= 560.0 { 2 } else { 1 };
        let rects = grid(ui, cols, 10.0, rows.len(), 58.0);
        for (index, (glyph, key, value, target)) in rows.iter().enumerate() {
            if rv(ui, 3.0 + index as f32 * 0.5, 16.0, |ui| {
                summary_row(
                    ui,
                    theme,
                    rects[index],
                    Id::unique(("onboarding-summary", index)),
                    glyph,
                    key,
                    value,
                )
            }) && let Some(target) = target
            {
                self.step = *target;
            }
        }

        let used = ui.cursor().min.y - top;
        let support = support_height(ui, theme, self.show_support);
        let filler = (self.view_height - used - support - 12.0).max(32.0);
        ui.add_space(filler);
        rv(ui, 5.5, 16.0, |ui| support_card(ui, theme, bridge, &mut self.show_support));
        ui.add_space(12.0);
    }
}

fn welcome(ui: &mut Ui, theme: &Theme, bridge: &mut AppBridge) {
    let returning = bridge.onboarding_returning_user();
    let title = if returning {
        "С возвращением в Lumen"
    } else {
        "Добро пожаловать в Lumen"
    };
    let intro = if returning {
        tr(
            "Lumen обновлён до версии {version}. Давайте за минуту проверим основные настройки.",
            &[("version", bridge.app_version())],
        )
    } else {
        tr(
            "Lumen — VPN-клиент для Windows: два ядра, TUN и системный прокси, умная маршрутизация и обход DPI через zapret. Настроим всё за минуту.",
            &[],
        )
    };
    hero_header(ui, theme, &tr(title, &[]), &intro, 30.0, 10.0, 1.0, 18.0);
    rv(ui, 3.0, 16.0, |ui| language_picker(ui, theme, bridge));
    rv(ui, 4.0, 0.0, |ui| section_label(ui, theme, &tr("Возможности", &[])));
    let tiles = [
        (
            "\u{ec05}",
            tr("Xray и sing-box", &[]),
            tr("Два ядра и автоматический выбор подходящего", &[]),
        ),
        (
            "\u{e701}",
            tr("TUN и системный прокси", &[]),
            tr("Весь трафик системы или только нужные приложения", &[]),
        ),
        (
            "\u{e774}",
            tr("Умная маршрутизация", &[]),
            tr("Региональные пресеты и свои правила для сайтов", &[]),
        ),
        (
            "\u{e945}",
            "Zapret".to_owned(),
            tr("Обход DPI-блокировок даже без VPN-сервера", &[]),
        ),
    ];
    let width = ui.available_width();
    let cols = if width >= 560.0 { 2 } else { 1 };
    let cell = grid_cell_width(width, cols, 12.0);
    let cell_height = tiles
        .iter()
        .map(|(_, title, subtitle)| tile_height(ui, theme, cell, title, subtitle))
        .fold(0.0, f32::max);
    let rects = grid(ui, cols, 12.0, tiles.len(), cell_height);
    for (index, (glyph, title, subtitle)) in tiles.iter().enumerate() {
        rv(ui, 4.0 + index as f32 * 0.6, 22.0, |ui| {
            feature_tile(
                ui,
                theme,
                rects[index],
                Id::unique(("onboarding-tile", index)),
                glyph,
                title,
                subtitle,
            )
        });
    }
    ui.add_space(8.0);
}

fn language_picker(ui: &mut Ui, theme: &Theme, bridge: &mut AppBridge) {
    section_label(ui, theme, &tr("Язык интерфейса", &[]));
    let codes = bridge.available_languages();
    let labels = bridge.language_labels();
    let current = bridge.language().to_owned();
    let size = theme.font_normal as f32;
    let sizes: Vec<Vec2> = codes
        .iter()
        .zip(&labels)
        .map(|(code, label)| {
            let font = if *code == current {
                semibold(size)
            } else {
                FontId::proportional(size)
            };
            vec2(lay(ui, label, font, theme.text).size().x + 28.0, 34.0)
        })
        .collect();
    let origin = ui.cursor().min;
    let (rects, total) = flow_layout(origin, ui.available_width(), 8.0, &sizes);
    ui.allocate_exact_size(vec2(ui.available_width(), total), Sense::hover());
    for (index, (code, label)) in codes.iter().zip(&labels).enumerate() {
        if chip(
            ui,
            theme,
            rects[index],
            Id::unique(("onboarding-language", index)),
            label,
            *code == current,
        ) {
            bridge.set_language(code);
        }
    }
}

fn route(ui: &mut Ui, theme: &Theme, bridge: &mut AppBridge) {
    header(
        ui,
        theme,
        &tr("Регион и маршрутизация", &[]),
        &tr(
            "Регион определяет списки заблокированных ресурсов и быстрые пресеты маршрутов",
            &[],
        ),
    );
    let busy = bridge.regional_preset_busy();
    let label_top = ui.cursor().min;
    let label_start = ui.painter().add(egui::Shape::Noop).0;
    let label_width = section_label(ui, theme, &tr("Региональный профиль", &[]));
    if busy {
        // RowLayout { SectionLabel; BusyIndicator { topMargin: 14 } }: the
        // 18 px ring starts at the label's text top, 5 px after it.
        crate::shell::widgets::busy_indicator(
            ui,
            Rect::from_min_size(
                pos2(label_top.x + label_width + 5.0, label_top.y + 22.0),
                vec2(18.0, 18.0),
            ),
            theme.accent,
        );
    }
    let label_end = ui.painter().add(egui::Shape::Noop).0;
    REVEAL_BLOCKS.with(|blocks| blocks.borrow_mut().push((label_start, label_end, 1.0, 0.0)));
    let grid_start = ui.painter().add(egui::Shape::Noop).0;
    let current = bridge.regional_preset();
    let regions = [("russia", "RU"), ("china", "CN"), ("iran", "IR")];
    let width = ui.available_width();
    let cols = if width >= 520.0 { 3 } else { 1 };
    let cell = grid_cell_width(width, cols, 10.0);
    let titles: Vec<String> = regions
        .iter()
        .map(|(key, _)| tr(region_label(key), &[]))
        .collect();
    let cards: Vec<Choice> = regions
        .iter()
        .zip(&titles)
        .map(|((key, code), title)| Choice {
            glyph: "",
            flag: Some(*code),
            title,
            subtitle: "",
            recommended: false,
            selected: current == *key,
            enabled: !busy,
        })
        .collect();
    let cell_height = cards
        .iter()
        .map(|card| card.height(ui, theme, cell))
        .fold(0.0, f32::max);
    let rects = grid(ui, cols, 10.0, cards.len(), cell_height);
    for (index, card) in cards.iter().enumerate() {
        let key = regions[index].0;
        if card.show(ui, theme, rects[index], Id::unique(("onboarding-region", index)))
            && current != key
        {
            bridge.set_regional_preset(key);
        }
    }
    let grid_end = ui.painter().add(egui::Shape::Noop).0;
    REVEAL_BLOCKS.with(|blocks| blocks.borrow_mut().push((grid_start, grid_end, 1.0, 16.0)));

    rv(ui, 2.0, 0.0, |ui| section_label(ui, theme, &tr("Что пускать через VPN", &[])));
    let presets_start = ui.painter().add(egui::Shape::Noop).0;
    let active = bridge.active_routing_preset_id();
    let presets: Vec<(String, String, String, &'static str, bool)> = bridge
        .regional_routing_presets()
        .into_iter()
        .map(|item| {
            let id = item["id"].as_str().unwrap_or_default().to_owned();
            let name = item["name"].as_str().unwrap_or_default().to_owned();
            let description = match id.as_str() {
                "global" => "Весь трафик идёт через VPN",
                "blocked" | "blocked_cn" => {
                    "Через VPN — только заблокированные сайты, остальное напрямую"
                }
                "except_ru" => "Через VPN всё, кроме российских сайтов и сервисов",
                "except_cn" => "Через VPN всё, кроме китайских сайтов и сервисов",
                "except_ir" => "Через VPN всё, кроме иранских сайтов и сервисов",
                _ => "",
            };
            let glyph = match id.as_str() {
                "global" => "\u{e774}",
                "blocked" | "blocked_cn" => "\u{e72e}",
                _ => "\u{e909}",
            };
            let description = if description.is_empty() {
                String::new()
            } else {
                tr(description, &[])
            };
            let recommended = id == "blocked" || id == "blocked_cn";
            (id, name, description, glyph, recommended)
        })
        .collect();
    let width = ui.available_width();
    let cards: Vec<Choice> = presets
        .iter()
        .map(|(id, name, description, glyph, recommended)| Choice {
            glyph,
            flag: None,
            title: name,
            subtitle: description,
            recommended: *recommended,
            selected: active == *id,
            enabled: true,
        })
        .collect();
    let cell_height = cards
        .iter()
        .map(|card| card.height(ui, theme, width))
        .fold(0.0, f32::max);
    let rects = grid(ui, 1, 10.0, cards.len(), cell_height);
    for (index, card) in cards.iter().enumerate() {
        if card.show(ui, theme, rects[index], Id::unique(("onboarding-routing", index))) {
            bridge.apply_routing_preset_option(&presets[index].0);
        }
    }
    let presets_end = ui.painter().add(egui::Shape::Noop).0;
    REVEAL_BLOCKS.with(|blocks| blocks.borrow_mut().push((presets_start, presets_end, 2.0, 16.0)));
    ui.add_space(8.0);
}

fn system(ui: &mut Ui, theme: &Theme, bridge: &mut AppBridge) {
    header(
        ui,
        theme,
        &tr("Обновления, запуск и диагностика", &[]),
        &tr("Выберите, как Lumen будет обновляться и запускаться.", &[]),
    );
    rv(ui, 1.0, 0.0, |ui| section_label(ui, theme, &tr("Обновления", &[])));
    let check = bridge.check_updates();
    let auto = bridge.app_auto_update();
    if let Some((index, value)) = rv(ui, 1.5, 16.0, |ui| option_group(
        ui,
        theme,
        "onboarding-updates",
        &[
            Toggle {
                glyph: "\u{e777}",
                title: "Проверять обновления",
                text: "Автоматически проверять наличие обновлений",
                checked: check,
                enabled: true,
            },
            Toggle {
                glyph: "\u{e895}",
                title: "Автообновление приложения",
                text: "Автоматически скачивать и устанавливать новые версии",
                checked: auto,
                enabled: true,
            },
        ],
    )) {
        if index == 0 {
            bridge.set_check_updates(value);
        } else {
            bridge.set_app_auto_update(value);
        }
    }
    rv(ui, 2.0, 0.0, |ui| section_label(ui, theme, &tr("Запуск", &[])));
    let startup = bridge.launch_on_startup();
    let tray = bridge.launch_in_tray_on_startup();
    if let Some((index, value)) = rv(ui, 2.5, 16.0, |ui| option_group(
        ui,
        theme,
        "onboarding-startup",
        &[
            Toggle {
                glyph: "\u{e7e8}",
                title: "Запускать при входе",
                text: "Автозапуск вместе с Windows",
                checked: startup,
                enabled: true,
            },
            Toggle {
                glyph: "\u{e8a7}",
                title: "Запускать автозапуск в трей",
                text: "При входе в Windows не показывать окно, только иконку в трее",
                checked: tray,
                enabled: startup,
            },
        ],
    )) {
        if index == 0 {
            bridge.set_launch_on_startup(value);
        } else {
            bridge.set_launch_in_tray_on_startup(value);
        }
    }
    rv(ui, 3.0, 0.0, |ui| section_label(ui, theme, &tr("Диагностика", &[])));
    let diagnostics = bridge.diagnostics_upload_enabled();
    if let Some((_, value)) = rv(ui, 3.5, 16.0, |ui| option_group(
        ui,
        theme,
        "onboarding-diagnostics",
        &[Toggle {
            glyph: "\u{e9d9}",
            title: "Анонимная диагностика",
            text: "Отправлять отчёты об ошибках без личных данных",
            checked: diagnostics,
            enabled: true,
        }],
    )) {
        bridge.set_diagnostics_upload(value);
    }
    ui.add_space(8.0);
}

pub(super) fn import_clipboard(bridge: &mut AppBridge) {
    if let Ok(mut clipboard) = arboard::Clipboard::new() {
        if let Ok(files) = clipboard.get().file_list()
            && files.iter().any(|path| has_supported_config_drop(path))
        {
            bridge.import_node_files(&files);
            return;
        }
        bridge.import_clipboard_text(&clipboard.get_text().unwrap_or_default());
    } else {
        bridge.import_clipboard_text("");
    }
}

pub(super) fn import_config_file(bridge: &mut AppBridge) -> anyhow::Result<()> {
    let filter = "VPN configs (*.conf *.ovpn *.txt *.json *.yaml *.yml)\0*.conf;*.ovpn;*.txt;*.json;*.yaml;*.yml\0OpenVPN profiles (*.ovpn)\0*.ovpn\0All files (*.*)\0*.*\0";
    if let Some(path) =
        win_native::pick_file(Path::new(""), &tr("Импортировать сервер", &[]), filter)?
    {
        bridge.import_node_files(&[path]);
    }
    Ok(())
}

fn theme_label(mode: &str) -> &'static str {
    match mode {
        "light" => "Светлая",
        "dark" => "Тёмная",
        _ => "Авто",
    }
}

fn region_label(region: &str) -> &'static str {
    match region {
        "china" => "Китай",
        "iran" => "Иран",
        _ => "Россия",
    }
}

// ── layout primitives ───────────────────────────────────────────────────

/// `Math.round(size * Theme.fontScale)`.
/// StepPage `rv(order)`: page content fades in from the top down. The QML
/// pages number their blocks by hand (about one per 56 px); here the order
/// follows from where a shape sits in the page.
thread_local! {
    /// Reveal blocks recorded while a page renders: shape range, order, slide.
    static REVEAL_BLOCKS: std::cell::RefCell<Vec<(usize, usize, f32, f32)>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// A QML block with `opacity: page.rv(order)` and
/// `transform: Translate { y: (1 - page.rv(order)) * dy }` (`dy` 0 for the
/// section labels, which only fade).
fn rv<R>(ui: &mut Ui, order: f32, dy: f32, add: impl FnOnce(&mut Ui) -> R) -> R {
    let start = ui.painter().add(egui::Shape::Noop).0;
    let result = add(ui);
    let end = ui.painter().add(egui::Shape::Noop).0;
    REVEAL_BLOCKS.with(|blocks| blocks.borrow_mut().push((start, end, order, dy)));
    result
}

/// Applies the recorded blocks of the page just rendered (`revealAnim`:
/// `reveal` runs 0 -> 1 linearly over 1100 ms). Blocks are always drained;
/// a leaving page keeps `reveal` at 1 as in QML.
fn apply_reveal(ctx: &Context, layer_id: egui::LayerId, reveal: Option<f32>) {
    let blocks = REVEAL_BLOCKS.with(|blocks| std::mem::take(&mut *blocks.borrow_mut()));
    let Some(reveal) = reveal.filter(|reveal| *reveal < 1.0) else {
        return;
    };
    ctx.graphics_mut(|graphics| {
        let list = graphics.entry(layer_id);
        for (start, end, order, dy) in blocks {
            let level = reveal_level(reveal, order);
            if level >= 1.0 {
                continue;
            }
            let offset = egui::emath::TSTransform::from_translation(vec2(0.0, (1.0 - level) * dy));
            for index in start..end {
                list.mutate_shape(egui::layers::ShapeIdx(index), |clipped| {
                    if dy != 0.0 {
                        clipped.shape.transform(offset);
                    }
                    egui::epaint::shape_transform::adjust_colors(&mut clipped.shape, move |color| {
                        *color = color.gamma_multiply(level);
                    });
                });
            }
        }
    });
}

#[allow(dead_code)]
fn stagger_reveal(
    ctx: &Context,
    layer_id: egui::LayerId,
    first: egui::layers::ShapeIdx,
    content_top: f32,
    reveal: f32,
) {
    ctx.graphics_mut(|graphics| {
        let list = graphics.entry(layer_id);
        let end = list.next_idx().0;
        for index in first.0..end {
            list.mutate_shape(egui::layers::ShapeIdx(index), |clipped| {
                let top = clipped.shape.visual_bounding_rect().top();
                if !top.is_finite() {
                    return;
                }
                let level = reveal_level(reveal, ((top - content_top) / 56.0).max(0.0));
                if level < 1.0 {
                    egui::epaint::shape_transform::adjust_colors(&mut clipped.shape, move |color| {
                        *color = color.gamma_multiply(level);
                    });
                }
            });
        }
    });
}

/// `1 - (1 - clamp((reveal - order * 0.085) / 0.42))^3`.
fn reveal_level(reveal: f32, order: f32) -> f32 {
    motion::ease_out_cubic((reveal - order * 0.085) / 0.42)
}

fn px(theme: &Theme, size: f32) -> f32 {
    (size * theme.font_scale as f32).round()
}

fn ease_out_back(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0) - 1.0;
    1.0 + 2.70158 * t * t * t + 1.70158 * t * t
}

fn with_alpha(color: Color32, alpha: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(
        color.r(),
        color.g(),
        color.b(),
        (alpha.clamp(0.0, 1.0) * 255.0).round() as u8,
    )
}

fn lay(ui: &Ui, text: &str, font: FontId, color: Color32) -> Arc<Galley> {
    ui.painter().layout_no_wrap(text.to_owned(), font, color)
}

/// Wrapped text; `line` is the QML `lineHeight` multiplier.
fn lay_wrapped(
    ui: &Ui,
    text: &str,
    font: FontId,
    color: Color32,
    width: f32,
    line: f32,
) -> Arc<Galley> {
    let natural = lay(ui, "Ag", font.clone(), color).size().y;
    let mut job = LayoutJob::simple(text.to_owned(), font, color, width.max(1.0));
    if (line - 1.0).abs() > f32::EPSILON {
        job.sections[0].format.line_height = Some((natural * line).round());
    }
    ui.painter().layout_job(job)
}

fn paragraph(ui: &mut Ui, text: &str, font: FontId, color: Color32, line: f32) {
    let width = ui.available_width();
    let galley = lay_wrapped(ui, text, font, color, width, line);
    let (rect, _) = ui.allocate_exact_size(vec2(width, galley.size().y), Sense::hover());
    ui.painter().galley(rect.min, galley, color);
}

/// `PageHeader`: 24pt semibold title and a normal muted subtitle.
/// `PageHeader`: one block, `rv(0)`, 16 px.
fn header(ui: &mut Ui, theme: &Theme, title: &str, subtitle: &str) {
    rv(ui, 0.0, 16.0, |ui| page_header(ui, theme, title, subtitle));
}

fn page_header(ui: &mut Ui, theme: &Theme, title: &str, subtitle: &str) {
    paragraph(ui, title, semibold(px(theme, 24.0)), theme.text, 1.0);
    if !subtitle.is_empty() {
        ui.add_space(6.0);
        paragraph(
            ui,
            subtitle,
            FontId::proportional(theme.font_normal as f32),
            theme.text_muted,
            1.15,
        );
    }
}

/// Large welcome/news/done title with a 15pt muted intro.
/// The title and intro are separate blocks: `rv(first)` and `rv(first + 1)`,
/// each sliding `dy` px.
#[allow(clippy::too_many_arguments)]
fn hero_header(ui: &mut Ui, theme: &Theme, title: &str, intro: &str, size: f32, gap: f32, first: f32, dy: f32) {
    rv(ui, first, dy, |ui| paragraph(ui, title, semibold(px(theme, size)), theme.text, 1.0));
    ui.add_space(gap);
    rv(ui, first + 1.0, dy, |ui| {
        paragraph(
            ui,
            intro,
            FontId::proportional(px(theme, 15.0)),
            theme.text_muted,
            1.2,
        )
    });
}

/// `SectionLabel`: uppercase muted small semibold, 22/10 margins.
fn section_label(ui: &mut Ui, theme: &Theme, text: &str) -> f32 {
    ui.add_space(22.0);
    let mut job = LayoutJob::simple_singleline(
        text.to_uppercase(),
        semibold(theme.font_small as f32),
        theme.text_muted,
    );
    job.sections[0].format.extra_letter_spacing = 0.6;
    let galley = ui.painter().layout_job(job);
    let (rect, _) = ui.allocate_exact_size(galley.size(), Sense::hover());
    let width = galley.size().x;
    ui.painter().galley(rect.min, galley, theme.text_muted);
    ui.add_space(10.0);
    width
}

fn grid_cell_width(avail: f32, cols: usize, gap: f32) -> f32 {
    let cols = cols.max(1) as f32;
    ((avail - gap * (cols - 1.0)) / cols).max(0.0)
}

/// Equal-width cells (QML `GridLayout` with `fillWidth` children).
fn grid_rects(avail: f32, cols: usize, gap: f32, cell_height: f32, count: usize) -> (Vec<Rect>, f32) {
    let cols = cols.max(1);
    let cell_width = grid_cell_width(avail, cols, gap);
    let rects = (0..count)
        .map(|index| {
            let (column, row) = (index % cols, index / cols);
            Rect::from_min_size(
                pos2(
                    column as f32 * (cell_width + gap),
                    row as f32 * (cell_height + gap),
                ),
                vec2(cell_width, cell_height),
            )
        })
        .collect();
    let rows = count.div_ceil(cols);
    let height = if rows == 0 {
        0.0
    } else {
        rows as f32 * cell_height + (rows - 1) as f32 * gap
    };
    (rects, height)
}

/// Allocates the whole grid and returns the absolute cell rectangles.
fn grid(ui: &mut Ui, cols: usize, gap: f32, count: usize, cell_height: f32) -> Vec<Rect> {
    let avail = ui.available_width();
    let (rects, height) = grid_rects(avail, cols, gap, cell_height, count);
    let (area, _) = ui.allocate_exact_size(vec2(avail, height), Sense::hover());
    rects
        .into_iter()
        .map(|rect| rect.translate(area.min.to_vec2()))
        .collect()
}

/// Left-to-right wrapping flow (QML `Flow`); returns rectangles and total height.
fn flow_layout(origin: Pos2, avail: f32, gap: f32, sizes: &[Vec2]) -> (Vec<Rect>, f32) {
    let (mut x, mut y, mut row_height) = (0.0_f32, 0.0_f32, 0.0_f32);
    let mut rects = Vec::with_capacity(sizes.len());
    for size in sizes {
        if x > 0.0 && x + size.x > avail + 0.01 {
            x = 0.0;
            y += row_height + gap;
            row_height = 0.0;
        }
        rects.push(Rect::from_min_size(pos2(origin.x + x, origin.y + y), *size));
        x += size.x + gap;
        row_height = row_height.max(size.y);
    }
    let height = if sizes.is_empty() { 0.0 } else { y + row_height };
    (rects, height)
}

/// Filled box with the border inside the rectangle (QML `Rectangle.border`).
fn box_fill(painter: &Painter, rect: Rect, radius: f32, fill: Color32, border: Color32, width: f32) {
    if width > 0.0 {
        painter.rect_filled(rect.shrink(width), (radius - width).max(0.0), fill);
        painter.rect_stroke(rect, radius, Stroke::new(width, border), StrokeKind::Inside);
    } else {
        painter.rect_filled(rect, radius, fill);
    }
}

fn card_radius(theme: &Theme) -> f32 {
    theme.radius as f32 + 2.0
}

fn accent_tint(theme: &Theme) -> Color32 {
    with_alpha(theme.accent, if theme.dark { 0.10 } else { 0.06 })
}

fn pointer_on_hover(ui: &Ui, response: &egui::Response) {
    if response.hovered() {
        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
    }
}

// ── buttons ─────────────────────────────────────────────────────────────

/// Width `FluentButton` gives a button with this content.
fn button_width(ui: &Ui, theme: &Theme, glyph: &str, text: &str, kind: ButtonKind) -> f32 {
    let size = theme.font_normal as f32;
    let glyph_width = if glyph.is_empty() {
        0.0
    } else {
        lay(ui, glyph, FontId::proportional(size), theme.text).size().x
    };
    let font = if kind == ButtonKind::Ghost {
        FontId::proportional(size)
    } else {
        semibold(size)
    };
    let label_width = if text.is_empty() {
        0.0
    } else {
        lay(ui, text, font, theme.text).size().x
    };
    let gap = if glyph_width > 0.0 && label_width > 0.0 { 8.0 } else { 0.0 };
    glyph_width + gap + label_width + 28.0
}

fn put_button(ui: &mut Ui, theme: &Theme, rect: Rect, button: FluentButton<'_>) -> egui::Response {
    // `new_child` leaves the parent cursor alone, unlike `scope_builder`.
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(Layout::top_down(Align::Min)),
    );
    button.show(&mut child, theme)
}

struct Btn<'a> {
    kind: ButtonKind,
    glyph: &'a str,
    text: &'a str,
    enabled: bool,
}

fn button_sizes(ui: &Ui, theme: &Theme, buttons: &[Btn<'_>]) -> Vec<Vec2> {
    let height = theme.control_height as f32;
    buttons
        .iter()
        .map(|button| vec2(button_width(ui, theme, button.glyph, button.text, button.kind), height))
        .collect()
}

/// A wrapping row of buttons with 8pt gaps; returns the clicked index.
fn button_flow(ui: &mut Ui, theme: &Theme, buttons: &[Btn<'_>]) -> Option<usize> {
    let avail = ui.available_width();
    let sizes = button_sizes(ui, theme, buttons);
    let (rects, total) = flow_layout(Pos2::ZERO, avail, 8.0, &sizes);
    let (area, _) = ui.allocate_exact_size(vec2(avail, total), Sense::hover());
    let mut clicked = None;
    for (index, (button, rect)) in buttons.iter().zip(rects).enumerate() {
        let rect = rect.translate(area.min.to_vec2());
        let response = put_button(
            ui,
            theme,
            rect,
            FluentButton::new(button.kind, button.glyph, button.text)
                .width(rect.width())
                .enabled(button.enabled),
        );
        if response.clicked() {
            clicked = Some(index);
        }
    }
    clicked
}

fn chip(ui: &mut Ui, theme: &Theme, rect: Rect, id: Id, label: &str, selected: bool) -> bool {
    let ctx = ui.ctx().clone();
    let response = ui.interact(rect, id, Sense::click());
    pointer_on_hover(ui, &response);
    let scale = motion::follow(
        &ctx,
        theme,
        id.with("scale"),
        if response.is_pointer_button_down_on() { 0.96 } else { 1.0 },
        theme.anim_fast as f32,
        motion::linear,
    );
    let target = if selected {
        theme.accent
    } else if response.hovered() {
        theme.card_hover
    } else {
        theme.card
    };
    let fill = motion::follow_color(&ctx, theme, id.with("fill"), target, theme.anim_normal as f32, motion::linear);
    let body = Rect::from_center_size(rect.center(), rect.size() * scale);
    box_fill(
        ui.painter(),
        body,
        body.height() / 2.0,
        fill,
        theme.divider,
        if selected { 0.0 } else { 1.0 },
    );
    let color = if selected { theme.accent_text } else { theme.text };
    let font = if selected {
        semibold(theme.font_normal as f32)
    } else {
        FontId::proportional(theme.font_normal as f32)
    };
    let galley = lay(ui, label, font, color);
    ui.painter()
        .galley(body.center() - galley.size() / 2.0, galley, color);
    response.clicked()
}

// ── brand ───────────────────────────────────────────────────────────────

/// `LumenMark`: the "L" polygon wiped in top to bottom.
fn lumen_mark(painter: &Painter, rect: Rect, ink: Color32, level: f32) {
    let k = rect.width() / 100.0;
    let at = |x: f32, y: f32| pos2(rect.left() + x * k, rect.top() + y * k);
    let reveal = rect.top() + rect.height() * (level * 1.25).min(1.0);
    let clip = Rect::from_min_max(rect.min, pos2(rect.right(), reveal)).intersect(painter.clip_rect());
    let painter = painter.with_clip_rect(clip);
    // The concave outline is split into a stem and a foot (both convex).
    painter.add(Shape::convex_polygon(
        vec![at(24.0, 24.0), at(37.5, 10.0), at(37.5, 75.0), at(24.0, 75.0)],
        ink,
        Stroke::NONE,
    ));
    painter.add(Shape::convex_polygon(
        vec![at(24.0, 74.0), at(85.0, 74.0), at(72.5, 87.5), at(24.0, 87.5)],
        ink,
        Stroke::NONE,
    ));
}

// ── cards ───────────────────────────────────────────────────────────────

/// `ChoiceCard`: icon/flag + title + subtitle + radio indicator.
struct Choice<'a> {
    glyph: &'a str,
    flag: Option<&'a str>,
    title: &'a str,
    subtitle: &'a str,
    recommended: bool,
    selected: bool,
    enabled: bool,
}

impl Choice<'_> {
    fn has_icon(&self) -> bool {
        self.flag.is_some() || !self.glyph.is_empty()
    }

    fn text_width(&self, width: f32) -> f32 {
        let icon = if self.has_icon() { 36.0 + 14.0 } else { 0.0 };
        (width - 32.0 - icon - 14.0 - 20.0).max(1.0)
    }

    fn badge(&self, ui: &Ui, theme: &Theme) -> Option<Arc<Galley>> {
        self.recommended.then(|| {
            lay(
                ui,
                &tr("Рекомендуем", &[]),
                semibold(px(theme, 11.0)),
                theme.accent,
            )
        })
    }

    fn text_height(&self, ui: &Ui, theme: &Theme, width: f32) -> f32 {
        let title = lay(ui, self.title, semibold(theme.font_normal as f32), theme.text);
        let badge = self.badge(ui, theme).map_or(0.0, |galley| galley.size().y + 4.0);
        let mut height = title.size().y.max(badge);
        if !self.subtitle.is_empty() {
            let subtitle = lay_wrapped(
                ui,
                self.subtitle,
                FontId::proportional(theme.font_small as f32),
                theme.text_muted,
                self.text_width(width),
                1.0,
            );
            height += 3.0 + subtitle.size().y;
        }
        height
    }

    fn height(&self, ui: &Ui, theme: &Theme, width: f32) -> f32 {
        (self.text_height(ui, theme, width).max(36.0) + 26.0).max(60.0)
    }

    fn show(&self, ui: &mut Ui, theme: &Theme, rect: Rect, id: Id) -> bool {
        let ctx = ui.ctx().clone();
        let response = ui.interact(rect, id, if self.enabled { Sense::click() } else { Sense::hover() });
        let hovered = self.enabled && response.hovered();
        let down = self.enabled && response.is_pointer_button_down_on();
        if hovered {
            pointer_on_hover(ui, &response);
        }
        let scale = motion::follow(
            &ctx,
            theme,
            id.with("scale"),
            if down { 0.985 } else { 1.0 },
            theme.anim_fast as f32,
            motion::ease_out_cubic,
        );
        let body = Rect::from_center_size(rect.center(), rect.size() * scale);
        let fill_target = if self.selected {
            accent_tint(theme)
        } else if down {
            theme.card_pressed
        } else if hovered {
            theme.card_hover
        } else {
            theme.card
        };
        let ms = theme.anim_normal as f32;
        let fill = motion::follow_color(&ctx, theme, id.with("fill"), fill_target, ms, motion::linear);
        let border = motion::follow_color(
            &ctx,
            theme,
            id.with("border"),
            if self.selected { theme.accent } else { theme.divider },
            ms,
            motion::linear,
        );
        let mut painter = ui.painter().clone();
        if !self.enabled {
            painter.multiply_opacity(0.55);
        }
        box_fill(
            &painter,
            body,
            card_radius(theme),
            fill,
            border,
            if self.selected { 2.0 } else { 1.0 },
        );

        let center_y = body.center().y;
        let mut x = body.left() + 16.0;
        if let Some(code) = self.flag {
            let center = pos2(x + 18.0, center_y);
            let mut flag_ui = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(Rect::from_center_size(center, vec2(30.0, 22.0)))
                    .layout(Layout::top_down(Align::Min)),
            );
            if !self.enabled {
                // `opacity: enabled ? 1 : 0.55` covers the whole card.
                flag_ui.multiply_opacity(0.55);
            }
            flag::show(&mut flag_ui, code, vec2(30.0, 22.0), flag::FlagBorder::None);
            painter.rect_stroke(
                Rect::from_center_size(center, vec2(32.0, 24.0)),
                3.0,
                Stroke::new(1.0, theme.divider),
                StrokeKind::Inside,
            );
            x += 36.0 + 14.0;
        } else if !self.glyph.is_empty() {
            let icon = Rect::from_min_size(pos2(x, center_y - 18.0), vec2(36.0, 36.0));
            let icon_fill = motion::follow_color(
                &ctx,
                theme,
                id.with("icon"),
                if self.selected { theme.accent } else { theme.control_fill },
                ms,
                motion::linear,
            );
            box_fill(
                &painter,
                icon,
                10.0,
                icon_fill,
                theme.divider,
                if self.selected { 0.0 } else { 1.0 },
            );
            let ink = if self.selected { theme.accent_text } else { theme.text_muted };
            let glyph = lay(ui, self.glyph, FontId::proportional(16.0), ink);
            painter.galley(icon.center() - glyph.size() / 2.0, glyph, ink);
            x += 36.0 + 14.0;
        }

        let text_width = self.text_width(body.width());
        let title = lay(ui, self.title, semibold(theme.font_normal as f32), theme.text);
        let badge = self.badge(ui, theme);
        let title_row = title
            .size()
            .y
            .max(badge.as_ref().map_or(0.0, |galley| galley.size().y + 4.0));
        let subtitle = (!self.subtitle.is_empty()).then(|| {
            lay_wrapped(
                ui,
                self.subtitle,
                FontId::proportional(theme.font_small as f32),
                theme.text_muted,
                text_width,
                1.0,
            )
        });
        let column = title_row + subtitle.as_ref().map_or(0.0, |galley| 3.0 + galley.size().y);
        let top = center_y - column / 2.0;
        let title_width = title.size().x;
        painter.galley(pos2(x, top + (title_row - title.size().y) / 2.0), title, theme.text);
        if let Some(badge) = badge {
            let pill = Rect::from_min_size(
                pos2(
                    x + title_width + 8.0,
                    top + (title_row - badge.size().y - 4.0) / 2.0,
                ),
                vec2(badge.size().x + 14.0, badge.size().y + 4.0),
            );
            painter.rect_filled(pill, pill.height() / 2.0, with_alpha(theme.accent, 0.18));
            painter.galley(pill.center() - badge.size() / 2.0, badge, theme.accent);
        }
        if let Some(subtitle) = subtitle {
            painter.galley(pos2(x, top + title_row + 3.0), subtitle, theme.text_muted);
        }

        // Radio indicator: the ring thickens to a dot when selected.
        let ring = motion::follow(
            &ctx,
            theme,
            id.with("ring"),
            if self.selected { 6.0 } else { 1.5 },
            ms,
            ease_out_back,
        )
        .max(0.5);
        painter.circle_stroke(
            pos2(body.right() - 16.0 - 10.0, center_y),
            // egui strokes circles outside the radius; the QML border is
            // inside the 20 px circle.
            10.0 - ring,
            Stroke::new(
                ring,
                if self.selected { theme.accent } else { theme.text_faint },
            ),
        );
        response.clicked()
    }
}

fn tile_height(ui: &Ui, theme: &Theme, width: f32, title: &str, subtitle: &str) -> f32 {
    let text_width = (width - 28.0 - 22.0 - 14.0).max(1.0);
    let title = lay(ui, title, semibold(theme.font_normal as f32), theme.text);
    let subtitle = lay_wrapped(
        ui,
        subtitle,
        FontId::proportional(theme.font_small as f32),
        theme.text_muted,
        text_width,
        1.0,
    );
    let glyph = lay(ui, "\u{e8bc}", FontId::proportional(18.0), theme.accent);
    (title.size().y + 3.0 + subtitle.size().y).max(2.0 + glyph.size().y) + 28.0
}

/// `FeatureTile`.
fn feature_tile(
    ui: &mut Ui,
    theme: &Theme,
    rect: Rect,
    id: Id,
    glyph: &str,
    title: &str,
    subtitle: &str,
) {
    let ctx = ui.ctx().clone();
    let response = ui.interact(rect, id, Sense::hover());
    let fill = motion::follow_color(
        &ctx,
        theme,
        id.with("fill"),
        if response.hovered() { theme.card_hover } else { theme.card },
        theme.anim_fast as f32,
        motion::linear,
    );
    box_fill(ui.painter(), rect, card_radius(theme), fill, theme.divider, 1.0);
    let text_width = (rect.width() - 28.0 - 22.0 - 14.0).max(1.0);
    let title = elided(
        ui,
        title,
        semibold(theme.font_normal as f32),
        theme.text,
        text_width,
    );
    let subtitle = lay_wrapped(
        ui,
        subtitle,
        FontId::proportional(theme.font_small as f32),
        theme.text_muted,
        text_width,
        1.0,
    );
    let icon = lay(ui, glyph, FontId::proportional(18.0), theme.accent);
    let column = title.size().y + 3.0 + subtitle.size().y;
    let row = column.max(2.0 + icon.size().y);
    let top = rect.center().y - row / 2.0;
    let x = rect.left() + 14.0;
    let title_height = title.size().y;
    ui.painter().galley(pos2(x, top + 2.0), icon, theme.accent);
    ui.painter().galley(pos2(x + 22.0 + 14.0, top), title, theme.text);
    ui.painter().galley(
        pos2(x + 22.0 + 14.0, top + title_height + 3.0),
        subtitle,
        theme.text_muted,
    );
}

/// `ThemeCard`: mini window preview, label and radio.
fn theme_card(
    ui: &mut Ui,
    theme: &Theme,
    rect: Rect,
    id: Id,
    mode: &str,
    label: &str,
    selected: bool,
) -> bool {
    let ctx = ui.ctx().clone();
    let response = ui.interact(rect, id, Sense::click());
    pointer_on_hover(ui, &response);
    let scale = motion::follow(
        &ctx,
        theme,
        id.with("scale"),
        if response.is_pointer_button_down_on() { 0.98 } else { 1.0 },
        theme.anim_fast as f32,
        motion::linear,
    );
    let body = Rect::from_center_size(rect.center(), rect.size() * scale);
    let border = motion::follow_color(
        &ctx,
        theme,
        id.with("border"),
        if selected { theme.accent } else { theme.divider },
        theme.anim_normal as f32,
        motion::linear,
    );
    box_fill(
        ui.painter(),
        body,
        card_radius(theme),
        if response.hovered() { theme.card_hover } else { theme.card },
        border,
        if selected { 2.0 } else { 1.0 },
    );

    let preview = Rect::from_min_size(
        body.min + vec2(10.0, 10.0),
        vec2(body.width() - 20.0, 62.0),
    );
    match mode {
        "system" => {
            mini_window(ui.painter(), theme, preview, false);
            let right = Rect::from_min_max(pos2(preview.center().x, preview.top()), preview.max);
            mini_window(&ui.painter().with_clip_rect(right.intersect(ui.clip_rect())), theme, preview, true);
        }
        other => mini_window(ui.painter(), theme, preview, other == "dark"),
    }

    let font = if selected {
        semibold(theme.font_normal as f32)
    } else {
        FontId::proportional(theme.font_normal as f32)
    };
    let text = lay(ui, label, font, theme.text);
    ui.painter().galley(
        pos2(body.left() + 12.0, body.bottom() - 11.0 - text.size().y),
        text,
        theme.text,
    );
    let ring = motion::follow(
        &ctx,
        theme,
        id.with("ring"),
        if selected { 5.0 } else { 1.5 },
        theme.anim_normal as f32,
        ease_out_back,
    )
    .max(0.5);
    ui.painter().circle_stroke(
        pos2(body.right() - 12.0 - 9.0, body.bottom() - 11.0 - 9.0),
        9.0 - ring,
        Stroke::new(
            ring,
            if selected { theme.accent } else { theme.text_faint },
        ),
    );
    response.clicked()
}

/// `MiniWindow`: fixed light/dark mock window with the accent bar.
fn mini_window(painter: &Painter, theme: &Theme, rect: Rect, dark: bool) {
    let pick = |dark_hex: [u8; 3], light_hex: [u8; 3]| {
        let [r, g, b] = if dark { dark_hex } else { light_hex };
        Color32::from_rgb(r, g, b)
    };
    box_fill(
        painter,
        rect,
        6.0,
        pick([0x20, 0x20, 0x20], [0xF3, 0xF3, 0xF3]),
        pick([0x3A, 0x3A, 0x3A], [0xD6, 0xD6, 0xD6]),
        1.0,
    );
    let (w, h) = (rect.width(), rect.height());
    let at = |x: f32, y: f32, width: f32, height: f32| {
        Rect::from_min_size(rect.min + vec2(x, y), vec2(width.max(0.0), height))
    };
    painter.rect_filled(
        at(6.0, 6.0, w * 0.22, h - 12.0),
        3.0,
        pick([0x2C, 0x2C, 0x2C], [0xFF, 0xFF, 0xFF]),
    );
    let left = w * 0.22 + 12.0;
    painter.rect_filled(at(left, 8.0, w * 0.42, 6.0), 3.0, theme.accent);
    let line = pick([0x4A, 0x4A, 0x4A], [0xDA, 0xDA, 0xDA]);
    painter.rect_filled(at(left, 20.0, w * 0.62 - 18.0, 5.0), 2.5, line);
    painter.rect_filled(at(left, 30.0, w * 0.46 - 18.0, 5.0), 2.5, line);
}

/// `PaletteSwatch`.
struct Swatch<'a> {
    label: &'a str,
    base: Color32,
    surface: Color32,
    tint: Color32,
    selected: bool,
}

impl Swatch<'_> {
    fn show(&self, ui: &mut Ui, theme: &Theme, rect: Rect, id: Id) -> bool {
        let ctx = ui.ctx().clone();
        let response = ui.interact(rect, id, Sense::click());
        pointer_on_hover(ui, &response);
        let scale = motion::follow(
            &ctx,
            theme,
            id.with("scale"),
            if response.is_pointer_button_down_on() {
                0.95
            } else if response.hovered() {
                1.04
            } else {
                1.0
            },
            theme.anim_fast as f32,
            motion::ease_out_cubic,
        );
        let boxed = Rect::from_min_size(rect.min, vec2(rect.width(), 56.0));
        let body = Rect::from_center_size(boxed.center(), boxed.size() * scale);
        let border = motion::follow_color(
            &ctx,
            theme,
            id.with("border"),
            if self.selected { theme.accent } else { theme.flyout_border },
            theme.anim_normal as f32,
            motion::linear,
        );
        let painter = ui.painter().clone();
        box_fill(
            &painter,
            body,
            theme.radius as f32 + 1.0,
            self.base,
            border,
            if self.selected { 2.0 } else { 1.0 },
        );
        let at = |x: f32, y: f32, w: f32, h: f32| {
            Rect::from_min_size(
                body.min + vec2(x, y) * scale,
                vec2(w.max(0.0), h) * scale,
            )
        };
        painter.rect_filled(at(8.0, 8.0, boxed.width() - 16.0, 18.0), 5.0, self.surface);
        painter.rect_filled(at(8.0, 32.0, 30.0, 14.0), 7.0, self.tint);
        painter.rect_filled(at(44.0, 36.0, boxed.width() - 52.0, 6.0), 3.0, self.surface);
        if self.selected {
            let center = pos2(body.right() - 4.0, body.top() + 4.0);
            painter.circle_filled(center, 10.0, theme.accent);
            let check = lay(ui, "\u{e73e}", FontId::proportional(10.0), theme.accent_text);
            painter.galley(center - check.size() / 2.0, check, theme.accent_text);
        }
        let color = if self.selected { theme.text } else { theme.text_muted };
        let label = elided(
            ui,
            self.label,
            FontId::proportional(theme.font_small as f32),
            color,
            rect.width(),
        );
        painter.galley(
            pos2(rect.center().x - label.size().x / 2.0, rect.top() + 56.0 + 6.0),
            label,
            color,
        );
        response.clicked()
    }
}

/// `AccentDot` / `CustomAccentDot`.
fn accent_dot(
    ui: &mut Ui,
    theme: &Theme,
    rect: Rect,
    id: Id,
    tint: Color32,
    selected: bool,
    custom: bool,
) -> egui::Response {
    let ctx = ui.ctx().clone();
    let response = ui.interact(rect, id, Sense::click());
    pointer_on_hover(ui, &response);
    let scale = motion::follow(
        &ctx,
        theme,
        id.with("scale"),
        if response.is_pointer_button_down_on() {
            0.88
        } else if response.hovered() {
            1.08
        } else {
            1.0
        },
        theme.anim_fast as f32,
        ease_out_back,
    );
    let ring = motion::follow_color(
        &ctx,
        theme,
        id.with("ring"),
        if selected { theme.text } else { Color32::TRANSPARENT },
        theme.anim_normal as f32,
        motion::linear,
    );
    let diameter = motion::follow(
        &ctx,
        theme,
        id.with("diameter"),
        if selected { 24.0 } else { 26.0 },
        theme.anim_normal as f32,
        motion::linear,
    );
    let center = rect.center();
    ui.painter()
        // 34 px circle with a 2 px border inside (egui strokes outside).
        .circle_stroke(center, 15.0, Stroke::new(2.0, ring));
    let radius = diameter / 2.0 * scale;
    if custom {
        ui.painter().circle_filled(
            center,
            radius,
            if selected { tint } else { theme.control_fill },
        );
        if !selected {
            ui.painter()
                .circle_stroke(center, radius - 1.0, Stroke::new(1.0, theme.divider));
        }
        let ink = if selected { theme.accent_text } else { theme.text_muted };
        let glyph = lay(
            ui,
            if selected { "\u{e70f}" } else { "\u{e710}" },
            FontId::proportional(if selected { 10.0 } else { 11.0 }),
            ink,
        );
        ui.painter().galley(center - glyph.size() / 2.0, glyph, ink);
    } else {
        ui.painter().circle_filled(center, radius, tint);
    }
    response
}

/// "What's new" group: one card with note rows and inset separators.
fn group_card(ui: &mut Ui, theme: &Theme, group: &serde_json::Value) {
    let width = ui.available_width();
    let items = group["items"].as_array().cloned().unwrap_or_default();
    let notes: Vec<(String, String, String)> = items
        .iter()
        .map(|item| {
            (
                item["glyph"].as_str().unwrap_or_default().to_owned(),
                tr(item["title"].as_str().unwrap_or_default(), &[]),
                tr(item["text"].as_str().unwrap_or_default(), &[]),
            )
        })
        .collect();
    let text_width = (width - 32.0 - 22.0 - 14.0).max(1.0);
    let heights: Vec<f32> = notes
        .iter()
        .map(|(glyph, title, text)| {
            let title = lay_wrapped(ui, title, semibold(theme.font_normal as f32), theme.text, text_width, 1.0);
            let text = lay_wrapped(
                ui,
                text,
                FontId::proportional(theme.font_small as f32),
                theme.text_muted,
                text_width,
                1.15,
            );
            let glyph = lay(
                ui,
                if glyph.is_empty() { "\u{e8bc}" } else { glyph.as_str() },
                FontId::proportional(18.0),
                theme.accent,
            );
            (title.size().y + 3.0 + text.size().y).max(2.0 + glyph.size().y) + 28.0
        })
        .collect();
    let total: f32 = heights.iter().sum();
    let (rect, _) = ui.allocate_exact_size(vec2(width, total), Sense::hover());
    box_fill(ui.painter(), rect, card_radius(theme), theme.card, theme.divider, 1.0);
    let mut y = rect.top();
    for (index, ((glyph, title, text), height)) in notes.iter().zip(&heights).enumerate() {
        let row = Rect::from_min_size(pos2(rect.left(), y), vec2(width, *height));
        if index > 0 {
            ui.painter().rect_filled(
                Rect::from_min_size(
                    pos2(row.left() + 16.0, row.top()),
                    vec2(row.width() - 32.0, 1.0),
                ),
                0.0,
                theme.divider,
            );
        }
        let title = lay_wrapped(ui, title, semibold(theme.font_normal as f32), theme.text, text_width, 1.0);
        let text = lay_wrapped(
            ui,
            text,
            FontId::proportional(theme.font_small as f32),
            theme.text_muted,
            text_width,
            1.15,
        );
        let column = title.size().y + 3.0 + text.size().y;
        let top = row.center().y - column / 2.0;
        let title_height = title.size().y;
        let x = row.left() + 16.0;
        if !glyph.is_empty() {
            let icon = lay(ui, glyph, FontId::proportional(18.0), theme.accent);
            ui.painter().galley(pos2(x, top + 2.0), icon, theme.accent);
        }
        ui.painter().galley(pos2(x + 22.0 + 14.0, top), title, theme.text);
        ui.painter()
            .galley(pos2(x + 22.0 + 14.0, top + title_height + 3.0), text, theme.text_muted);
        y += height;
    }
}

/// `SummaryRow`: glyph, muted key, semibold value and a hover chevron.
fn summary_row(
    ui: &mut Ui,
    theme: &Theme,
    rect: Rect,
    id: Id,
    glyph: &str,
    key: &str,
    value: &str,
) -> bool {
    let ctx = ui.ctx().clone();
    let response = ui.interact(rect, id, Sense::click());
    pointer_on_hover(ui, &response);
    let fast = theme.anim_fast as f32;
    let fill = motion::follow_color(
        &ctx,
        theme,
        id.with("fill"),
        if response.hovered() { theme.card_hover } else { theme.card },
        fast,
        motion::linear,
    );
    box_fill(ui.painter(), rect, card_radius(theme), fill, theme.divider, 1.0);
    let icon = lay(ui, glyph, FontId::proportional(16.0), theme.accent);
    let chevron_alpha = motion::toggle(&ctx, theme, id.with("chevron"), response.hovered(), fast, motion::linear);
    let chevron = lay(
        ui,
        "\u{e70f}",
        FontId::proportional(12.0),
        theme.text_faint.gamma_multiply(chevron_alpha),
    );
    let x = rect.left() + 14.0;
    let text_x = x + icon.size().x + 12.0;
    let text_width = (rect.right() - 14.0 - chevron.size().x - 12.0 - text_x).max(1.0);
    let key = lay(ui, key, FontId::proportional(theme.font_small as f32), theme.text_faint);
    let value = elided(ui, value, semibold(theme.font_normal as f32), theme.text, text_width);
    let column = key.size().y + 1.0 + value.size().y;
    let top = rect.center().y - column / 2.0;
    let key_height = key.size().y;
    ui.painter()
        .galley(pos2(x, rect.center().y - icon.size().y / 2.0), icon, theme.accent);
    ui.painter().galley(pos2(text_x, top), key, theme.text_faint);
    ui.painter()
        .galley(pos2(text_x, top + key_height + 1.0), value, theme.text);
    ui.painter().galley(
        pos2(
            rect.right() - 14.0 - chevron.size().x,
            rect.center().y - chevron.size().y / 2.0,
        ),
        chevron,
        theme.text_faint,
    );
    response.clicked()
}

// ── support card ────────────────────────────────────────────────────────

fn support_buttons<'a>(
    star: &'a str,
    donate: &'a str,
    alerts: &'a str,
    usdt: &'a str,
) -> ([Btn<'a>; 2], [Btn<'a>; 2]) {
    (
        [
            Btn { kind: ButtonKind::Accent, glyph: "\u{e734}", text: star, enabled: true },
            Btn { kind: ButtonKind::Ghost, glyph: "\u{eb52}", text: donate, enabled: true },
        ],
        [
            Btn { kind: ButtonKind::Ghost, glyph: "\u{e8f1}", text: alerts, enabled: true },
            Btn { kind: ButtonKind::Ghost, glyph: "\u{e8c8}", text: usdt, enabled: true },
        ],
    )
}

const SUPPORT_TEXT: &str = "Lumen бесплатный и с открытым кодом. Поставьте звезду на GitHub — так о проекте узнает больше людей. А если хотите помочь развитию — поддержите проект.";

fn flow_height(ui: &Ui, theme: &Theme, buttons: &[Btn<'_>], avail: f32) -> f32 {
    flow_layout(Pos2::ZERO, avail, 8.0, &button_sizes(ui, theme, buttons)).1
}

fn support_height(ui: &Ui, theme: &Theme, open: bool) -> f32 {
    let level = motion::toggle(
        ui.ctx(),
        theme,
        Id::unique("onboarding-support-open"),
        open,
        260.0,
        motion::ease_out_cubic,
    );
    let inner = ui.available_width() - 32.0;
    let title = lay(ui, &tr("Нравится Lumen?", &[]), semibold(theme.font_normal as f32), theme.text);
    let text = lay_wrapped(
        ui,
        &tr(SUPPORT_TEXT, &[]),
        FontId::proportional(theme.font_small as f32),
        theme.text_muted,
        inner,
        1.15,
    );
    let (star, donate, alerts, usdt) = (
        tr("Поставить звезду на GitHub", &[]),
        tr("Поддержать проект", &[]),
        "DonationAlerts".to_owned(),
        tr("Скопировать адрес USDT (TRC20)", &[]),
    );
    let (first, second) = support_buttons(&star, &donate, &alerts, &usdt);
    32.0 + title.size().y
        + 3.0
        + text.size().y
        + 12.0
        + flow_height(ui, theme, &first, inner)
        + level * (12.0 + flow_height(ui, theme, &second, inner))
}

/// `SupportCard`: GitHub star and donation actions.
fn support_card(ui: &mut Ui, theme: &Theme, bridge: &mut AppBridge, open: &mut bool) {
    let width = ui.available_width();
    let height = support_height(ui, theme, *open);
    let (rect, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
    let level = motion::toggle(
        ui.ctx(),
        theme,
        Id::unique("onboarding-support-open"),
        *open,
        260.0,
        motion::ease_out_cubic,
    );
    let painter = ui.painter().clone();
    box_fill(
        &painter,
        rect,
        card_radius(theme),
        with_alpha(theme.accent, if theme.dark { 0.08 } else { 0.05 }),
        with_alpha(theme.accent, 0.35),
        1.0,
    );
    let inner = rect.shrink(16.0);
    let title = lay(ui, &tr("Нравится Lumen?", &[]), semibold(theme.font_normal as f32), theme.text);
    let text = lay_wrapped(
        ui,
        &tr(SUPPORT_TEXT, &[]),
        FontId::proportional(theme.font_small as f32),
        theme.text_muted,
        inner.width(),
        1.15,
    );
    let title_height = title.size().y;
    let text_height = text.size().y;
    painter.galley(inner.min, title, theme.text);
    painter.galley(inner.min + vec2(0.0, title_height + 3.0), text, theme.text_muted);
    let first_top = inner.top() + title_height + 3.0 + text_height + 12.0;

    let (star, donate, alerts, usdt) = (
        tr("Поставить звезду на GitHub", &[]),
        tr("Поддержать проект", &[]),
        "DonationAlerts".to_owned(),
        tr("Скопировать адрес USDT (TRC20)", &[]),
    );
    let (first, second) = support_buttons(&star, &donate, &alerts, &usdt);
    let first_height = flow_height(ui, theme, &first, inner.width());

    let mut clicked_second = None;
    let mut first_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(Rect::from_min_size(
                pos2(inner.left(), first_top),
                vec2(inner.width(), first_height),
            ))
            .layout(Layout::top_down(Align::Min)),
    );
    let clicked_first = button_flow(&mut first_ui, theme, &first);
    if level > 0.001 {
        let second_top = first_top + first_height + 12.0;
        let mut second_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(Rect::from_min_size(
                    pos2(inner.left(), second_top),
                    vec2(inner.width(), 1000.0),
                ))
                .layout(Layout::top_down(Align::Min)),
        );
        second_ui.set_clip_rect(rect.intersect(ui.clip_rect()));
        second_ui.multiply_opacity(level);
        clicked_second = button_flow(&mut second_ui, theme, &second);
    }
    if clicked_first == Some(0) {
        open_url(ui, REPO_URL);
    }
    if clicked_first == Some(1) {
        *open = !*open;
    }
    if clicked_second == Some(0) {
        open_url(ui, DONATION_URL);
    }
    if clicked_second == Some(1)
        && let Some(address) = bridge.copy_text(USDT_ADDRESS)
    {
        ui.ctx().copy_text(address);
    }
}

// ── import status, reveal, toggles ─────────────────────────────────────

/// Spinner / success tick / wrapped status text on one row.
fn status_row(
    ui: &mut Ui,
    theme: &Theme,
    spinner: bool,
    tick: bool,
    text: &str,
    color: Color32,
) {
    let width = ui.available_width();
    let mut lead = 0.0;
    if spinner {
        lead += 18.0 + 8.0;
    }
    let tick_galley = tick.then(|| lay(ui, "\u{e73e}", FontId::proportional(14.0), theme.success));
    if let Some(galley) = &tick_galley {
        lead += galley.size().x + 8.0;
    }
    let galley = lay_wrapped(
        ui,
        text,
        FontId::proportional(theme.font_small as f32),
        color,
        (width - lead).max(1.0),
        1.0,
    );
    let height = galley
        .size()
        .y
        .max(if spinner { 18.0 } else { 0.0 })
        .max(tick_galley.as_ref().map_or(0.0, |g| g.size().y));
    let (row, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
    let mut x = row.left();
    if spinner {
        crate::shell::widgets::busy_indicator(
            ui,
            Rect::from_min_size(pos2(x, row.center().y - 9.0), vec2(18.0, 18.0)),
            theme.accent,
        );
        x += 18.0 + 8.0;
    }
    if let Some(tick) = tick_galley {
        let size = tick.size();
        ui.painter()
            .galley(pos2(x, row.center().y - size.y / 2.0), tick, theme.success);
        x += size.x + 8.0;
    }
    ui.painter()
        .galley(pos2(x, row.center().y - galley.size().y / 2.0), galley, color);
}

/// `Reveal`: a block that grows, fades and slides in when `shown` turns true.
/// Its natural height is remembered from the previous frame.
fn reveal(
    ui: &mut Ui,
    theme: &Theme,
    id: Id,
    shown: bool,
    gap: f32,
    add: impl FnOnce(&mut Ui),
) {
    let ctx = ui.ctx().clone();
    let level = motion::toggle(&ctx, theme, id.with("level"), shown, 420.0, motion::ease_out_cubic);
    if level <= 0.001 {
        return;
    }
    let stored = ctx
        .data(|data| data.get_temp::<f32>(id.with("height")))
        .unwrap_or(0.0);
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(vec2(width, (stored + gap) * level), Sense::hover());
    let top = rect.top() + gap * level - (1.0 - level) * 12.0;
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(Rect::from_min_size(pos2(rect.left(), top), vec2(width, 10_000.0)))
            .layout(Layout::top_down(Align::LEFT)),
    );
    child.set_clip_rect(rect.intersect(ui.clip_rect()));
    child.multiply_opacity(level);
    add(&mut child);
    let height = child.min_rect().height();
    if (height - stored).abs() > 0.5 {
        ctx.data_mut(|data| data.insert_temp(id.with("height"), height));
        ctx.request_repaint();
    }
}

struct Toggle<'a> {
    glyph: &'a str,
    title: &'a str,
    text: &'a str,
    checked: bool,
    enabled: bool,
}

/// `OptionGroup` of `ToggleRow`s; returns `(row, new value)` when one flipped.
fn option_group(ui: &mut Ui, theme: &Theme, salt: &str, rows: &[Toggle<'_>]) -> Option<(usize, bool)> {
    let width = ui.available_width();
    let text_width = (width - 16.0 - 12.0 - 22.0 - 14.0 - 14.0 - 52.0).max(1.0);
    let measured: Vec<(Arc<Galley>, Arc<Galley>)> = rows
        .iter()
        .map(|row| {
            (
                lay_wrapped(ui, &tr(row.title, &[]), semibold(theme.font_normal as f32), theme.text, text_width, 1.0),
                lay_wrapped(
                    ui,
                    &tr(row.text, &[]),
                    FontId::proportional(theme.font_small as f32),
                    theme.text_muted,
                    text_width,
                    1.15,
                ),
            )
        })
        .collect();
    let heights: Vec<f32> = measured
        .iter()
        .map(|(title, text)| (title.size().y + 3.0 + text.size().y).max(20.0) + 20.0)
        .collect();
    let (rect, _) = ui.allocate_exact_size(vec2(width, heights.iter().sum()), Sense::hover());
    box_fill(ui.painter(), rect, card_radius(theme), theme.card, theme.divider, 1.0);
    let ctx = ui.ctx().clone();
    let mut y = rect.top();
    let mut flipped = None;
    for (index, ((row, (title, text)), height)) in rows.iter().zip(measured).zip(&heights).enumerate() {
        let strip = Rect::from_min_size(pos2(rect.left(), y), vec2(width, *height));
        y += height;
        if index > 0 {
            ui.painter().rect_filled(
                Rect::from_min_size(
                    pos2(strip.left() + 16.0, strip.top()),
                    vec2(strip.width() - 32.0, 1.0),
                ),
                0.0,
                theme.divider,
            );
        }
        let opacity = motion::follow(
            &ctx,
            theme,
            Id::unique((salt, index, "opacity")),
            if row.enabled { 1.0 } else { 0.45 },
            theme.anim_normal as f32,
            motion::linear,
        );
        let mut painter = ui.painter().clone();
        painter.multiply_opacity(opacity);
        let icon = lay(ui, row.glyph, FontId::proportional(18.0), theme.accent);
        painter.galley(
            pos2(strip.left() + 16.0, strip.center().y - icon.size().y / 2.0),
            icon,
            theme.accent,
        );
        let column = title.size().y + 3.0 + text.size().y;
        let top = strip.center().y - column / 2.0;
        let title_height = title.size().y;
        let text_x = strip.left() + 16.0 + 22.0 + 14.0;
        painter.galley(pos2(text_x, top), title, theme.text);
        painter.galley(pos2(text_x, top + title_height + 3.0), text, theme.text_muted);
        let slot = Rect::from_min_size(
            pos2(strip.right() - 12.0 - 52.0, strip.center().y - 10.0),
            vec2(52.0, 20.0),
        );
        let mut switch_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(slot)
                .layout(Layout::top_down(Align::Min)),
        );
        if let Some(value) = switch_widget(&mut switch_ui, theme, row.checked, row.enabled) {
            flipped = Some((index, value));
        }
    }
    flipped
}

/// QML `Switch` as a 44×20 Fluent pill.
fn switch_widget(ui: &mut Ui, theme: &Theme, checked: bool, enabled: bool) -> Option<bool> {
    crate::shell::pages::settings::switch_widget(ui, theme, checked, enabled)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upgrade_without_setup_shows_only_news() {
        assert_eq!(steps(false, true), [Step::News]);
        assert_eq!(steps(true, true).first(), Some(&Step::News));
        assert_eq!(
            steps(true, false),
            [
                Step::Welcome,
                Step::Look,
                Step::Route,
                Step::Configs,
                Step::System,
                Step::Done
            ]
        );
    }

    #[test]
    fn grid_cells_are_equal_and_do_not_overlap() {
        let (rects, height) = grid_rects(616.0, 2, 12.0, 74.0, 4);
        assert_eq!(rects.len(), 4);
        assert_eq!(rects[0].width(), 302.0);
        assert_eq!(rects[1].left() - rects[0].right(), 12.0);
        assert_eq!(rects[2].top() - rects[0].bottom(), 12.0);
        assert_eq!(height, 160.0);
    }

    #[test]
    fn flow_wraps_when_the_next_item_does_not_fit() {
        let sizes = [vec2(92.0, 84.0); 4];
        let (rects, height) = flow_layout(Pos2::ZERO, 200.0, 10.0, &sizes);
        assert_eq!(rects[1].left(), 102.0);
        assert_eq!(rects[2].top(), 94.0);
        assert_eq!(height, 178.0);
    }
}
