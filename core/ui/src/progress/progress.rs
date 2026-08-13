use crate::{ActiveTheme, Sizable, Size, StyledExt};
use gpui::{
    Animation, AnimationExt as _, App, Background, Corners, ElementId, Hsla,
    InteractiveElement as _, IntoElement, ParentElement, Pixels, RenderOnce, Role,
    StatefulInteractiveElement as _, StyleRefinement, Styled, Window, canvas, div, fill,
    prelude::FluentBuilder, px, relative,
};
use instant::Duration;

use super::ProgressState;

/// A linear horizontal progress bar element.
#[derive(IntoElement)]
pub struct Progress {
    id: ElementId,
    style: StyleRefinement,
    color: Option<Hsla>,
    value: f32,
    size: Size,
    loading: bool,
    wavy: bool,
}

impl Progress {
    /// Create a new Progress bar.
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            value: Default::default(),
            color: None,
            style: StyleRefinement::default(),
            size: Size::default(),
            loading: false,
            wavy: false,
        }
    }

    /// Enable indeterminate loading animation.
    ///
    /// When `loading` is `true`, the `value` is ignored and an infinite
    /// sliding animation is shown instead.
    pub fn loading(mut self, loading: bool) -> Self {
        self.loading = loading;
        self
    }

    /// Enable wavy progress bar.
    pub fn wavy(mut self, wavy: bool) -> Self {
        self.wavy = wavy;
        self
    }

    /// Set the color of the progress bar.
    pub fn color(mut self, color: impl Into<Hsla>) -> Self {
        self.color = Some(color.into());
        self
    }

    /// Set the percentage value of the progress bar.
    ///
    /// The value should be between 0.0 and 100.0.
    pub fn value(mut self, value: f32) -> Self {
        self.value = value.clamp(0., 100.);
        self
    }
}

impl Styled for Progress {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl Sizable for Progress {
    fn with_size(mut self, size: impl Into<Size>) -> Self {
        self.size = size.into();
        self
    }
}

impl RenderOnce for Progress {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let bg_color = self.color.unwrap_or(cx.theme().primary);
        let bg = Background::from(bg_color);
        let value = self.value;
        let loading = self.loading;
        let wavy = self.wavy;

        let radius = self.style.corner_radii.clone();
        let mut inner_style = StyleRefinement::default();
        inner_style.corner_radii = radius;
        let inner_style_loading = inner_style.clone();

        let (height, radius) = match self.size {
            Size::XSmall => (px(4.), px(2.)),
            Size::Small => (px(6.), px(3.)),
            Size::Medium => (px(8.), px(4.)),
            Size::Large => (px(10.), px(5.)),
            Size::Size(s) => (s, s / 2.),
        };

        let state = window.use_keyed_state(self.id.clone(), cx, |_, _| ProgressState::new(value));
        let prev_target = state.read(cx).target();
        let has_changed = prev_target != value;

        let start_time_state = window.use_keyed_state(
            ElementId::Name(format!("{}-start-time", self.id).into()),
            cx,
            |_, _| instant::Instant::now(),
        );
        let start_time = *start_time_state.read(cx);

        let sine_wave_path =
            |x_min: f32,
             x_max: f32,
             h: f32,
             stroke_w: f32,
             amp: f32,
             wave_len: f32,
             shift: f32,
             origin: gpui::Point<Pixels>|
             -> (gpui::Path<Pixels>, gpui::Point<Pixels>, gpui::Point<Pixels>) {
                let mut builder = gpui::PathBuilder::stroke(px(stroke_w));
                let center_y = h / 2.0;

                let y_val = |x_pos: f32| -> f32 {
                    center_y
                        + amp * ((2.0 * std::f32::consts::PI * (x_pos + shift)) / wave_len).sin()
                };

                let start_pt = gpui::Point::new(origin.x + px(x_min), origin.y + px(y_val(x_min)));
                let end_pt = gpui::Point::new(origin.x + px(x_max), origin.y + px(y_val(x_max)));

                if x_max <= x_min {
                    builder.move_to(start_pt);
                    return (builder.build().unwrap(), start_pt, end_pt);
                }

                let mut x = x_min;
                let step = 2.0f32; // 2px step for high smoothness

                builder.move_to(start_pt);
                while x < x_max {
                    x = (x + step).min(x_max);
                    builder.line_to(gpui::Point::new(origin.x + px(x), origin.y + px(y_val(x))));
                }

                (builder.build().unwrap(), start_pt, end_pt)
            };

        if wavy {
            let element = div()
                .id(self.id.clone())
                .role(Role::ProgressIndicator)
                .aria_numeric_value(value as f64)
                .aria_min_numeric_value(0.0)
                .aria_max_numeric_value(100.0)
                .w_full()
                .relative()
                .h(height)
                .refine_style(&self.style);

            let active_element = canvas(
                move |bounds: gpui::Bounds<Pixels>, _window: &mut Window, _cx: &mut App| bounds,
                move |_bounds, prepaint_bounds, window: &mut Window, _cx: &mut App| {
                    let width = prepaint_bounds.size.width.as_f32();
                    let h = prepaint_bounds.size.height.as_f32();
                    let stroke_w = (h * 0.5).min(4.0);
                    let amp = h * 0.3;
                    let wave_len = h * 3.0;

                    let time_sec = start_time.elapsed().as_secs_f32();
                    let shift = time_sec * wave_len;

                    let draw_cap =
                        |pt: gpui::Point<Pixels>, r: f32, color: Hsla, window: &mut Window| {
                            let size = r * 2.0;
                            let bounds = gpui::Bounds {
                                origin: gpui::Point::new(pt.x - px(r), pt.y - px(r)),
                                size: gpui::Size {
                                    width: px(size),
                                    height: px(size),
                                },
                            };
                            window
                                .paint_quad(fill(bounds, color).corner_radii(Corners::all(px(r))));
                        };

                    // Draw track (with 20% opacity)
                    let (track_path, track_start, track_end) = sine_wave_path(
                        0.0,
                        width,
                        h,
                        stroke_w * 0.75,
                        amp,
                        wave_len,
                        shift,
                        prepaint_bounds.origin,
                    );
                    window.paint_path(track_path, bg_color.opacity(0.2));
                    draw_cap(track_start, stroke_w * 0.375, bg_color.opacity(0.2), window);
                    draw_cap(track_end, stroke_w * 0.375, bg_color.opacity(0.2), window);

                    // Always draw the boundary dot at the end of the track (on the right) in active color (full opacity)
                    draw_cap(track_end, stroke_w * 0.5, bg_color, window);

                    if loading {
                        // Indeterminate segments
                        let cycle_ms = 1750.0;
                        let elapsed_ms = (time_sec * 1000.0) % cycle_ms;

                        let ease = crate::animation::cubic_bezier(0.3, 0.0, 0.8, 0.15);

                        // Line 1:
                        let l1_head = if elapsed_ms < 1000.0 {
                            ease(elapsed_ms / 1000.0)
                        } else {
                            1.0
                        };
                        let l1_tail = if elapsed_ms < 250.0 {
                            0.0
                        } else if elapsed_ms < 1250.0 {
                            ease((elapsed_ms - 250.0) / 1000.0)
                        } else {
                            1.0
                        };

                        // Line 2:
                        let l2_head = if elapsed_ms < 650.0 {
                            0.0
                        } else if elapsed_ms < 1500.0 {
                            ease((elapsed_ms - 650.0) / 850.0)
                        } else {
                            1.0
                        };
                        let l2_tail = if elapsed_ms < 900.0 {
                            0.0
                        } else if elapsed_ms < 1750.0 {
                            ease((elapsed_ms - 900.0) / 850.0)
                        } else {
                            1.0
                        };

                        // Line 1
                        if l1_head > l1_tail {
                            let (path1, start1, end1) = sine_wave_path(
                                width * l1_tail,
                                width * l1_head,
                                h,
                                stroke_w,
                                amp,
                                wave_len,
                                shift,
                                prepaint_bounds.origin,
                            );
                            window.paint_path(path1, bg_color);
                            draw_cap(start1, stroke_w * 0.5, bg_color, window);
                            draw_cap(end1, stroke_w * 0.5, bg_color, window);
                        }

                        // Line 2
                        if l2_head > l2_tail {
                            let (path2, start2, end2) = sine_wave_path(
                                width * l2_tail,
                                width * l2_head,
                                h,
                                stroke_w,
                                amp,
                                wave_len,
                                shift,
                                prepaint_bounds.origin,
                            );
                            window.paint_path(path2, bg_color);
                            draw_cap(start2, stroke_w * 0.5, bg_color, window);
                            draw_cap(end2, stroke_w * 0.5, bg_color, window);
                        }
                    } else {
                        // Determinate Active progress
                        let active_width = width * (value / 100.0);
                        if active_width > 0.0 {
                            let (active_path, active_start, active_end) = sine_wave_path(
                                0.0,
                                active_width,
                                h,
                                stroke_w,
                                amp,
                                wave_len,
                                shift,
                                prepaint_bounds.origin,
                            );
                            window.paint_path(active_path, bg_color);
                            draw_cap(active_start, stroke_w * 0.5, bg_color, window);
                            draw_cap(active_end, stroke_w * 0.5, bg_color, window);
                        }
                    }
                },
            )
            .absolute()
            .size_full();

            element
                .child(active_element)
                .with_animation(
                    "wavy-flow",
                    Animation::new(Duration::from_secs(100)).repeat(),
                    |this, _| this,
                )
                .into_any_element()
        } else {
            let element = div()
                .id(self.id.clone())
                .role(Role::ProgressIndicator)
                .aria_numeric_value(value as f64)
                .aria_min_numeric_value(0.0)
                .aria_max_numeric_value(100.0)
                .w_full()
                .relative()
                .h(height)
                .rounded(radius)
                .refine_style(&self.style)
                .bg(bg.opacity(0.2));

            let final_element = if loading {
                element
                    .child(div().size_full().relative().with_animation(
                        "progress-loading",
                        Animation::new(Duration::from_millis(1750)).repeat(),
                        move |this, delta| {
                            let time_ms = delta * 1750.0;
                            let ease = crate::animation::cubic_bezier(0.3, 0.0, 0.8, 0.15);

                            // Line 1:
                            let l1_head = if time_ms < 1000.0 {
                                ease(time_ms / 1000.0)
                            } else {
                                1.0
                            };
                            let l1_tail = if time_ms < 250.0 {
                                0.0
                            } else if time_ms < 1250.0 {
                                ease((time_ms - 250.0) / 1000.0)
                            } else {
                                1.0
                            };

                            // Line 2:
                            let l2_head = if time_ms < 650.0 {
                                0.0
                            } else if time_ms < 1500.0 {
                                ease((time_ms - 650.0) / 850.0)
                            } else {
                                1.0
                            };
                            let l2_tail = if time_ms < 900.0 {
                                0.0
                            } else if time_ms < 1750.0 {
                                ease((time_ms - 900.0) / 850.0)
                            } else {
                                1.0
                            };

                            this.child(
                                div()
                                    .absolute()
                                    .top_0()
                                    .left(relative(l1_tail))
                                    .w(relative(l1_head - l1_tail))
                                    .h_full()
                                    .bg(bg)
                                    .rounded(radius)
                                    .refine_style(&inner_style_loading),
                            )
                            .child(
                                div()
                                    .absolute()
                                    .top_0()
                                    .left(relative(l2_tail))
                                    .w(relative(l2_head - l2_tail))
                                    .h_full()
                                    .bg(bg)
                                    .rounded(radius)
                                    .refine_style(&inner_style_loading),
                            )
                        },
                    ))
                    .into_any_element()
            } else {
                element
                    .child(
                        div()
                            .absolute()
                            .top_0()
                            .left_0()
                            .h_full()
                            .bg(bg)
                            .rounded(radius)
                            .refine_style(&inner_style)
                            .map(|this| {
                                if has_changed {
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

                                    this.with_animation(
                                        "progress-animation",
                                        Animation::new(duration),
                                        move |this, delta| {
                                            let current_value = from + (value - from) * delta;
                                            let w = relative((current_value / 100.).clamp(0., 1.));
                                            this.w(w)
                                        },
                                    )
                                    .into_any_element()
                                } else {
                                    this.w(relative((value / 100.).clamp(0., 1.)))
                                        .into_any_element()
                                }
                            }),
                    )
                    .into_any_element()
            };
            final_element
        }
    }
}

#[cfg(test)]
impl Progress {
    pub(crate) fn is_loading(&self) -> bool {
        self.loading
    }

    pub(crate) fn get_value(&self) -> f32 {
        self.value
    }

    pub(crate) fn get_color(&self) -> Option<Hsla> {
        self.color
    }

    pub(crate) fn is_wavy(&self) -> bool {
        self.wavy
    }

    pub(crate) fn get_size(&self) -> Size {
        self.size
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_progress_builder() {
        let p = Progress::new("test-progress")
            .loading(true)
            .wavy(true)
            .color(gpui::blue())
            .value(45.5)
            .with_size(Size::Large);

        assert!(p.is_loading());
        assert!(p.is_wavy());
        assert_eq!(p.get_value(), 45.5);
        assert_eq!(p.get_color(), Some(gpui::blue()));
        assert_eq!(p.get_size(), Size::Large);
    }

    #[test]
    fn test_progress_value_clamping() {
        let p_under = Progress::new("test-p").value(-10.0);
        assert_eq!(p_under.get_value(), 0.0);

        let p_over = Progress::new("test-p").value(150.0);
        assert_eq!(p_over.get_value(), 100.0);
    }
}
