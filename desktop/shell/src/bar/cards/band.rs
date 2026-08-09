//! `CardBandView` — the GPUI `Render` entity that drives one edge-band
//! layer-shell surface.
//!
//! Each band is a transparent overlay surface spanning the full monitor width
//! (horizontal bars) or full monitor height (vertical bars).  The band renders
//! either nothing (fully transparent + empty input region) or a single M3 card
//! shell at the placement-engine-supplied position.
//!
//! Animation: 200–250 ms emphasized ease `cubic_bezier(0.2, 0.0, 0.0, 1.0)`,
//! combined fade + scale + inward translation.  When `reduced_motion` is true
//! content appears / disappears immediately.

use gpui::{
    App, Bounds, Context, FocusHandle, Focusable, InteractiveElement, IntoElement, ParentElement,
    Pixels, Point, Render, SharedString, StatefulInteractiveElement, Styled, Subscription, Window,
    div, prelude::FluentBuilder as _, px,
};
use shilpo_config::BarPosition;
use shilpo_ui::ActiveTheme;

use super::{
    model::{CardChannel, CardDismissReason, CardOwnerId, CardRequest},
    provider::CardContentRenderFn,
};

// ────────────────────────────────────────────────────────────────
// CardBandView
// ────────────────────────────────────────────────────────────────

/// Animation phase for card enter / exit.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
enum AnimPhase {
    #[default]
    Hidden,
    /// Entering: `t` runs 0.0 → 1.0 over `ANIM_DURATION_MS`.
    Entering {
        progress: f32,
    },
    Visible,
    /// Exiting: `t` runs 1.0 → 0.0.
    Exiting {
        progress: f32,
    },
}

const ANIM_DURATION_MS: f32 = 220.0;

/// Owns one edge-band layer-shell surface rendering context.
///
/// The coordinator creates one `CardBandView` entity per monitor per channel
/// and updates its `content` field when cards open or close.
pub(crate) struct CardBandView {
    /// Which bar edge this band is attached to (determines animation direction).
    pub bar_edge: BarPosition,
    /// Which channel this band serves.
    pub channel: CardChannel,
    /// Card bounds in band-local (surface-local) coordinates.
    pub card_local_bounds: Option<Bounds<Pixels>>,
    /// Lazy content factory.  `None` means the band is empty (transparent).
    pub content: Option<CardContentRenderFn>,
    /// Focus handle — only set for `Persistent` channel surfaces.
    pub focus_handle: Option<FocusHandle>,
    /// Whether to skip animation and show / hide immediately.
    pub reduced_motion: bool,
    /// Animation state.
    anim: AnimPhase,
    /// Owner id for diagnostics in key handlers.
    pub owner_id: Option<CardOwnerId>,
    /// Which display (for diagnostics).
    pub _surface_id: SharedString,
    _subscriptions: Vec<Subscription>,
}

impl CardBandView {
    pub fn new(
        bar_edge: BarPosition,
        channel: CardChannel,
        reduced_motion: bool,
        surface_id: impl Into<SharedString>,
        focus_handle: Option<FocusHandle>,
    ) -> Self {
        Self {
            bar_edge,
            channel,
            card_local_bounds: None,
            content: None,
            focus_handle,
            reduced_motion,
            anim: AnimPhase::Hidden,
            owner_id: None,
            _surface_id: surface_id.into(),
            _subscriptions: Vec::new(),
        }
    }

    pub fn install_focus_loss_listener(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(handle) = self.focus_handle.clone() else {
            return;
        };
        let tracked = handle.clone();
        self._subscriptions
            .push(cx.on_blur(&handle, window, move |_, window, cx| {
                if !tracked.contains_focused(window, cx) {
                    super::adapter::CardCoordinator::dispatch(
                        cx,
                        CardRequest::Dismiss {
                            channel: CardChannel::Persistent,
                            reason: CardDismissReason::FocusLost,
                        },
                    );
                }
            }));
    }

    /// Show card content at the given band-local bounds.
    pub fn show(
        &mut self,
        card_local_bounds: Bounds<Pixels>,
        owner_id: CardOwnerId,
        content: CardContentRenderFn,
        cx: &mut Context<Self>,
    ) {
        self.card_local_bounds = Some(card_local_bounds);
        self.owner_id = Some(owner_id);
        self.content = Some(content);
        self.anim = if self.reduced_motion {
            AnimPhase::Visible
        } else {
            AnimPhase::Entering { progress: 0.0 }
        };
        cx.notify();
    }

    /// Hide card content (start exit animation or clear immediately).
    pub fn hide(&mut self, cx: &mut Context<Self>) {
        if self.content.is_none() {
            return;
        }
        if self.reduced_motion {
            self.clear(cx);
        } else {
            self.anim = AnimPhase::Exiting { progress: 1.0 };
            cx.notify();
        }
    }

    /// Move an already-visible card without restarting its entrance animation.
    pub fn reposition(&mut self, card_local_bounds: Bounds<Pixels>, cx: &mut Context<Self>) {
        self.card_local_bounds = Some(card_local_bounds);
        cx.notify();
    }

    /// Immediately clear all content (used after exit animation completes or on shutdown).
    pub fn clear(&mut self, cx: &mut Context<Self>) {
        self.content = None;
        self.card_local_bounds = None;
        self.owner_id = None;
        self.anim = AnimPhase::Hidden;
        cx.notify();
    }

    /// Whether the band currently has content (or is animating out).
    #[allow(dead_code)]
    pub fn is_visible(&self) -> bool {
        self.content.is_some()
    }

    // ── Animation tick ────────────────────────────────────────────

    fn tick_animation(&mut self, elapsed_ms: f32, cx: &mut Context<Self>) {
        match self.anim {
            AnimPhase::Entering { progress } => {
                let next = (progress + elapsed_ms / ANIM_DURATION_MS).min(1.0);
                if next >= 1.0 {
                    self.anim = AnimPhase::Visible;
                } else {
                    self.anim = AnimPhase::Entering { progress: next };
                }
                cx.notify();
            }
            AnimPhase::Exiting { progress } => {
                let next = (progress - elapsed_ms / ANIM_DURATION_MS).max(0.0);
                if next <= 0.0 {
                    self.clear(cx);
                } else {
                    self.anim = AnimPhase::Exiting { progress: next };
                    cx.notify();
                }
            }
            _ => {}
        }
    }

    fn anim_progress(&self) -> f32 {
        match self.anim {
            AnimPhase::Hidden => 0.0,
            AnimPhase::Entering { progress } => progress,
            AnimPhase::Visible => 1.0,
            AnimPhase::Exiting { progress } => progress,
        }
    }

    /// Compute the inward translation offset based on bar edge and progress.
    fn translation_offset(&self, progress: f32) -> Point<Pixels> {
        let distance = px(12.0) * (1.0 - progress);
        match self.bar_edge {
            BarPosition::Top => Point {
                x: px(0.0),
                y: -distance,
            },
            BarPosition::Bottom => Point {
                x: px(0.0),
                y: distance,
            },
            BarPosition::Left => Point {
                x: -distance,
                y: px(0.0),
            },
            BarPosition::Right => Point {
                x: distance,
                y: px(0.0),
            },
        }
    }
}

impl Focusable for CardBandView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        // This should only be called for persistent bands which always have a handle.
        self.focus_handle
            .clone()
            .expect("CardBandView: focus handle requested but not set")
    }
}

impl Render for CardBandView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Tick animation via GPUI's request_animation_frame mechanism.
        if matches!(
            self.anim,
            AnimPhase::Entering { .. } | AnimPhase::Exiting { .. }
        ) {
            let frame_time_ms = 16.0_f32; // approximate; GPUI drives at display refresh
            self.tick_animation(frame_time_ms, cx);
        }

        let raw_progress = self.anim_progress();
        let progress = shilpo_ui::animation::cubic_bezier(0.2, 0.0, 0.0, 1.0)(raw_progress);
        let has_content = self.content.is_some();
        let card_bounds = self.card_local_bounds;

        // Build the band root: full-surface transparent container.
        let root = div()
            .id("card-band-root")
            .w_full()
            .h_full()
            .relative()
            .bg(gpui::transparent_black());

        if !has_content || card_bounds.is_none() {
            // Set empty input region — click-through.
            window.set_input_region(Some(&[]));
            return root;
        }

        let bounds = card_bounds.unwrap();
        let translation = self.translation_offset(progress);
        let scale = 0.96 + 0.04 * progress;
        let scaled_size = gpui::size(bounds.size.width * scale, bounds.size.height * scale);
        let visual_bounds = Bounds {
            origin: Point {
                x: bounds.origin.x + (bounds.size.width - scaled_size.width) / 2.0 + translation.x,
                y: bounds.origin.y
                    + (bounds.size.height - scaled_size.height) / 2.0
                    + translation.y,
            },
            size: scaled_size,
        };

        // Build M3 card shell.
        let card_content = self
            .content
            .as_ref()
            .map(|content_fn| content_fn(window, cx));

        let theme = cx.theme().clone();

        // Escape key handler for persistent channel.
        let channel = self.channel;
        let owner_for_escape = self.owner_id.clone();
        let owner_for_hover = self.owner_id.clone();

        let card_shell = div()
            .id("card-shell")
            .absolute()
            .left(visual_bounds.origin.x)
            .top(visual_bounds.origin.y)
            .w(visual_bounds.size.width)
            .h(visual_bounds.size.height)
            .rounded_3xl()
            .bg(theme.surface_container_high)
            .shadow_lg()
            .overflow_hidden()
            .opacity(progress)
            .when_some(self.focus_handle.as_ref(), |this, handle| {
                this.track_focus(handle)
            })
            .when_some(card_content, |this, content| this.child(content))
            .when(channel == CardChannel::Preview, |this| {
                this.on_hover(move |hovered, _, cx| {
                    if let Some(owner) = owner_for_hover.clone() {
                        let request = if *hovered {
                            CardRequest::PreviewEnter { owner }
                        } else {
                            CardRequest::PreviewLeave { owner }
                        };
                        super::adapter::CardCoordinator::dispatch(cx, request);
                    }
                })
            })
            .when(channel == CardChannel::Persistent, |this| {
                this.on_mouse_down_out(|_, _, cx| {
                    super::adapter::CardCoordinator::dispatch(
                        cx,
                        CardRequest::Dismiss {
                            channel: CardChannel::Persistent,
                            reason: CardDismissReason::OutsideClick,
                        },
                    );
                })
                .on_key_down(move |event, _, cx| {
                    if event.keystroke.key == "escape"
                        && let Some(ref owner) = owner_for_escape
                    {
                        super::adapter::CardCoordinator::dispatch(
                            cx,
                            CardRequest::Dismiss {
                                channel: CardChannel::Persistent,
                                reason: CardDismissReason::Escape,
                            },
                        );
                        tracing::debug!(
                            owner = %owner,
                            "card dismissed via Escape key"
                        );
                    }
                })
            });

        // Update input region to match card bounds.
        let card_region = visual_bounds;
        window.set_input_region(Some(&[card_region]));

        root.child(card_shell)
    }
}
