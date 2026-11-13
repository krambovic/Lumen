//! Motion helpers for the QML `Behavior` / `NumberAnimation` ports.
//!
//! Durations are the literal QML milliseconds. Every helper collapses to the
//! target value when `Theme.animations` is off (`anim_fast == 0`).

use egui::{Color32, Context, Id};

use super::theme::Theme;

/// `Theme.easeStandard` (`Easing.OutCubic`).
pub(crate) fn ease_out_cubic(t: f32) -> f32 {
    1.0 - (1.0 - t.clamp(0.0, 1.0)).powi(3)
}

/// `Theme.easeEmphasized` (`Easing.OutQuint`).
pub(crate) fn ease_out_quint(t: f32) -> f32 {
    1.0 - (1.0 - t.clamp(0.0, 1.0)).powi(5)
}

/// `Easing.InCubic`.
pub(crate) fn ease_in_cubic(t: f32) -> f32 {
    t.clamp(0.0, 1.0).powi(3)
}

pub(crate) fn linear(t: f32) -> f32 {
    t.clamp(0.0, 1.0)
}

fn seconds(theme: &Theme, ms: f32) -> f32 {
    if theme.anim_fast > 0 { ms / 1000.0 } else { 0.0 }
}

#[derive(Clone, Copy)]
struct Follow {
    from: f32,
    to: f32,
    start: f64,
}

/// Animate towards `target`, restarting from the current value whenever the
/// target changes (QML `Behavior on <property>`).
pub(crate) fn follow(
    ctx: &Context,
    theme: &Theme,
    id: Id,
    target: f32,
    ms: f32,
    easing: fn(f32) -> f32,
) -> f32 {
    let duration = seconds(theme, ms);
    if duration <= 0.0 {
        return target;
    }
    let now = ctx.input(|input| input.time);
    let mut state = ctx.data(|data| data.get_temp::<Follow>(id)).unwrap_or(Follow {
        from: target,
        to: target,
        start: now - f64::from(duration),
    });
    let progress = |state: &Follow| ((now - state.start) as f32 / duration).clamp(0.0, 1.0);
    let value = |state: &Follow| state.from + (state.to - state.from) * easing(progress(state));
    if state.to != target {
        state = Follow {
            from: value(&state),
            to: target,
            start: now,
        };
    }
    let current = value(&state);
    if progress(&state) < 1.0 {
        ctx.request_repaint();
    }
    ctx.data_mut(|data| data.insert_temp(id, state));
    current
}

#[derive(Clone, Copy)]
struct ColorFollow {
    from: Color32,
    to: Color32,
    start: f64,
}

/// `Behavior on color { ColorAnimation }` towards an arbitrary target.
pub(crate) fn follow_color(
    ctx: &Context,
    theme: &Theme,
    id: Id,
    target: Color32,
    ms: f32,
    easing: fn(f32) -> f32,
) -> Color32 {
    let duration = seconds(theme, ms);
    if duration <= 0.0 {
        return target;
    }
    let now = ctx.input(|input| input.time);
    let mut state = ctx
        .data(|data| data.get_temp::<ColorFollow>(id))
        .unwrap_or(ColorFollow {
            from: target,
            to: target,
            start: now - f64::from(duration),
        });
    let progress = |state: &ColorFollow| ((now - state.start) as f32 / duration).clamp(0.0, 1.0);
    let value = |state: &ColorFollow| mix(state.from, state.to, easing(progress(state)));
    if state.to != target {
        state = ColorFollow {
            from: value(&state),
            to: target,
            start: now,
        };
    }
    let current = value(&state);
    if progress(&state) < 1.0 {
        ctx.request_repaint();
    }
    ctx.data_mut(|data| data.insert_temp(id, state));
    current
}

/// `Easing.InOutCubic`.
pub(crate) fn ease_in_out_cubic(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    if t < 0.5 {
        4.0 * t * t * t
    } else {
        1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
    }
}

/// 0..1 progress that follows a two-state property.
pub(crate) fn toggle(
    ctx: &Context,
    theme: &Theme,
    id: Id,
    on: bool,
    ms: f32,
    easing: fn(f32) -> f32,
) -> f32 {
    follow(ctx, theme, id, if on { 1.0 } else { 0.0 }, ms, easing)
}

/// Seconds since `value` last changed; a value seen for the first time counts
/// as changed now.
pub(crate) fn elapsed_since_change<T>(ctx: &Context, id: Id, value: T) -> f32
where
    T: PartialEq + Clone + Send + Sync + 'static,
{
    let now = ctx.input(|input| input.time);
    let previous = ctx.data(|data| data.get_temp::<(T, f64)>(id));
    match previous {
        Some((seen, since)) if seen == value => (now - since) as f32,
        _ => {
            ctx.data_mut(|data| data.insert_temp(id, (value, now)));
            0.0
        }
    }
}

/// Eased 0..1 progress of a one-shot animation `elapsed` seconds after its start.
pub(crate) fn progress(
    ctx: &Context,
    theme: &Theme,
    elapsed: f32,
    ms: f32,
    easing: fn(f32) -> f32,
) -> f32 {
    let duration = seconds(theme, ms);
    if duration <= 0.0 || elapsed >= duration {
        return 1.0;
    }
    ctx.request_repaint();
    easing(elapsed / duration)
}

pub(crate) fn mix(from: Color32, to: Color32, t: f32) -> Color32 {
    from.lerp_to_gamma(to, t.clamp(0.0, 1.0))
}

/// `Behavior on color { ColorAnimation }` between an idle and a hovered fill.
pub(crate) fn hover_color(
    ctx: &Context,
    theme: &Theme,
    id: Id,
    hovered: bool,
    idle: Color32,
    hot: Color32,
    ms: f32,
) -> Color32 {
    mix(idle, hot, toggle(ctx, theme, id, hovered, ms, ease_out_cubic))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn easings_cover_the_unit_interval() {
        for easing in [ease_out_cubic, ease_out_quint, linear] {
            assert_eq!(easing(0.0), 0.0);
            assert_eq!(easing(1.0), 1.0);
            assert_eq!(easing(2.0), 1.0);
            assert!(easing(0.5) >= 0.5);
        }
    }

    #[test]
    fn disabled_animations_jump_to_the_target() {
        let ctx = Context::default();
        let mut inputs = crate::shell::theme::ThemeInputs::default();
        inputs.animations = false;
        let theme = Theme::resolve(&inputs);
        assert_eq!(theme.anim_fast, 0);
        let id = Id::unique("motion-test");
        assert_eq!(follow(&ctx, &theme, id, 5.0, 200.0, linear), 5.0);
        assert_eq!(toggle(&ctx, &theme, id, true, 200.0, linear), 1.0);
        assert_eq!(progress(&ctx, &theme, 0.0, 200.0, linear), 1.0);
    }

    #[test]
    fn follow_restarts_from_the_current_value() {
        let ctx = Context::default();
        let theme = Theme::resolve(&crate::shell::theme::ThemeInputs::default());
        let id = Id::unique("motion-follow");
        assert_eq!(follow(&ctx, &theme, id, 0.0, 200.0, linear), 0.0);
        // The target changed in the same instant: the animation starts at 0.
        assert_eq!(follow(&ctx, &theme, id, 1.0, 200.0, linear), 0.0);
    }

    #[test]
    fn elapsed_resets_when_the_value_changes() {
        let ctx = Context::default();
        let id = Id::unique("motion-elapsed");
        assert_eq!(elapsed_since_change(&ctx, id, 1_u8), 0.0);
        assert_eq!(elapsed_since_change(&ctx, id, 2_u8), 0.0);
    }
}
