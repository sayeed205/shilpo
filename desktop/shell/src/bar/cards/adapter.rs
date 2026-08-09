//! `CardCoordinator` — GPUI adapter that interprets `CardEffect`s from the
//! pure state machine into real shell operations.
//!
//! Responsibilities:
//! - Owns the `CardState` instance.
//! - Schedules and cancels preview-open / preview-close timers via `cx.spawn`.
//! - Creates, reuses, and destroys edge-band layer-shell surfaces (one per
//!   monitor per channel, lazily).
//! - Manages focus capture and restoration via the compositor snapshot.
//! - Updates input regions on band surfaces.
//! - Emits structured `tracing::debug!` diagnostics.

use std::{collections::HashMap, sync::Arc};

use gpui::layer_shell::{Anchor, KeyboardInteractivity, Layer, LayerShellOptions};
use gpui::{
    App, AppContext, Bounds, DisplayId, IntoElement, Pixels, Task, WindowBackgroundAppearance,
    WindowBounds, WindowHandle, WindowKind, WindowOptions, px,
};
use shilpo_config::BarPosition;

use crate::runtime::ShellRuntime;

use super::{
    band::CardBandView,
    model::{
        CardChannel, CardDiagnostic, CardDismissReason, CardEffect, CardOwnerId, CardRequest,
        CardSourceState, CardState, TimerKind,
    },
    placement::{
        BandGeometry, PlacementInput, PlacementResult, compute_persistent_band_geometry,
        compute_placement,
    },
    provider::CardProvider,
};

// ────────────────────────────────────────────────────────────────
// CardCoordinator
// ────────────────────────────────────────────────────────────────

/// Central coordinator owned as a field of `ShellSurfaces`.
#[derive(Default)]
pub struct CardCoordinator {
    state: CardState,
    providers: HashMap<CardOwnerId, Arc<dyn CardProvider>>,

    // ── Timer tasks (dropping the Task cancels it) ────────────────
    preview_open_task: Option<Task<()>>,
    preview_close_task: Option<Task<()>>,
    persistent_exit_task: Option<Task<()>>,
    preview_exit_task: Option<Task<()>>,

    // ── Edge-band surfaces (lazy per-monitor per-channel) ─────────
    persistent_bands: HashMap<DisplayId, WindowHandle<CardBandView>>,
    preview_bands: HashMap<DisplayId, WindowHandle<CardBandView>>,

    // ── Focus restoration ─────────────────────────────────────────
    /// Compositor focused-window ID captured before the first persistent open.
    prior_focused_window: Option<u64>,
}

impl CardCoordinator {
    // ── Public API ────────────────────────────────────────────────

    /// Register a built-in card provider.  Replaces any previously registered
    /// provider with the same `owner_id`.
    #[allow(dead_code)]
    pub(crate) fn register_provider(cx: &mut App, provider: Arc<dyn CardProvider>) {
        let owner_id = provider.owner_id();
        tracing::debug!(owner = %owner_id, "card provider registered");
        cx.global_mut::<ShellRuntime>()
            .shell_surfaces_mut()
            .card_coordinator
            .providers
            .insert(owner_id, provider);
    }

    pub(crate) fn register_provider_direct(&mut self, provider: Arc<dyn CardProvider>) {
        let owner_id = provider.owner_id();
        tracing::debug!(owner = %owner_id, "card provider registered directly");
        self.providers.insert(owner_id, provider);
    }

    /// Remove a provider.  Dispatches `OwnerRemoved` dismiss into the state
    /// machine to cleanly close any open card for that owner.
    #[allow(dead_code)]
    pub(crate) fn remove_provider(cx: &mut App, owner_id: &CardOwnerId) {
        tracing::debug!(owner = %owner_id, "card provider removed");
        Self::dispatch(
            cx,
            CardRequest::OwnerRemoved {
                owner: owner_id.clone(),
            },
        );
        cx.global_mut::<ShellRuntime>()
            .shell_surfaces_mut()
            .card_coordinator
            .providers
            .remove(owner_id);
    }

    /// Dispatch a semantic `CardRequest` through the state machine and
    /// interpret the resulting `CardEffect`s.
    pub(crate) fn dispatch(cx: &mut App, request: CardRequest) {
        tracing::debug!(request = ?request, "card request dispatched");

        if !Self::request_is_supported(cx, &request) {
            tracing::debug!(request = ?request, "card request ignored: unsupported capability");
            return;
        }

        // Run the pure reducer.
        let effects = {
            let coordinator = &mut cx
                .global_mut::<ShellRuntime>()
                .shell_surfaces_mut()
                .card_coordinator;
            coordinator.state.reduce(request)
        };

        // Interpret effects one by one.
        Self::apply_effects(cx, effects);
    }

    fn request_is_supported(cx: &App, request: &CardRequest) -> bool {
        let (owner, hover, click) = match request {
            CardRequest::SourceEnter { owner }
            | CardRequest::SourceLeave { owner }
            | CardRequest::PreviewEnter { owner }
            | CardRequest::PreviewLeave { owner } => (owner, true, false),
            CardRequest::PersistentToggle { owner }
            | CardRequest::PersistentToggleAt { owner, .. } => (owner, false, true),
            _ => return true,
        };
        let Some(provider) = cx
            .global::<ShellRuntime>()
            .shell_surfaces()
            .card_coordinator
            .providers
            .get(owner)
        else {
            return false;
        };
        let capabilities = provider.capabilities();
        (!hover || capabilities.hover) && (!click || capabilities.click)
    }

    /// Query the source state for a given owner.
    #[allow(dead_code)]
    pub(crate) fn source_state(cx: &App, owner: &CardOwnerId) -> CardSourceState {
        if !cx.has_global::<ShellRuntime>() {
            return CardSourceState::Idle;
        }
        cx.global::<ShellRuntime>()
            .shell_surfaces()
            .card_coordinator
            .state
            .source_state(owner)
    }

    /// Whether the card system currently holds bar visibility.
    pub(crate) fn holds_bar_visibility(cx: &App) -> bool {
        if !cx.has_global::<ShellRuntime>() {
            return false;
        }
        cx.global::<ShellRuntime>()
            .shell_surfaces()
            .card_coordinator
            .state
            .holds_bar_visibility()
    }

    pub(crate) fn persistent_band_displays(&self) -> impl Iterator<Item = DisplayId> + '_ {
        self.persistent_bands.keys().copied()
    }

    pub(crate) fn preview_band_displays(&self) -> impl Iterator<Item = DisplayId> + '_ {
        self.preview_bands.keys().copied()
    }

    /// Re-render any open card owned by `owner` after its authoritative data changes.
    pub(crate) fn refresh_owner(cx: &mut App, owner: &CardOwnerId) {
        let handles = {
            let coordinator = &cx
                .global::<ShellRuntime>()
                .shell_surfaces()
                .card_coordinator;
            [CardChannel::Persistent, CardChannel::Preview]
                .into_iter()
                .filter_map(|channel| {
                    let slot = match channel {
                        CardChannel::Persistent => &coordinator.state.persistent,
                        CardChannel::Preview => &coordinator.state.preview,
                    };
                    if slot.owner.as_ref() != Some(owner) || !slot.is_open() {
                        return None;
                    }
                    let display = slot.display_id?;
                    match channel {
                        CardChannel::Persistent => coordinator.persistent_bands.get(&display),
                        CardChannel::Preview => coordinator.preview_bands.get(&display),
                    }
                    .copied()
                })
                .collect::<Vec<_>>()
        };
        for handle in handles {
            let _ = handle.update(cx, |_, _, cx| cx.notify());
        }
    }

    fn project_lifecycle(
        slot: &super::model::ChannelSlot,
    ) -> crate::runtime::shell_surfaces::SurfaceLifecycle {
        use crate::runtime::shell_surfaces::SurfaceLifecycle;
        match slot.lifecycle {
            super::model::ChannelLifecycle::Closed => SurfaceLifecycle::Closed,
            super::model::ChannelLifecycle::Open => SurfaceLifecycle::Open {
                generation: slot.generation,
            },
            super::model::ChannelLifecycle::Closing => SurfaceLifecycle::Closing {
                generation: slot.generation,
            },
        }
    }

    /// Lifecycle projection for `SurfaceSnapshot`.
    pub(crate) fn persistent_lifecycle(
        cx: &App,
    ) -> crate::runtime::shell_surfaces::SurfaceLifecycle {
        if !cx.has_global::<ShellRuntime>() {
            return crate::runtime::shell_surfaces::SurfaceLifecycle::Closed;
        }
        Self::project_lifecycle(
            &cx.global::<ShellRuntime>()
                .shell_surfaces()
                .card_coordinator
                .state
                .persistent,
        )
    }

    /// Lifecycle projection for `SurfaceSnapshot`.
    pub(crate) fn preview_lifecycle(cx: &App) -> crate::runtime::shell_surfaces::SurfaceLifecycle {
        if !cx.has_global::<ShellRuntime>() {
            return crate::runtime::shell_surfaces::SurfaceLifecycle::Closed;
        }
        Self::project_lifecycle(
            &cx.global::<ShellRuntime>()
                .shell_surfaces()
                .card_coordinator
                .state
                .preview,
        )
    }

    // ── Effect interpreter ────────────────────────────────────────

    fn apply_effects(cx: &mut App, effects: Vec<CardEffect>) {
        for effect in effects {
            Self::apply_effect(cx, effect);
        }
    }

    fn apply_effect(cx: &mut App, effect: CardEffect) {
        match effect {
            CardEffect::OpenChannel {
                channel,
                owner,
                generation,
            } => {
                tracing::debug!(
                    owner = %owner,
                    channel = ?channel,
                    generation,
                    "card channel opening"
                );
                Self::open_channel(cx, channel, owner, generation);
            }

            CardEffect::CloseChannel {
                channel,
                reason,
                display_id,
                generation,
            } => {
                tracing::debug!(
                    channel = ?channel,
                    reason = ?reason,
                    "card channel closing"
                );
                Self::close_channel(cx, channel, display_id, reason);
                Self::schedule_close_completion(cx, channel, display_id, generation);
            }

            CardEffect::RepositionChannel { channel, owner } => {
                Self::position_channel(cx, channel, owner, false);
            }

            CardEffect::StartTimer { kind, generation } => {
                let delay_ms = match kind {
                    TimerKind::PreviewOpen => 350,
                    TimerKind::PreviewClose => 200,
                };
                let delay = std::time::Duration::from_millis(delay_ms);
                tracing::debug!(
                    kind = ?kind,
                    generation,
                    delay_ms,
                    "card timer started"
                );

                let task = cx.spawn(async move |cx| {
                    cx.background_executor().timer(delay).await;
                    cx.update(|cx| match kind {
                        TimerKind::PreviewOpen => {
                            Self::dispatch(cx, CardRequest::PreviewOpenTimer { generation })
                        }
                        TimerKind::PreviewClose => {
                            Self::dispatch(cx, CardRequest::PreviewCloseTimer { generation })
                        }
                    });
                });

                let coordinator = &mut cx
                    .global_mut::<ShellRuntime>()
                    .shell_surfaces_mut()
                    .card_coordinator;
                match kind {
                    TimerKind::PreviewOpen => coordinator.preview_open_task = Some(task),
                    TimerKind::PreviewClose => coordinator.preview_close_task = Some(task),
                }
            }

            CardEffect::CancelTimers { kind } => {
                tracing::debug!(kind = ?kind, "card timer cancelled");
                let coordinator = &mut cx
                    .global_mut::<ShellRuntime>()
                    .shell_surfaces_mut()
                    .card_coordinator;
                match kind {
                    TimerKind::PreviewOpen => coordinator.preview_open_task = None,
                    TimerKind::PreviewClose => coordinator.preview_close_task = None,
                }
            }

            CardEffect::CaptureFocus => {
                let focused =
                    crate::runtime::shell_surfaces::ShellSurfaces::compositor_snapshot(cx)
                        .focused_window_id;
                cx.global_mut::<ShellRuntime>()
                    .shell_surfaces_mut()
                    .card_coordinator
                    .prior_focused_window = focused;
                tracing::debug!(
                    focused_window = ?focused,
                    "card coordinator captured prior focus"
                );
            }

            CardEffect::RestoreFocus => {
                let prior = cx
                    .global_mut::<ShellRuntime>()
                    .shell_surfaces_mut()
                    .card_coordinator
                    .prior_focused_window
                    .take();
                if let Some(window_id) = prior {
                    tracing::debug!(window_id, "card coordinator restoring prior focus");
                    let _ = crate::runtime::shell_surfaces::ShellSurfaces::overview_focus_window(
                        cx, window_id,
                    );
                }
            }

            CardEffect::UpdateBarHold { hold } => {
                // Infrastructure state — a future auto-hide controller will consume this.
                tracing::debug!(hold, "card coordinator bar visibility hold updated");
                // The hold value is already reflected in `CardState::bar_visibility_hold`.
                // The coordinator exposes it via `holds_bar_visibility()`.
            }

            CardEffect::Diagnostic(diag) => {
                Self::emit_diagnostic(diag);
            }
        }
    }

    fn schedule_close_completion(
        cx: &mut App,
        channel: CardChannel,
        display_id: Option<DisplayId>,
        generation: u64,
    ) {
        let reduced_motion = ShellRuntime::active_config(cx).theme.reduced_motion;
        if reduced_motion {
            Self::dispatch(
                cx,
                CardRequest::CloseAnimationFinished {
                    channel,
                    generation,
                },
            );
            return;
        }

        let task = cx.spawn(async move |cx| {
            cx.background_executor()
                .timer(super::band::ANIM_DURATION)
                .await;
            cx.update(|cx| {
                Self::clear_band_if_still_closing(cx, channel, display_id, generation);
                Self::dispatch(
                    cx,
                    CardRequest::CloseAnimationFinished {
                        channel,
                        generation,
                    },
                );
            });
        });
        let coordinator = &mut cx
            .global_mut::<ShellRuntime>()
            .shell_surfaces_mut()
            .card_coordinator;
        match channel {
            CardChannel::Persistent => coordinator.persistent_exit_task = Some(task),
            CardChannel::Preview => coordinator.preview_exit_task = Some(task),
        }
    }

    fn clear_band_if_still_closing(
        cx: &mut App,
        channel: CardChannel,
        display_id: Option<DisplayId>,
        generation: u64,
    ) {
        let Some(display_id) = display_id else {
            return;
        };
        let handle = {
            let coordinator = &cx
                .global::<ShellRuntime>()
                .shell_surfaces()
                .card_coordinator;
            let slot = match channel {
                CardChannel::Persistent => &coordinator.state.persistent,
                CardChannel::Preview => &coordinator.state.preview,
            };
            if slot.lifecycle != super::model::ChannelLifecycle::Closing
                || slot.generation != generation
            {
                return;
            }
            match channel {
                CardChannel::Persistent => coordinator.persistent_bands.get(&display_id),
                CardChannel::Preview => coordinator.preview_bands.get(&display_id),
            }
            .copied()
        };
        if let Some(handle) = handle {
            if channel == CardChannel::Persistent {
                cx.global_mut::<ShellRuntime>()
                    .shell_surfaces_mut()
                    .card_coordinator
                    .persistent_bands
                    .remove(&display_id);
                let _ = handle.update(cx, |_, window, _| window.remove_window());
            } else {
                let _ = handle.update(cx, |band, _, cx| band.clear(cx));
            }
        }
    }

    // ── Channel surface management ────────────────────────────────

    fn open_channel(cx: &mut App, channel: CardChannel, owner: CardOwnerId, _generation: u64) {
        Self::position_channel(cx, channel, owner, true);
    }

    fn position_channel(cx: &mut App, channel: CardChannel, owner: CardOwnerId, animate: bool) {
        // Find the provider.
        let provider = {
            let coordinator = &cx
                .global::<ShellRuntime>()
                .shell_surfaces()
                .card_coordinator;
            coordinator.providers.get(&owner).map(|_| ())
        };
        if provider.is_none() {
            tracing::warn!(owner = %owner, "card open requested but provider not found");
            Self::dismiss_failed_open(cx, channel);
            return;
        }

        // Get anchor bounds and placement info from the provider.
        let (provider_anchor, size_tier) = {
            let coordinator = &cx
                .global::<ShellRuntime>()
                .shell_surfaces()
                .card_coordinator;
            if let Some(p) = coordinator.providers.get(&owner) {
                let anchor = p.anchor_bounds(cx);
                let tier = p.size_tier();
                (anchor, tier)
            } else {
                return;
            }
        };

        let state_anchor = {
            let state = &cx
                .global::<ShellRuntime>()
                .shell_surfaces()
                .card_coordinator
                .state;
            let slot = match channel {
                CardChannel::Persistent => &state.persistent,
                CardChannel::Preview => &state.preview,
            };
            (slot.owner.as_ref() == Some(&owner))
                .then(|| slot.anchor_bounds.zip(slot.display_id))
                .flatten()
        };
        let Some((anchor_bounds, display_id)) = state_anchor.or(provider_anchor) else {
            tracing::warn!(owner = %owner, "card open: provider has no anchor bounds, skipping");
            Self::dismiss_failed_open(cx, channel);
            return;
        };

        // Get collision bounds (persistent card bounds when opening preview).
        let collision_bounds: Option<Bounds<Pixels>> = if channel == CardChannel::Preview {
            let persistent = &cx
                .global::<ShellRuntime>()
                .shell_surfaces()
                .card_coordinator
                .state
                .persistent;
            if persistent.is_open() && persistent.display_id == Some(display_id) {
                persistent.card_bounds
            } else {
                None
            }
        } else {
            None
        };

        // Determine bar edge from config.
        let bar_config = ShellRuntime::active_config(cx).bar;
        let bar_edge = bar_config.position;
        let floating_margin = if bar_config.style == shilpo_config::BarStyle::Float {
            match bar_edge {
                BarPosition::Top | BarPosition::Bottom => bar_config.margin.vertical as f32,
                BarPosition::Left | BarPosition::Right => bar_config.margin.horizontal as f32,
            }
        } else {
            0.0
        };
        let bar_thickness = px(bar_config.height as f32 + floating_margin);

        // Get monitor bounds for this display.
        let monitor_bounds = cx
            .displays()
            .into_iter()
            .find(|d| d.id() == display_id)
            .map(|d| d.bounds())
            .unwrap_or_else(|| {
                gpui::Bounds::new(
                    gpui::point(px(0.0), px(0.0)),
                    gpui::size(px(1920.0), px(1080.0)),
                )
            });

        let requested_size = gpui::size(px(size_tier.max_width()), px(size_tier.max_height()));

        let placement_input = PlacementInput {
            monitor_bounds,
            bar_edge,
            bar_thickness,
            source_bounds: anchor_bounds,
            requested_size,
            collision_bounds,
        };

        let placement = compute_placement(&placement_input);

        match placement {
            PlacementResult::Suppressed { reason } => {
                tracing::debug!(
                    owner = %owner,
                    channel = ?channel,
                    reason,
                    "card placement suppressed"
                );
                Self::dismiss_failed_open(cx, channel);
            }
            PlacementResult::Placed {
                card_bounds,
                band_geometry,
            } => {
                let band_geometry = if channel == CardChannel::Persistent {
                    compute_persistent_band_geometry(&placement_input)
                } else {
                    band_geometry
                };
                // Ensure band surface exists.
                let Some(band_handle) =
                    Self::ensure_band(cx, channel, display_id, &band_geometry, bar_edge)
                else {
                    Self::dismiss_failed_open(cx, channel);
                    return;
                };

                // Convert global card bounds to band-local (surface-local) coords.
                let band_local_bounds = gpui::Bounds {
                    origin: gpui::Point {
                        x: card_bounds.origin.x - band_geometry.bounds.origin.x,
                        y: card_bounds.origin.y - band_geometry.bounds.origin.y,
                    },
                    size: card_bounds.size,
                };

                tracing::debug!(
                    owner = %owner,
                    channel = ?channel,
                    card_x = card_bounds.origin.x.as_f32(),
                    card_y = card_bounds.origin.y.as_f32(),
                    card_w = card_bounds.size.width.as_f32(),
                    card_h = card_bounds.size.height.as_f32(),
                    "card placement computed"
                );

                // Render content into band.
                let owner_clone = owner.clone();
                let _ = band_handle.update(cx, move |band, window, cx| {
                    let provider = cx
                        .global::<ShellRuntime>()
                        .shell_surfaces()
                        .card_coordinator
                        .providers
                        .get(&owner_clone)
                        .map(|_| ()); // just check presence

                    if provider.is_some() {
                        let content_owner = owner_clone.clone();
                        let ch = channel;
                        let content_box: super::provider::CardContentRenderFn =
                            Box::new(move |window, cx| {
                                let provider = cx
                                    .global::<ShellRuntime>()
                                    .shell_surfaces()
                                    .card_coordinator
                                    .providers
                                    .get(&content_owner)
                                    .cloned();

                                if let Some(provider) = provider {
                                    provider.render_content(ch, window, cx)
                                } else {
                                    gpui::div().into_any_element()
                                }
                            });
                        if animate {
                            band.show(band_local_bounds, owner_clone, content_box, cx);
                        } else {
                            band.reposition(band_local_bounds, cx);
                        }

                        // Focus persistent bands.
                        if channel == CardChannel::Persistent
                            && let Some(ref handle) = band.focus_handle.clone()
                        {
                            window.activate_window();
                            handle.focus(window, cx);
                        }
                    }
                });

                Self::dispatch(
                    cx,
                    CardRequest::PlacementUpdated {
                        owner,
                        channel,
                        bounds: card_bounds,
                        display_id,
                    },
                );
            }
        }
    }

    fn dismiss_failed_open(cx: &mut App, channel: CardChannel) {
        Self::dispatch(
            cx,
            CardRequest::Dismiss {
                channel,
                reason: CardDismissReason::Explicit,
            },
        );
    }

    fn close_channel(
        cx: &mut App,
        channel: CardChannel,
        display_id: Option<DisplayId>,
        reason: CardDismissReason,
    ) {
        if let Some(display_id) = display_id {
            let handle = {
                let coordinator = &cx
                    .global::<ShellRuntime>()
                    .shell_surfaces()
                    .card_coordinator;
                match channel {
                    CardChannel::Persistent => {
                        coordinator.persistent_bands.get(&display_id).copied()
                    }
                    CardChannel::Preview => coordinator.preview_bands.get(&display_id).copied(),
                }
            };

            if let Some(band_handle) = handle {
                let _ = band_handle.update(cx, |band, window, cx| {
                    if channel == CardChannel::Persistent
                        && reason == CardDismissReason::SourceToggle
                    {
                        window.activate_window();
                    }
                    band.hide(cx);
                });
            }
        }
    }

    /// Ensure a band surface exists for the given display+channel, creating it
    /// lazily if needed.  Returns the `WindowHandle<CardBandView>`.
    fn ensure_band(
        cx: &mut App,
        channel: CardChannel,
        display_id: DisplayId,
        band_geometry: &BandGeometry,
        bar_edge: BarPosition,
    ) -> Option<WindowHandle<CardBandView>> {
        // Check if band already exists.
        {
            let coordinator = &cx
                .global::<ShellRuntime>()
                .shell_surfaces()
                .card_coordinator;
            let existing = match channel {
                CardChannel::Persistent => coordinator.persistent_bands.get(&display_id).copied(),
                CardChannel::Preview => coordinator.preview_bands.get(&display_id).copied(),
            };
            if let Some(handle) = existing {
                tracing::debug!(
                    channel = ?channel,
                    display = ?display_id,
                    "card band reused"
                );
                return Some(handle);
            }
        }

        // Create a new band surface.
        tracing::debug!(
            channel = ?channel,
            display = ?display_id,
            "card band created"
        );

        let keyboard_interactivity = match channel {
            CardChannel::Persistent => KeyboardInteractivity::OnDemand,
            CardChannel::Preview => KeyboardInteractivity::None,
        };

        let bg = &band_geometry.bounds;
        let surface_bounds = Bounds::new(gpui::point(px(0.0), px(0.0)), bg.size);
        let options = WindowOptions {
            titlebar: None,
            window_bounds: Some(WindowBounds::Windowed(surface_bounds)),
            display_id: Some(display_id),
            app_id: Some(format!(
                "shilpo-card-{}",
                match channel {
                    CardChannel::Persistent => "persistent",
                    CardChannel::Preview => "preview",
                }
            )),
            window_background: WindowBackgroundAppearance::Transparent,
            kind: WindowKind::LayerShell(LayerShellOptions {
                namespace: format!(
                    "card-{}",
                    match channel {
                        CardChannel::Persistent => "persistent",
                        CardChannel::Preview => "preview",
                    }
                ),
                layer: Layer::Overlay,
                // Anchor to bar edge — full span across the axis perpendicular to bar.
                anchor: Self::band_anchor(bar_edge),
                exclusive_edge: Some(match bar_edge {
                    BarPosition::Top => Anchor::TOP,
                    BarPosition::Bottom => Anchor::BOTTOM,
                    BarPosition::Left => Anchor::LEFT,
                    BarPosition::Right => Anchor::RIGHT,
                }),
                exclusive_zone: None,
                margin: Some(Self::band_margin(bar_edge, band_geometry.bar_thickness)),
                keyboard_interactivity,
            }),
            ..Default::default()
        };

        let reduced_motion = ShellRuntime::active_config(cx).theme.reduced_motion;

        let surface_id = format!("band-{:?}-{:?}", channel, display_id);

        let handle = match cx.open_window(options, move |window, cx| {
            let focus_handle = match channel {
                CardChannel::Persistent => Some(cx.focus_handle()),
                CardChannel::Preview => None,
            };
            let view = cx.new(|_| {
                CardBandView::new(bar_edge, channel, reduced_motion, surface_id, focus_handle)
            });
            if channel == CardChannel::Persistent {
                view.update(cx, |view, cx| {
                    view.install_focus_loss_listener(window, cx);
                });
            }
            view
        }) {
            Ok(handle) => handle,
            Err(error) => {
                tracing::warn!(
                    error = %error,
                    channel = ?channel,
                    display = ?display_id,
                    "failed to open card band surface"
                );
                return None;
            }
        };

        {
            let coordinator = &mut cx
                .global_mut::<ShellRuntime>()
                .shell_surfaces_mut()
                .card_coordinator;
            match channel {
                CardChannel::Persistent => {
                    coordinator.persistent_bands.insert(display_id, handle);
                }
                CardChannel::Preview => {
                    coordinator.preview_bands.insert(display_id, handle);
                }
            }
        }

        Some(handle)
    }

    /// Destroy all band surfaces for a given display (called on `DisplayRemoved`).
    #[allow(dead_code)]
    pub(crate) fn destroy_bands_for_display(cx: &mut App, display_id: DisplayId) {
        let (persistent, preview) = {
            let coordinator = &mut cx
                .global_mut::<ShellRuntime>()
                .shell_surfaces_mut()
                .card_coordinator;
            (
                coordinator.persistent_bands.remove(&display_id),
                coordinator.preview_bands.remove(&display_id),
            )
        };
        for handle in persistent.into_iter().chain(preview) {
            let _ = handle.update(cx, |_, window, _| window.remove_window());
            tracing::debug!(display = ?display_id, "card band destroyed");
        }
    }

    /// Destroy all band surfaces (called on shutdown).
    pub(crate) fn destroy_all_bands(cx: &mut App) {
        let (persistent, preview) = {
            let coordinator = &mut cx
                .global_mut::<ShellRuntime>()
                .shell_surfaces_mut()
                .card_coordinator;
            (
                std::mem::take(&mut coordinator.persistent_bands),
                std::mem::take(&mut coordinator.preview_bands),
            )
        };
        for (display_id, handle) in persistent.into_iter().chain(preview) {
            let _ = handle.update(cx, |_, window, _| window.remove_window());
            tracing::debug!(display = ?display_id, "card band destroyed on shutdown");
        }
    }

    pub(crate) fn forget_window(cx: &mut App, window_id: gpui::WindowId) {
        let coordinator = &mut cx
            .global_mut::<ShellRuntime>()
            .shell_surfaces_mut()
            .card_coordinator;
        coordinator
            .persistent_bands
            .retain(|_, handle| handle.window_id() != window_id);
        coordinator
            .preview_bands
            .retain(|_, handle| handle.window_id() != window_id);
    }

    // ── Layer-shell anchor helpers ────────────────────────────────

    fn band_anchor(bar_edge: BarPosition) -> gpui::layer_shell::Anchor {
        use gpui::layer_shell::Anchor;
        match bar_edge {
            BarPosition::Top => Anchor::TOP | Anchor::LEFT | Anchor::RIGHT,
            BarPosition::Bottom => Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT,
            BarPosition::Left => Anchor::LEFT | Anchor::TOP | Anchor::BOTTOM,
            BarPosition::Right => Anchor::RIGHT | Anchor::TOP | Anchor::BOTTOM,
        }
    }

    fn band_margin(
        _bar_edge: BarPosition,
        _bar_thickness: gpui::Pixels,
    ) -> (gpui::Pixels, gpui::Pixels, gpui::Pixels, gpui::Pixels) {
        // Placement and window bounds already include the bar thickness and
        // source gap. Adding it again here doubles the bar-to-card offset.
        (px(0.0), px(0.0), px(0.0), px(0.0))
    }

    // ── Diagnostics ───────────────────────────────────────────────

    fn emit_diagnostic(diag: CardDiagnostic) {
        tracing::debug!(
            kind = ?diag.kind,
            owner = ?diag.owner.as_ref().map(|o| o.0.as_ref()),
            channel = ?diag.channel,
            generation = diag.generation,
            "card diagnostic"
        );
    }
}

// ────────────────────────────────────────────────────────────────
// Tests
// ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::shell_surfaces::SurfaceLifecycle;
    use gpui::{TestAppContext, point, size};

    struct TestProvider {
        owner: CardOwnerId,
        capabilities: super::super::model::CardCapabilities,
    }

    impl CardProvider for TestProvider {
        fn owner_id(&self) -> CardOwnerId {
            self.owner.clone()
        }

        fn capabilities(&self) -> super::super::model::CardCapabilities {
            self.capabilities
        }

        fn size_tier(&self) -> super::super::model::CardSizeTier {
            super::super::model::CardSizeTier::Compact
        }

        fn anchor_bounds(&self, _cx: &App) -> Option<(Bounds<Pixels>, DisplayId)> {
            Some((
                Bounds::new(point(px(40.), px(40.)), size(px(32.), px(32.))),
                DisplayId::new(0),
            ))
        }

        fn render_content(
            &self,
            _channel: CardChannel,
            _window: &mut gpui::Window,
            _cx: &mut App,
        ) -> gpui::AnyElement {
            gpui::div().into_any_element()
        }
    }

    // Helper: install ShellRuntime for test.
    fn setup(cx: &mut TestAppContext) {
        cx.update(|app| {
            shilpo_ui::init(app);
            ShellRuntime::install_for_test(app);
            for id in ["battery", "test-battery", "test-workspaces"] {
                CardCoordinator::register_provider(
                    app,
                    Arc::new(TestProvider {
                        owner: CardOwnerId::new(id),
                        capabilities: super::super::model::CardCapabilities {
                            hover: true,
                            click: true,
                        },
                    }),
                );
            }
        });
    }

    #[gpui::test]
    fn unsupported_hover_is_rejected_before_reaching_the_reducer(cx: &mut TestAppContext) {
        setup(cx);
        let owner = CardOwnerId::new("click-only");
        cx.update(|app| {
            CardCoordinator::register_provider(
                app,
                Arc::new(TestProvider {
                    owner: owner.clone(),
                    capabilities: super::super::model::CardCapabilities {
                        hover: false,
                        click: true,
                    },
                }),
            );
            CardCoordinator::dispatch(
                app,
                CardRequest::SourceEnter {
                    owner: owner.clone(),
                },
            );
            assert_eq!(
                CardCoordinator::source_state(app, &owner),
                CardSourceState::Idle
            );
            assert!(!CardCoordinator::holds_bar_visibility(app));
        });
    }

    #[gpui::test]
    fn persistent_card_lifecycle_snapshot_reflects_open_state(cx: &mut TestAppContext) {
        setup(cx);

        cx.update(|cx| {
            let owner = CardOwnerId::new("test-battery");
            // Initially closed
            assert_eq!(
                CardCoordinator::persistent_lifecycle(cx),
                SurfaceLifecycle::Closed
            );

            // Open persistent channel via toggle
            CardCoordinator::dispatch(
                cx,
                CardRequest::PersistentToggle {
                    owner: owner.clone(),
                },
            );
        });

        cx.update(|cx| {
            // Should now be Open (even without a provider, state machine still transitions)
            let lifecycle = CardCoordinator::persistent_lifecycle(cx);
            assert!(
                matches!(lifecycle, SurfaceLifecycle::Open { .. }),
                "Persistent lifecycle should be Open after toggle"
            );
        });
    }

    #[gpui::test]
    fn preview_card_lifecycle_snapshot_reflects_timer_state(cx: &mut TestAppContext) {
        setup(cx);

        cx.update(|cx| {
            let owner = CardOwnerId::new("test-workspaces");
            CardCoordinator::dispatch(
                cx,
                CardRequest::SourceEnter {
                    owner: owner.clone(),
                },
            );
        });

        // Timer started — preview still closed until timer fires
        cx.update(|cx| {
            let preview_lifecycle = CardCoordinator::preview_lifecycle(cx);
            // The state machine opened ChannelLifecycle::Closed (timer pending, not yet open)
            assert_eq!(
                preview_lifecycle,
                SurfaceLifecycle::Closed,
                "Preview should still be closed while timer is pending"
            );
        });
    }

    #[gpui::test]
    fn holds_bar_visibility_true_when_persistent_open(cx: &mut TestAppContext) {
        setup(cx);

        cx.update(|cx| {
            assert!(!CardCoordinator::holds_bar_visibility(cx));
            CardCoordinator::dispatch(
                cx,
                CardRequest::PersistentToggle {
                    owner: CardOwnerId::new("battery"),
                },
            );
            assert!(
                CardCoordinator::holds_bar_visibility(cx),
                "Bar hold should be true when persistent card is open"
            );
        });
    }

    #[gpui::test]
    fn holds_bar_visibility_false_after_close(cx: &mut TestAppContext) {
        setup(cx);

        cx.update(|cx| {
            let owner = CardOwnerId::new("battery");
            CardCoordinator::dispatch(
                cx,
                CardRequest::PersistentToggle {
                    owner: owner.clone(),
                },
            );
            CardCoordinator::dispatch(
                cx,
                CardRequest::PersistentToggle {
                    owner: owner.clone(),
                },
            );
            assert!(CardCoordinator::holds_bar_visibility(cx));
        });
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(250));
        cx.run_until_parked();
        cx.update(|cx| assert!(!CardCoordinator::holds_bar_visibility(cx)));
    }

    #[gpui::test]
    fn preview_open_timer_fires_after_350ms(cx: &mut TestAppContext) {
        setup(cx);

        cx.update(|cx| {
            let owner = CardOwnerId::new("battery");
            CardCoordinator::dispatch(cx, CardRequest::SourceEnter { owner });
        });

        // Advance past 350ms
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(400));
        cx.run_until_parked();

        cx.update(|cx| {
            let preview = CardCoordinator::preview_lifecycle(cx);
            // After timer fires, preview should be Open
            assert!(
                matches!(preview, SurfaceLifecycle::Open { .. }),
                "Preview should be Open after 350ms timer: got {:?}",
                preview
            );
        });
    }

    #[gpui::test]
    fn source_state_idle_initially(cx: &mut TestAppContext) {
        setup(cx);

        cx.update(|cx| {
            let owner = CardOwnerId::new("battery");
            assert_eq!(
                CardCoordinator::source_state(cx, &owner),
                CardSourceState::Idle
            );
        });
    }

    #[gpui::test]
    fn source_state_persistent_open_after_toggle(cx: &mut TestAppContext) {
        setup(cx);

        cx.update(|cx| {
            let owner = CardOwnerId::new("battery");
            CardCoordinator::dispatch(
                cx,
                CardRequest::PersistentToggle {
                    owner: owner.clone(),
                },
            );
            assert_eq!(
                CardCoordinator::source_state(cx, &owner),
                CardSourceState::PersistentOpen
            );
        });
    }

    #[gpui::test]
    fn overview_opened_clears_persistent_card(cx: &mut TestAppContext) {
        setup(cx);

        cx.update(|cx| {
            CardCoordinator::dispatch(
                cx,
                CardRequest::PersistentToggle {
                    owner: CardOwnerId::new("battery"),
                },
            );
            assert!(CardCoordinator::holds_bar_visibility(cx));
            CardCoordinator::dispatch(cx, CardRequest::OverviewOpened);
            assert!(CardCoordinator::holds_bar_visibility(cx));
        });
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(250));
        cx.run_until_parked();
        cx.update(|cx| assert!(!CardCoordinator::holds_bar_visibility(cx)));
    }

    #[gpui::test]
    fn shutdown_clears_all_channels(cx: &mut TestAppContext) {
        setup(cx);

        cx.update(|cx| {
            CardCoordinator::dispatch(
                cx,
                CardRequest::PersistentToggle {
                    owner: CardOwnerId::new("battery"),
                },
            );
            CardCoordinator::dispatch(cx, CardRequest::Shutdown);
            assert!(matches!(
                CardCoordinator::persistent_lifecycle(cx),
                SurfaceLifecycle::Closing { .. }
            ));
        });
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(250));
        cx.run_until_parked();
        cx.update(|cx| {
            assert_eq!(
                CardCoordinator::persistent_lifecycle(cx),
                SurfaceLifecycle::Closed
            );
        });
    }
}
