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
    Pixels, Point, Render, SharedString, StatefulInteractiveElement, Styled, Subscription, Task,
    Window, div, prelude::FluentBuilder as _, px,
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

pub(super) const ANIM_DURATION: std::time::Duration = std::time::Duration::from_millis(250);

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
    animation_task: Option<Task<()>>,
    focus_loss_task: Option<Task<()>>,
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
            animation_task: None,
            focus_loss_task: None,
        }
    }

    fn schedule_focus_loss(&mut self, cx: &mut Context<Self>) {
        // Wayland deactivates the card layer before the bar receives the click
        // that toggles its source. Give that click a chance to become the
        // authoritative dismissal instead of racing it with FocusLost.
        self.focus_loss_task = Some(cx.spawn(async move |_, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(220))
                .await;
            cx.update(|cx| {
                super::adapter::CardCoordinator::dispatch(
                    cx,
                    CardRequest::Dismiss {
                        channel: CardChannel::Persistent,
                        reason: CardDismissReason::FocusLost,
                    },
                );
            });
        }));
    }

    pub fn install_focus_loss_listener(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(handle) = self.focus_handle.clone() else {
            return;
        };
        let tracked = handle.clone();
        self._subscriptions
            .push(cx.on_blur(&handle, window, move |this, window, cx| {
                if !tracked.contains_focused(window, cx) {
                    this.schedule_focus_loss(cx);
                }
            }));
        self._subscriptions
            .push(cx.observe_window_activation(window, |this, window, cx| {
                if !window.is_window_active() {
                    this.schedule_focus_loss(cx);
                } else {
                    this.focus_loss_task = None;
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
        self.start_animation_task(cx);
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
            self.start_animation_task(cx);
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

    fn start_animation_task(&mut self, cx: &mut Context<Self>) {
        if self.reduced_motion {
            self.animation_task = None;
            return;
        }
        self.animation_task = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(16))
                    .await;
                let keep_running = this
                    .update(cx, |this, cx| {
                        this.tick_animation(16.0, cx);
                        matches!(
                            this.anim,
                            AnimPhase::Entering { .. } | AnimPhase::Exiting { .. }
                        )
                    })
                    .unwrap_or(false);
                if !keep_running {
                    break;
                }
            }
        }));
    }

    fn tick_animation(&mut self, elapsed_ms: f32, cx: &mut Context<Self>) {
        match self.anim {
            AnimPhase::Entering { progress } => {
                let next = (progress + elapsed_ms / ANIM_DURATION.as_millis() as f32).min(1.0);
                if next >= 1.0 {
                    self.anim = AnimPhase::Visible;
                } else {
                    self.anim = AnimPhase::Entering { progress: next };
                }
                cx.notify();
            }
            AnimPhase::Exiting { progress } => {
                let next = (progress - elapsed_ms / ANIM_DURATION.as_millis() as f32).max(0.0);
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
        let distance = match self.bar_edge {
            BarPosition::Top | BarPosition::Bottom => self
                .card_local_bounds
                .map_or(px(0.0), |bounds| bounds.size.height),
            BarPosition::Left | BarPosition::Right => self
                .card_local_bounds
                .map_or(px(0.0), |bounds| bounds.size.width),
        } * (1.0 - progress);
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
        let visual_bounds = Bounds {
            origin: Point {
                x: bounds.origin.x + translation.x,
                y: bounds.origin.y + translation.y,
            },
            size: bounds.size,
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
                this.on_key_down(move |event, _, cx| {
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

        // Everything outside the card remains true Wayland click-through. For
        // persistent cards the underlying app receives the original click and
        // its focus change dismisses this card through the blur listener.
        window.set_input_region(Some(&[visual_bounds]));

        root.child(card_shell)
    }
}
