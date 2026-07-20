use std::time::{Duration, Instant};

use gpui::{
    App, Bounds, Corners, Element, ElementId, Entity, Hsla, IntoElement,
    LayoutId, PaintQuad, Pixels, Point, Window, px,
};

#[derive(Clone, Copy)]
pub struct ActiveRipple {
    pub start_time: Instant,
    pub press_position: Point<Pixels>,
}

pub struct RippleState {
    pub ripples: Vec<ActiveRipple>,
}

impl Default for RippleState {
    fn default() -> Self {
        Self::new()
    }
}

impl RippleState {
    pub fn new() -> Self {
        Self { ripples: Vec::new() }
    }

    pub fn start_ripple(
        state: Entity<Self>,
        press_position: Point<Pixels>,
        cx: &mut App,
    ) {
        _ = state.update(cx, |this, _| {
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
                    cx.background_executor().timer(Duration::from_millis(16)).await;
                    let finished = cx.update(|cx| {
                        state.update(cx, |this, cx| {
                            // Retain ripples that are still within their 375ms animation lifetime
                            this.ripples.retain(|r| r.start_time.elapsed().as_secs_f32() < 0.375);
                            cx.notify();
                            this.ripples.is_empty()
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
        self.child.request_layout(global_id, inspector_id, window, cx)
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
        self.child.prepaint(global_id, inspector_id, bounds, request_layout, window, cx)
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
        // Paint the child first, so the ripple is overlaid on top of it.
        self.child.paint(global_id, inspector_id, bounds, request_layout, prepaint, window, cx);

        // Retrieve active ripples and paint them
        let ripples = {
            let state = self.state.read(cx);
            state.ripples.clone()
        };

        if !ripples.is_empty() {
            use crate::theme::ActiveTheme;
            let base_color = self.color.unwrap_or_else(|| cx.theme().on_surface);

            window.with_content_mask(Some(gpui::ContentMask { bounds }), |window| {
                for ripple in ripples {
                    let elapsed = ripple.start_time.elapsed().as_secs_f32();
                    if elapsed >= 0.375 {
                        continue;
                    }

                    // Material 3 specs:
                    // - FadeInDuration = 75ms (linear)
                    // - RadiusDuration = 225ms (ease-out cubic / FastOutSlowIn)
                    // - FadeOutDuration = 150ms (linear, starting after 225ms)
                    let radius_progress = (elapsed / 0.225).clamp(0.0, 1.0);
                    let fade_in_progress = (elapsed / 0.075).clamp(0.0, 1.0);
                    let fade_out_progress = if elapsed > 0.225 {
                        ((elapsed - 0.225) / 0.150).clamp(0.0, 1.0)
                    } else {
                        0.0
                    };

                    let alpha = fade_in_progress * (1.0 - fade_out_progress) * 0.16;

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

                    let current_radius_f32 = start_radius + (end_radius - start_radius) * eased_radius_progress;
                    let current_radius = px(current_radius_f32);

                    // Center shifts towards the exact center of the bounding box
                    let center_progress = radius_progress; // linear
                    let target_center = Point {
                        x: bounds.origin.x + bounds.size.width * 0.5,
                        y: bounds.origin.y + bounds.size.height * 0.5,
                    };
                    let current_center = Point {
                        x: ripple.press_position.x + (target_center.x - ripple.press_position.x) * center_progress,
                        y: ripple.press_position.y + (target_center.y - ripple.press_position.y) * center_progress,
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
    use super::*;
    use gpui::{point, AppContext, TestAppContext};

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
        });
    }
}
