use crate::{ActiveTheme, Sizable, Size, StyledExt};
use gpui::{
    Animation, AnimationExt as _, App, ElementId, Hsla, InteractiveElement as _, IntoElement,
    ParentElement as _, Pixels, RenderOnce, StyleRefinement, Styled, Window, canvas, div,
    prelude::FluentBuilder as _, px,
};
use instant::Duration;
use std::f32::consts::TAU;

use crate::motion::ExpressiveSpring;

/// A Material Design 3 Expressive morphing star loading indicator.
#[derive(IntoElement)]
pub struct LoadingIndicator {
    id: ElementId,
    style: StyleRefinement,
    color: Option<Hsla>,
    contained: bool,
    container_color: Option<Hsla>,
    size: Size,
}

impl LoadingIndicator {
    /// Create a new LoadingIndicator.
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            style: StyleRefinement::default(),
            color: None,
            contained: false,
            container_color: None,
            size: Size::Medium,
        }
    }

    /// Enable contained style with a background.
    pub fn contained(mut self, contained: bool) -> Self {
        self.contained = contained;
        self
    }

    /// Set the container background color (only applicable when contained).
    pub fn container_color(mut self, color: impl Into<Hsla>) -> Self {
        self.container_color = Some(color.into());
        self
    }

    /// Set the loading indicator color.
    pub fn color(mut self, color: impl Into<Hsla>) -> Self {
        self.color = Some(color.into());
        self
    }
}

impl Styled for LoadingIndicator {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl Sizable for LoadingIndicator {
    fn with_size(mut self, size: impl Into<Size>) -> Self {
        self.size = size.into();
        self
    }
}

fn get_shape_radius(shape_idx: usize, theta: f32) -> f32 {
    match shape_idx {
        0 => 0.85 + 0.15 * (10.0 * theta).cos(), // SoftBurst (10 lobes)
        1 => 0.85 + 0.15 * (9.0 * theta).cos(),  // Cookie9Sided (9 lobes)
        2 => 0.85 + 0.15 * (5.0 * theta).cos(),  // Pentagon (5 lobes)
        3 => 0.82 + 0.18 * (2.0 * theta).cos(),  // Pill (2 lobes)
        4 => 0.85 + 0.15 * (8.0 * theta).cos(),  // Sunny (8 lobes)
        5 => 0.82 + 0.18 * (4.0 * theta).cos(),  // Cookie4Sided (4 lobes)
        6 => 0.82 + 0.18 * (2.0 * theta + std::f32::consts::FRAC_PI_2).cos(), // Oval (2 lobes, phase rotated)
        _ => 1.0,
    }
}

impl RenderOnce for LoadingIndicator {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let indicator_color = self.color.unwrap_or(cx.theme().primary);
        let contained = self.contained;

        let (container_size, active_size, default_container_bg) = match self.size {
            Size::XSmall => (px(24.), px(18.), cx.theme().primary_container.opacity(0.4)),
            Size::Small => (px(36.), px(28.), cx.theme().primary_container.opacity(0.5)),
            Size::Medium => (px(48.), px(38.), cx.theme().primary_container),
            Size::Large => (px(64.), px(52.), cx.theme().primary_container),
            Size::Size(s) => (s, s * 0.8, cx.theme().primary_container),
        };

        let container_bg = self.container_color.unwrap_or(default_container_bg);

        let start_time_state = window.use_keyed_state(
            ElementId::Name(format!("{}-start-time", self.id).into()),
            cx,
            |_, _| instant::Instant::now(),
        );
        let start_time = *start_time_state.read(cx);

        div()
            .id(self.id.clone())
            .size(container_size)
            .relative()
            .flex()
            .items_center()
            .justify_center()
            .when(contained, |this| {
                this.bg(container_bg)
                    .rounded_full()
                    .refine_style(&self.style)
            })
            .child(
                canvas(
                    move |_bounds: gpui::Bounds<Pixels>, _window: &mut Window, _cx: &mut App| {
                        _bounds
                    },
                    move |_bounds, prepaint_bounds, window: &mut Window, _cx: &mut App| {
                        let center_x = prepaint_bounds.origin.x + prepaint_bounds.size.width / 2.0;
                        let center_y = prepaint_bounds.origin.y + prepaint_bounds.size.height / 2.0;

                        let actual_size = prepaint_bounds
                            .size
                            .width
                            .min(prepaint_bounds.size.height)
                            .as_f32();
                        let radius_outer = actual_size * 0.5;

                        // Animations:
                        let time_sec = start_time.elapsed().as_secs_f32();

                        // 1. Global Rotation: 360 degrees every 4.666 seconds
                        let global_rotation = (time_sec / 4.666) * TAU;

                        // 2. Morph sequence indices and spring interpolation:
                        // Interval of 650ms (0.65 seconds)
                        let total_intervals = time_sec / 0.65;
                        let current_idx = (total_intervals.floor() as usize) % 7;
                        let next_idx = (current_idx + 1) % 7;

                        let interval_t = total_intervals - total_intervals.floor();

                        // AndroidX spring config: dampingRatio = 0.6, stiffness = 200.0
                        let spring = ExpressiveSpring {
                            damping: 0.6,
                            stiffness: 200.0,
                        };
                        let spring_p = spring.evaluate(interval_t * 0.65);

                        // Coerced progress for morphing (clamped 0..1 to prevent geometry self-intersection)
                        let morph_p = spring_p.clamp(0.0, 1.0);

                        // Rotation inherits the spring overshoot (bouncy step rotation)
                        let rotation_target_angle =
                            (current_idx as f32) * std::f32::consts::FRAC_PI_2;
                        let rotation = global_rotation
                            + rotation_target_angle
                            + spring_p * std::f32::consts::FRAC_PI_2;

                        // Build the morphing star path
                        let mut builder = gpui::PathBuilder::fill();

                        let num_points = 180;
                        let step = TAU / (num_points as f32);

                        // Start vertex
                        let r_start = get_shape_radius(current_idx, 0.0);
                        let r_end = get_shape_radius(next_idx, 0.0);
                        let r = r_start + (r_end - r_start) * morph_p;
                        let pt_radius = r * radius_outer;

                        let start_x = center_x + px(pt_radius * rotation.cos());
                        let start_y = center_y + px(pt_radius * rotation.sin());
                        builder.move_to(gpui::Point::new(start_x, start_y));

                        for j in 1..num_points {
                            let angle = (j as f32) * step;
                            let r_start = get_shape_radius(current_idx, angle);
                            let r_end = get_shape_radius(next_idx, angle);
                            let r = r_start + (r_end - r_start) * morph_p;
                            let pt_radius = r * radius_outer;

                            let x = center_x + px(pt_radius * (angle + rotation).cos());
                            let y = center_y + px(pt_radius * (angle + rotation).sin());
                            builder.line_to(gpui::Point::new(x, y));
                        }

                        if let Ok(path) = builder.build() {
                            window.paint_path(path, indicator_color);
                        }
                    },
                )
                .absolute()
                .size(active_size),
            )
            // Infinite repeating dummy animation to drive 60 FPS redraw
            .with_animation(
                "loading-indicator-flow",
                Animation::new(Duration::from_secs(100)).repeat(),
                |this, _| this,
            )
    }
}
