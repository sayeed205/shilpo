use std::time::{Duration, Instant};

use gpui::{
    App, Bounds, Corners, Element, ElementId, Entity, Hsla, IntoElement, LayoutId, PaintQuad,
    Pixels, Point, Window, px,
};

pub fn is_point_in_rounded_bounds(
    point: Point<Pixels>,
    bounds: Bounds<Pixels>,
    corners: Corners<Pixels>,
) -> bool {
    if !bounds.contains(&point) {
        return false;
    }

    let px = f32::from(point.x);
    let py = f32::from(point.y);
    let x = f32::from(bounds.origin.x);
    let y = f32::from(bounds.origin.y);
    let w = f32::from(bounds.size.width);
    let h = f32::from(bounds.size.height);

    let max_r = (w * 0.5).min(h * 0.5);
    let r_tl = f32::from(corners.top_left).min(max_r);
    let r_tr = f32::from(corners.top_right).min(max_r);
    let r_br = f32::from(corners.bottom_right).min(max_r);
    let r_bl = f32::from(corners.bottom_left).min(max_r);

    // Top-Left Corner
    if px < x + r_tl && py < y + r_tl {
        let dx = px - (x + r_tl);
        let dy = py - (y + r_tl);
        return (dx * dx + dy * dy) <= (r_tl * r_tl);
    }

    // Top-Right Corner
    if px > x + w - r_tr && py < y + r_tr {
        let dx = px - (x + w - r_tr);
        let dy = py - (y + r_tr);
        return (dx * dx + dy * dy) <= (r_tr * r_tr);
    }

    // Bottom-Right Corner
    if px > x + w - r_br && py > y + h - r_br {
        let dx = px - (x + w - r_br);
        let dy = py - (y + h - r_br);
        return (dx * dx + dy * dy) <= (r_br * r_br);
    }

    // Bottom-Left Corner
    if px < x + r_bl && py > y + h - r_bl {
        let dx = px - (x + r_bl);
        let dy = py - (y + h - r_bl);
        return (dx * dx + dy * dy) <= (r_bl * r_bl);
    }

    true
}

#[derive(Clone, Copy)]
pub struct ActiveRipple {
    pub start_time: Instant,
    pub press_position: Point<Pixels>,
}

pub struct RippleState {
    pub ripples: Vec<ActiveRipple>,
    pub is_pressed: bool,
    pub press_start_time: Option<Instant>,
    pub release_time: Option<Instant>,
    pub last_bounds: Option<Bounds<Pixels>>,
    pub last_corners: Option<Corners<Pixels>>,
}

impl Default for RippleState {
    fn default() -> Self {
        Self::new()
    }
}

impl RippleState {
    pub fn new() -> Self {
        Self {
            ripples: Vec::new(),
            is_pressed: false,
            press_start_time: None,
            release_time: None,
            last_bounds: None,
            last_corners: None,
        }
    }

    pub fn is_point_inside(&self, point: Point<Pixels>) -> bool {
        if let (Some(bounds), Some(corners)) = (self.last_bounds, self.last_corners) {
            is_point_in_rounded_bounds(point, bounds, corners)
        } else {
            true
        }
    }

    pub fn current_spring_progress(&self) -> f32 {
        if self.is_pressed {
            if let Some(start) = self.press_start_time {
                let elapsed = start.elapsed().as_secs_f32();
                return crate::motion::ExpressiveSpring::fast_spatial()
                    .evaluate(elapsed)
                    .clamp(0.0, 1.0);
            }
            return 1.0;
        }

        if let Some(rel) = self.release_time {
            let elapsed = rel.elapsed().as_secs_f32();
            if elapsed < 0.3 {
                let unspring = crate::motion::ExpressiveSpring::fast_spatial().evaluate(elapsed);
                return (1.0 - unspring).clamp(0.0, 1.0);
            }
        }

        0.0
    }

    pub fn handle_mouse_down(state: Entity<Self>, press_position: Point<Pixels>, cx: &mut App) {
        if !state.read(cx).is_point_inside(press_position) {
            return;
        }

        _ = state.update(cx, |this, _| {
            this.is_pressed = true;
            this.press_start_time = Some(Instant::now());
            this.release_time = None;
            this.ripples.push(ActiveRipple {
                start_time: Instant::now(),
                press_position,
            });
        });

        // Spawn a background timer loop to animate the ripple frames at 60fps (16ms)
        cx.spawn({
            let state = state.clone();
            async move |cx| {
                loop {
                    cx.background_executor()
                        .timer(Duration::from_millis(16))
                        .await;
                    let finished = cx.update(|cx| {
                        state.update(cx, |this, cx| {
                            // Retain ripples until release + fadeout is completed
                            this.ripples.retain(|r| {
                                let elapsed = r.start_time.elapsed().as_secs_f32();
                                if this.is_pressed {
                                    true
                                } else if let Some(rel) = this.release_time {
                                    rel.elapsed().as_secs_f32() < 0.150
                                } else {
                                    elapsed < 0.375
                                }
                            });
                            cx.notify();

                            let unspring_done = match this.release_time {
                                Some(rel) => rel.elapsed().as_secs_f32() >= 0.150,
                                None => false,
                            };

                            this.ripples.is_empty() && !this.is_pressed && unspring_done
                        })
                    });
                    if finished {
                        break;
                    }
                }
            }
        })
        .detach();
    }

    pub fn handle_mouse_up(state: Entity<Self>, cx: &mut App) {
        _ = state.update(cx, |this, cx| {
            this.is_pressed = false;
            this.release_time = Some(Instant::now());
            cx.notify();
        });
    }

    pub fn start_ripple(state: Entity<Self>, press_position: Point<Pixels>, cx: &mut App) {
        Self::handle_mouse_down(state, press_position, cx);
    }
}

pub struct RippleElement<E: Element + 'static> {
    pub child: E,
    pub state: Entity<RippleState>,
    pub corner_radii: Corners<Pixels>,
    pub color: Option<Hsla>,
}

impl<E: Element + 'static> RippleElement<E> {
    pub fn new(child: E, state: Entity<RippleState>) -> Self {
        Self {
            child,
            state,
            corner_radii: Corners::all(px(0.)),
            color: None,
        }
    }

    pub fn corner_radii(mut self, corner_radii: Corners<Pixels>) -> Self {
        self.corner_radii = corner_radii;
        self
    }

    pub fn color(mut self, color: Hsla) -> Self {
        self.color = Some(color);
        self
    }
}

impl<E: Element + 'static> IntoElement for RippleElement<E> {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl<E: Element + 'static> Element for RippleElement<E> {
    type RequestLayoutState = E::RequestLayoutState;
    type PrepaintState = E::PrepaintState;

    fn id(&self) -> Option<ElementId> {
        self.child.id()
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        self.child.source_location()
    }

    fn request_layout(
        &mut self,
        global_id: Option<&gpui::GlobalElementId>,
        inspector_id: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        self.child
            .request_layout(global_id, inspector_id, window, cx)
    }

    fn prepaint(
        &mut self,
        global_id: Option<&gpui::GlobalElementId>,
        inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        self.child
            .prepaint(global_id, inspector_id, bounds, request_layout, window, cx)
    }

    fn paint(
        &mut self,
        global_id: Option<&gpui::GlobalElementId>,
        inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        request_layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        // Record bounds and corner_radii for rounded shape hit-testing
        _ = self.state.update(cx, |this, _| {
            this.last_bounds = Some(bounds);
            this.last_corners = Some(self.corner_radii);
        });

        // Paint the child first, so the ripple is overlaid on top of it.
        self.child.paint(
            global_id,
            inspector_id,
            bounds,
            request_layout,
            prepaint,
            window,
            cx,
        );

        // Retrieve active ripples and press state
        let (ripples, is_pressed, release_time) = {
            let state = self.state.read(cx);
            (state.ripples.clone(), state.is_pressed, state.release_time)
        };

        if !ripples.is_empty() {
            use crate::theme::ActiveTheme;
            let base_color = self.color.unwrap_or_else(|| cx.theme().on_surface);

            window.with_content_mask(Some(gpui::ContentMask { bounds }), |window| {
                for ripple in ripples {
                    let elapsed = ripple.start_time.elapsed().as_secs_f32();

                    // Material 3 specs:
                    // - FadeInDuration = 75ms (linear)
                    // - RadiusDuration = 225ms (ease-out cubic / FastOutSlowIn)
                    // - FadeOutDuration = 150ms (linear, starting on release)
                    let radius_progress = (elapsed / 0.225).clamp(0.0, 1.0);
                    let fade_in_progress = (elapsed / 0.075).clamp(0.0, 1.0);

                    let fade_out_progress = if !is_pressed {
                        if let Some(rel) = release_time {
                            (rel.elapsed().as_secs_f32() / 0.150).clamp(0.0, 1.0)
                        } else if elapsed > 0.225 {
                            ((elapsed - 0.225) / 0.150).clamp(0.0, 1.0)
                        } else {
                            0.0
                        }
                    } else {
                        0.0
                    };

                    let alpha = fade_in_progress * (1.0 - fade_out_progress) * 0.16;
                    if alpha <= 0.001 {
                        continue;
                    }

                    // EaseOut cubic for radius expansion: 1 - (1 - t)^3
                    let eased_radius_progress = 1.0 - (1.0 - radius_progress).powi(3);

                    // Convert Pixels to f32 to perform standard floating-point operations
                    let w = f32::from(bounds.size.width);
                    let h = f32::from(bounds.size.height);

                    // Starting radius is 15% of the largest dimension
                    let start_radius = w.max(h) * 0.15;
                    // Bounded ending radius expands to cover the entire diagonal plus 10px extra
                    let diagonal = (w * w + h * h).sqrt();
                    let end_radius = diagonal * 0.5 + 10.0;

                    let current_radius_f32 =
                        start_radius + (end_radius - start_radius) * eased_radius_progress;
                    let current_radius = px(current_radius_f32);

                    // Center shifts towards the exact center of the bounding box
                    let center_progress = radius_progress; // linear
                    let target_center = Point {
                        x: bounds.origin.x + bounds.size.width * 0.5,
                        y: bounds.origin.y + bounds.size.height * 0.5,
                    };
                    let current_center = Point {
                        x: ripple.press_position.x
                            + (target_center.x - ripple.press_position.x) * center_progress,
                        y: ripple.press_position.y
                            + (target_center.y - ripple.press_position.y) * center_progress,
                    };

                    let unclipped_ripple_bounds = Bounds {
                        origin: Point {
                            x: current_center.x - current_radius,
                            y: current_center.y - current_radius,
                        },
                        size: gpui::Size {
                            width: current_radius * 2.0,
                            height: current_radius * 2.0,
                        },
                    };

                    let clipped_ripple_bounds = bounds.intersect(&unclipped_ripple_bounds);
                    let ripple_color = base_color.opacity(alpha);

                    window.paint_quad(PaintQuad {
                        bounds: clipped_ripple_bounds,
                        border_widths: gpui::Edges::all(px(0.0)),
                        border_color: gpui::transparent_black(),
                        background: ripple_color.into(),
                        corner_radii: self.corner_radii,
                        border_style: gpui::BorderStyle::default(),
                    });
                }
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use gpui::{AppContext, TestAppContext, point};

    use super::*;

    #[test]
    fn test_is_point_in_rounded_bounds() {
        let bounds = Bounds {
            origin: point(px(100.0), px(100.0)),
            size: gpui::Size {
                width: px(100.0),
                height: px(40.0),
            },
        };
        let corners = Corners::all(px(20.0));

        // Center point -> inside
        assert!(is_point_in_rounded_bounds(
            point(px(150.0), px(120.0)),
            bounds,
            corners
        ));

        // Top-right corner outside curve -> outside
        assert!(!is_point_in_rounded_bounds(
            point(px(198.0), px(102.0)),
            bounds,
            corners
        ));

        // Top-right corner inside curve -> inside
        assert!(is_point_in_rounded_bounds(
            point(px(185.0), px(115.0)),
            bounds,
            corners
        ));
    }

    #[gpui::test]
    fn test_ripple_state_start_ripple(cx: &mut TestAppContext) {
        let state = cx.new(|_| RippleState::new());
        assert!(cx.read(|cx| state.read(cx).ripples.is_empty()));

        let pos = point(px(10.0), px(20.0));
        cx.update(|cx| {
            RippleState::start_ripple(state.clone(), pos, cx);
        });

        cx.read(|cx| {
            let ripples = &state.read(cx).ripples;
            assert_eq!(ripples.len(), 1);
            assert_eq!(ripples[0].press_position, pos);
            assert!(state.read(cx).is_pressed);
        });
    }
}
