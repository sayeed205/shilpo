use std::f32::consts::TAU;

use gpui::prelude::FluentBuilder as _;
use gpui::{
    Animation, AnimationExt as _, AnyElement, App, ElementId, Hsla, InteractiveElement as _,
    IntoElement, ParentElement, Pixels, RenderOnce, StyleRefinement, Styled, Window, canvas, px,
    relative,
};
use gpui::{Bounds, Corners, div, fill};
use instant::Duration;

use super::ProgressState;
use crate::plot::shape::{Arc, ArcData};
use crate::{ActiveTheme, Sizable, Size, StyledExt};

#[derive(Clone)]
struct WaveMorphState {
    prev_wavy: bool,
    morph_start: instant::Instant,
}

/// A circular progress indicator element.
#[derive(IntoElement)]
pub struct ProgressCircle {
    id: ElementId,
    style: StyleRefinement,
    color: Option<Hsla>,
    value: f32,
    size: Size,
    stroke_width: Option<Pixels>,
    wave_speed: f32,
    children: Vec<AnyElement>,
    loading: bool,
    wavy: bool,
}

impl ProgressCircle {
    /// Create a new circular progress indicator.
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            value: Default::default(),
            color: None,
            style: StyleRefinement::default(),
            size: Size::default(),
            stroke_width: None,
            wave_speed: 1.0,
            children: Vec::new(),
            loading: false,
            wavy: false,
        }
    }

    /// Enable indeterminate loading animation.
    ///
    /// When `loading` is `true`, the `value` is ignored and an infinite
    /// rotating arc animation is shown instead.
    pub fn loading(mut self, loading: bool) -> Self {
        self.loading = loading;
        self
    }

    /// Enable wavy progress circle.
    pub fn wavy(mut self, wavy: bool) -> Self {
        self.wavy = wavy;
        self
    }

    /// Set wave animation speed (clamped between 0.0 and 1.0).
    pub fn wave_speed(mut self, speed: f32) -> Self {
        self.wave_speed = speed.clamp(0.0, 1.0);
        self
    }

    /// Set a custom stroke width for the progress circle.
    pub fn stroke_width(mut self, width: impl Into<Pixels>) -> Self {
        self.stroke_width = Some(width.into());
        self
    }

    /// Set the color of the progress circle.
    pub fn color(mut self, color: impl Into<Hsla>) -> Self {
        self.color = Some(color.into());
        self
    }

    /// Set the percentage value of the progress circle.
    ///
    /// The value should be between 0.0 and 100.0.
    pub fn value(mut self, value: f32) -> Self {
        self.value = value.clamp(0., 100.);
        self
    }

    /// Render the arc canvas. `start_value` and `end_value` are in 0.0–100.0 percentage.
    /// The progress arc is skipped when `end_value <= 0`.
    fn render_circle(
        start_value: f32,
        end_value: f32,
        color: Hsla,
        wave_shift: f32,
        custom_stroke_width: Option<Pixels>,
        wave_factor: f32,
    ) -> impl IntoElement {
        struct PrepaintState {
            start_value: f32,
            end_value: f32,
            actual_inner_radius: f32,
            actual_outer_radius: f32,
            bounds: Bounds<Pixels>,
        }

        canvas(
            move |bounds: Bounds<Pixels>, _window: &mut Window, _cx: &mut App| {
                let stroke_width =
                    custom_stroke_width.unwrap_or_else(|| (bounds.size.width * 0.15).min(px(5.)));
                let actual_size = bounds.size.width.min(bounds.size.height);
                let actual_radius = (actual_size.as_f32() - stroke_width.as_f32()) / 2.;
                PrepaintState {
                    start_value,
                    end_value,
                    actual_inner_radius: actual_radius - stroke_width.as_f32() / 2.,
                    actual_outer_radius: actual_radius + stroke_width.as_f32() / 2.,
                    bounds,
                }
            },
            move |_bounds, prepaint, window: &mut Window, _cx: &mut App| {
                let stroke_width = prepaint.actual_outer_radius - prepaint.actual_inner_radius;
                let actual_radius =
                    (prepaint.actual_inner_radius + prepaint.actual_outer_radius) / 2.0;
                let center_x = prepaint.bounds.origin.x + prepaint.bounds.size.width / 2.0;
                let center_y = prepaint.bounds.origin.y + prepaint.bounds.size.height / 2.0;

                let draw_cap = |angle: f32, r_val: f32, color: Hsla, window: &mut Window| {
                    let cx = center_x + px(r_val * angle.cos());
                    let cy = center_y + px(r_val * angle.sin());
                    let size = px(stroke_width);
                    let cap_r = px(stroke_width / 2.0);
                    let bounds = gpui::Bounds {
                        origin: gpui::Point::new(cx - cap_r, cy - cap_r),
                        size: gpui::Size {
                            width: size,
                            height: size,
                        },
                    };
                    window.paint_quad(fill(bounds, color).corner_radii(Corners::all(cap_r)));
                };

                if wave_factor > 0.001 {
                    let start_angle =
                        (prepaint.start_value / 100.) * TAU - std::f32::consts::FRAC_PI_2;
                    let end_angle = (prepaint.end_value / 100.) * TAU - std::f32::consts::FRAC_PI_2;

                    if end_angle > start_angle {
                        let step = 2.0 * std::f32::consts::PI / 180.0;
                        let mut theta = start_angle;

                        let num_waves = 10.0f32;
                        let amp = stroke_width * 0.4 * wave_factor;

                        let get_radius = |t: f32| -> f32 {
                            actual_radius + amp * (num_waves * (t - wave_shift)).cos()
                        };

                        let get_point = |t: f32| -> gpui::Point<Pixels> {
                            let r = get_radius(t);
                            gpui::Point::new(center_x + px(r * t.cos()), center_y + px(r * t.sin()))
                        };

                        let mut builder = gpui::PathBuilder::stroke(px(stroke_width));
                        builder.move_to(get_point(theta));
                        while theta < end_angle {
                            theta = (theta + step).min(end_angle);
                            builder.line_to(get_point(theta));
                        }

                        if let Ok(p) = builder.build() {
                            window.paint_path(p, color);
                        }

                        if (end_angle - start_angle).abs() < TAU - 0.001 {
                            draw_cap(start_angle, get_radius(start_angle), color, window);
                            draw_cap(end_angle, get_radius(end_angle), color, window);
                        }
                    }
                } else {
                    let arc = Arc::new()
                        .inner_radius(prepaint.actual_inner_radius)
                        .outer_radius(prepaint.actual_outer_radius);

                    arc.paint(
                        &ArcData {
                            data: &(),
                            index: 0,
                            value: 100.,
                            start_angle: 0.,
                            end_angle: TAU,
                            pad_angle: 0.,
                        },
                        color.opacity(0.2),
                        None,
                        None,
                        &prepaint.bounds,
                        window,
                    );

                    let start_angle = prepaint.start_value / 100. * TAU;
                    let end_angle = prepaint.end_value / 100. * TAU;

                    if end_angle > start_angle {
                        arc.paint(
                            &ArcData {
                                data: &(),
                                index: 0,
                                value: 100.,
                                start_angle,
                                end_angle,
                                pad_angle: 0.,
                            },
                            color,
                            None,
                            None,
                            &prepaint.bounds,
                            window,
                        );
                    }
                }
            },
        )
        .absolute()
        .inset_0()
    }
}

impl Styled for ProgressCircle {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl ParentElement for ProgressCircle {
    fn extend(&mut self, children: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(children);
    }
}

impl RenderOnce for ProgressCircle {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let value = self.value;
        let loading = self.loading;
        let wavy = self.wavy && (value >= 7.0 && value <= 97.0);
        let wave_speed = self.wave_speed.clamp(0.0, 1.0);

        let state = window.use_keyed_state(self.id.clone(), cx, |_, _| ProgressState::new(value));
        let prev_target = state.read(cx).target();
        let has_changed = prev_target != value;

        let color = self.color.unwrap_or(cx.theme().primary);

        let start_time_state = window.use_keyed_state(
            ElementId::Name(format!("{}-start-time", self.id).into()),
            cx,
            |_, _| instant::Instant::now(),
        );
        let start_time = *start_time_state.read(cx);
        let time_sec = start_time.elapsed().as_secs_f32();
        let wave_shift = time_sec * 2.0 * wave_speed;

        let morph_state = window.use_keyed_state(
            ElementId::Name(format!("{}-wave-morph", self.id).into()),
            cx,
            |_, _| WaveMorphState {
                prev_wavy: wavy,
                morph_start: instant::Instant::now() - Duration::from_secs(1),
            },
        );

        let mut morph = morph_state.read(cx).clone();
        if morph.prev_wavy != wavy {
            morph.prev_wavy = wavy;
            morph.morph_start = instant::Instant::now();
            morph_state.update(cx, |this, _| {
                *this = morph.clone();
            });
        }

        let elapsed = morph.morph_start.elapsed().as_secs_f32();
        let morph_duration = 0.35;
        let morph_progress = (elapsed / morph_duration).clamp(0.0, 1.0);
        let ease_progress = crate::animation::cubic_bezier(0.2, 0.0, 0.0, 1.0)(morph_progress);

        let wave_factor = if wavy {
            ease_progress
        } else {
            1.0 - ease_progress
        };

        let element = div()
            .id(self.id.clone())
            .relative()
            .flex()
            .items_center()
            .justify_center()
            .line_height(relative(1.))
            .map(|this| match self.size {
                Size::XSmall => this.size_2(),
                Size::Small => this.size_3(),
                Size::Medium => this.size_4(),
                Size::Large => this.size_5(),
                Size::Size(s) => this.size(s),
            })
            .refine_style(&self.style)
            .children(self.children);

        let final_element = if has_changed {
            let from = prev_target;
            state.read(cx).set_target(value);

            let duration = Duration::from_secs_f64(0.15);
            cx.spawn({
                let state = state.clone();
                async move |cx| {
                    cx.background_executor().timer(duration).await;
                    _ = state.update(cx, |this, _| {
                        this.value = this.target();
                    });
                }
            })
            .detach();

            let stroke_width = self.stroke_width;

            element
                .with_animation(
                    format!("progress-circle-{}", from),
                    Animation::new(duration),
                    move |this, delta| {
                        let v = from + (value - from) * delta;
                        this.child(Self::render_circle(
                            0.,
                            v,
                            color,
                            wave_shift + delta * 2.0 * wave_speed,
                            stroke_width,
                            wave_factor,
                        ))
                    },
                )
                .into_any_element()
        } else if loading {
            let stroke_width = self.stroke_width;

            element
                .with_animation(
                    "progress-circle-loading",
                    Animation::new(Duration::from_secs(6)).repeat(),
                    move |this, delta| {
                        let global_rotation = delta * 3.0 * TAU;
                        let mut additional_rotation = 0.0;
                        for i in 0..4 {
                            let step_start = i as f32 * 0.25;
                            let step_end = step_start + 0.05;
                            if delta >= step_end {
                                additional_rotation += std::f32::consts::FRAC_PI_2;
                            } else if delta > step_start {
                                let progress = (delta - step_start) / 0.05;
                                let ease_progress =
                                    crate::animation::cubic_bezier(0.05, 0.7, 0.1, 1.0)(progress);
                                additional_rotation += ease_progress * std::f32::consts::FRAC_PI_2;
                            }
                        }

                        let sweep_easing = crate::animation::cubic_bezier(0.2, 0.0, 0.0, 1.0);
                        let (start, end) = if delta < 0.5 {
                            let p = delta / 0.5;
                            let sweep = 0.1 + (0.87 - 0.1) * sweep_easing(p);
                            (0.0, sweep * 100.)
                        } else {
                            let p = (delta - 0.5) / 0.5;
                            let sweep = sweep_easing(p) * 0.77;
                            (sweep * 100., 0.87 * 100.)
                        };

                        let rotation_percentage =
                            (global_rotation + additional_rotation) / TAU * 100.;

                        this.child(Self::render_circle(
                            start + rotation_percentage,
                            end + rotation_percentage,
                            color,
                            wave_shift,
                            stroke_width,
                            wave_factor,
                        ))
                    },
                )
                .into_any_element()
        } else {
            let stroke_width = self.stroke_width;
            let morph_start = morph.morph_start;
            let morph_wavy = wavy;
            if wavy {
                element
                    .with_animation(
                        "wavy-circle-flow",
                        Animation::new(Duration::from_secs(100)).repeat(),
                        move |this, delta| {
                            let elapsed = morph_start.elapsed().as_secs_f32();
                            let morph_progress = (elapsed / 0.35).clamp(0.0, 1.0);
                            let ease =
                                crate::animation::cubic_bezier(0.2, 0.0, 0.0, 1.0)(morph_progress);
                            let wave_factor = if morph_wavy { ease } else { 1.0 - ease };
                            this.child(Self::render_circle(
                                0.,
                                value,
                                color,
                                wave_shift + delta * 2.0 * wave_speed,
                                stroke_width,
                                wave_factor,
                            ))
                        },
                    )
                    .into_any_element()
            } else if morph_progress < 1.0 {
                element
                    .with_animation(
                        "wavy-circle-morph",
                        Animation::new(Duration::from_secs_f32(morph_duration)),
                        move |this, delta| {
                            let ease = crate::animation::cubic_bezier(0.2, 0.0, 0.0, 1.0)(delta);
                            let factor = 1.0 - ease;
                            this.child(Self::render_circle(
                                0.,
                                value,
                                color,
                                wave_shift + delta * 2.0 * wave_speed,
                                stroke_width,
                                factor,
                            ))
                        },
                    )
                    .into_any_element()
            } else {
                element
                    .child(Self::render_circle(
                        0.,
                        value,
                        color,
                        wave_shift,
                        stroke_width,
                        0.0,
                    ))
                    .into_any_element()
            }
        };

        final_element
    }
}

impl Sizable for ProgressCircle {
    fn with_size(mut self, size: impl Into<Size>) -> Self {
        self.size = size.into();
        self
    }
}
