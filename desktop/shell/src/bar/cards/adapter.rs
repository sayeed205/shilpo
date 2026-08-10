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

use crate::runtime::{ShellRuntime, ShellSurfaces};

use super::{
    band::CardBandView,
    model::{
        CardChannel, CardDismissReason, CardEffect, CardOwnerId, CardRequest, CardSourceId,
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

#[derive(Clone, Copy)]
enum PositionMode {
    Show,
    Reposition,
    Retarget,
}

impl CardCoordinator {
    // ── Public API ────────────────────────────────────────────────

    /// Register a built-in card provider. Replaces any previously registered
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

    /// Remove a provider. Dispatches `OwnerRemoved` dismiss into the state
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

        let refresh_source_state = !matches!(
            &request,
            CardRequest::AnchorUpdate { .. } | CardRequest::PlacementUpdated { .. }
        );

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
        if refresh_source_state {
            ShellSurfaces::refresh_bars(cx);
        }
    }

    fn request_is_supported(cx: &App, request: &CardRequest) -> bool {
        let (owner, hover, click) = match request {
            CardRequest::SourceEnter { source }
            | CardRequest::SourceLeave { source }
            | CardRequest::PreviewEnter { source }
            | CardRequest::PreviewLeave { source } => (&source.owner, true, false),
            CardRequest::PersistentToggle { source }
            | CardRequest::PersistentToggleAt { source, .. } => (&source.owner, false, true),
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

    /// Query the source state for a given source.
    #[allow(dead_code)]
    pub(crate) fn source_state(cx: &App, source: &CardSourceId) -> CardSourceState {
        if !cx.has_global::<ShellRuntime>() {
            return CardSourceState::Idle;
        }
        cx.global::<ShellRuntime>()
            .shell_surfaces()
            .card_coordinator
            .state
            .source_state(source)
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
        let (provider, candidate_sources) = {
            let coordinator = &cx
                .global::<ShellRuntime>()
                .shell_surfaces()
                .card_coordinator;
            let provider = coordinator.providers.get(owner).cloned();
            let sources = [
                coordinator.state.persistent.source.clone(),
                coordinator.state.preview.source.clone(),
                coordinator.state.preview.pending_open_source.clone(),
            ]
            .into_iter()
            .flatten()
            .filter(|source| &source.owner == owner)
            .collect::<Vec<_>>();
            (provider, sources)
        };

        if let Some(provider) = provider {
            for source in candidate_sources {
                if !provider.source_available(&source, cx) {
                    Self::dispatch(cx, CardRequest::AnchorRemoved { source });
                }
            }
        }

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
                    if slot.source.as_ref().map(|s| &s.owner) != Some(owner) || !slot.is_open() {
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
            if let Err(error) = handle.update(cx, |_, _, cx| cx.notify()) {
                tracing::debug!(
                    ?error,
                    window_id = ?handle.window_id(),
                    surface = "card-band",
                    "stale window handle on card refresh"
                );
            }
        }
    }

    pub(crate) fn handle_window_closed(&mut self, window_id: gpui::WindowId) {
        self.persistent_bands
            .retain(|_, handle| handle.window_id() != window_id);
        self.preview_bands
            .retain(|_, handle| handle.window_id() != window_id);
    }

    pub(crate) fn destroy_bands_for_display(cx: &mut App, display_id: DisplayId) {
        Self::dispatch(cx, CardRequest::DisplayRemoved { display_id });
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

    pub(crate) fn destroy_all_bands(cx: &mut App) {
        Self::dispatch(cx, CardRequest::Shutdown);
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
                source,
                generation,
            } => {
                tracing::debug!(
                    source = %source,
                    channel = ?channel,
                    generation,
                    "card channel opening"
                );
                Self::open_channel(cx, channel, source, generation);
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

            CardEffect::RepositionChannel { channel, source } => {
                Self::position_channel(cx, channel, source, PositionMode::Reposition);
            }

            CardEffect::RetargetChannel {
                channel, source, ..
            } => {
                Self::position_channel(cx, channel, source, PositionMode::Retarget);
            }

            CardEffect::StartTimer { kind, generation } => {
                Self::start_timer(cx, kind, generation);
            }

            CardEffect::CancelTimers { kind } => {
                Self::cancel_timers(cx, kind);
            }

            CardEffect::CaptureFocus => {
                Self::capture_focus(cx);
            }

            CardEffect::RestoreFocus => {
                Self::restore_focus(cx);
            }

            CardEffect::UpdateBarHold { hold } => {
                Self::update_bar_hold(cx, hold);
            }

            CardEffect::Diagnostic(diag) => {
                tracing::debug!(
                    kind = ?diag.kind,
                    source = ?diag.source.as_ref().map(|s| s.to_string()),
                    channel = ?diag.channel,
                    generation = diag.generation,
                    "card diagnostic"
                );
            }
        }
    }

    // ── Timer management ──────────────────────────────────────────

    fn start_timer(cx: &mut App, kind: TimerKind, generation: u64) {
        let duration = match kind {
            TimerKind::PreviewOpen => std::time::Duration::from_millis(350),
            TimerKind::PreviewClose => std::time::Duration::from_millis(200),
        };

        let task = cx.spawn(async move |cx| {
            cx.background_executor().timer(duration).await;
            cx.update(move |cx| {
                let request = match kind {
                    TimerKind::PreviewOpen => CardRequest::PreviewOpenTimer { generation },
                    TimerKind::PreviewClose => CardRequest::PreviewCloseTimer { generation },
                };
                Self::dispatch(cx, request);
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

    fn cancel_timers(cx: &mut App, kind: TimerKind) {
        let coordinator = &mut cx
            .global_mut::<ShellRuntime>()
            .shell_surfaces_mut()
            .card_coordinator;
        match kind {
            TimerKind::PreviewOpen => coordinator.preview_open_task = None,
            TimerKind::PreviewClose => coordinator.preview_close_task = None,
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

    // ── Focus management ──────────────────────────────────────────

    fn capture_focus(cx: &mut App) {
        let current = ShellSurfaces::compositor_snapshot(cx).focused_window_id;
        tracing::debug!(focused_window_id = ?current, "capturing focus for persistent card");
        cx.global_mut::<ShellRuntime>()
            .shell_surfaces_mut()
            .card_coordinator
            .prior_focused_window = current;
    }

    fn restore_focus(cx: &mut App) {
        let prior = cx
            .global_mut::<ShellRuntime>()
            .shell_surfaces_mut()
            .card_coordinator
            .prior_focused_window
            .take();

        if let Some(win_id) = prior {
            tracing::debug!(win_id, "restoring focus after persistent card close");
            let _ = ShellRuntime::dispatch_action(
                cx,
                crate::actions::ActionInvocation::FocusWindow(win_id),
            );
        } else {
            tracing::debug!("no prior focused window to restore");
        }
    }

    fn update_bar_hold(cx: &mut App, hold: bool) {
        tracing::debug!(hold, "updating bar visibility hold from card system");
        ShellSurfaces::refresh_bars(cx);
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

            if let Some(band_handle) = handle
                && let Err(error) = band_handle.update(cx, |band, window, cx| {
                    if channel == CardChannel::Persistent
                        && reason == CardDismissReason::SourceToggle
                    {
                        window.activate_window();
                    }
                    band.hide(cx);
                })
            {
                tracing::debug!(
                    ?error,
                    window_id = ?band_handle.window_id(),
                    surface = "card-band",
                    "stale window handle on card channel close"
                );
            }
        }
    }

    // ── Channel surface management ────────────────────────────────

    fn open_channel(cx: &mut App, channel: CardChannel, source: CardSourceId, _generation: u64) {
        Self::position_channel(cx, channel, source, PositionMode::Show);
    }

    fn position_channel(
        cx: &mut App,
        channel: CardChannel,
        source: CardSourceId,
        mode: PositionMode,
    ) {
        let provider_exists = {
            let coordinator = &cx
                .global::<ShellRuntime>()
                .shell_surfaces()
                .card_coordinator;
            coordinator.providers.contains_key(&source.owner)
        };
        if !provider_exists {
            tracing::warn!(source = %source, "card open requested but provider not found");
            Self::dismiss_failed_open(cx, channel);
            return;
        }

        let (state_anchor, preferred_size) = {
            let coordinator = &cx
                .global::<ShellRuntime>()
                .shell_surfaces()
                .card_coordinator;
            let provider = coordinator.providers.get(&source.owner);
            let preferred = provider.map(|p| p.preferred_size(channel, &source, cx));
            let slot = match channel {
                CardChannel::Persistent => &coordinator.state.persistent,
                CardChannel::Preview => &coordinator.state.preview,
            };
            let anchor = (slot.source.as_ref() == Some(&source))
                .then(|| slot.anchor_bounds.zip(slot.display_id))
                .flatten();
            (anchor, preferred)
        };

        let Some((anchor_bounds, display_id)) = state_anchor else {
            tracing::warn!(source = %source, "card open: source has no anchor bounds, skipping");
            Self::dismiss_failed_open(cx, channel);
            return;
        };

        let requested_size = preferred_size.unwrap_or_else(|| gpui::size(px(360.0), px(280.0)));

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

        let (monitor_bounds, scale) = Self::resolve_monitor_bounds_and_scale(cx, display_id);

        let placement_input = PlacementInput {
            monitor_bounds,
            bar_edge,
            bar_thickness,
            source_bounds: anchor_bounds,
            requested_size,
            collision_bounds,
            scale,
        };

        let placement = compute_placement(&placement_input);

        match placement {
            PlacementResult::Suppressed { reason } => {
                tracing::debug!(
                    source = %source,
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

                let Some(band_handle) =
                    Self::ensure_band(cx, channel, display_id, &band_geometry, bar_edge)
                else {
                    Self::dismiss_failed_open(cx, channel);
                    return;
                };

                let band_local_bounds = gpui::Bounds {
                    origin: gpui::Point {
                        x: card_bounds.origin.x - band_geometry.bounds.origin.x,
                        y: card_bounds.origin.y - band_geometry.bounds.origin.y,
                    },
                    size: card_bounds.size,
                };

                tracing::debug!(
                    source = %source,
                    channel = ?channel,
                    card_x = card_bounds.origin.x.as_f32(),
                    card_y = card_bounds.origin.y.as_f32(),
                    card_w = card_bounds.size.width.as_f32(),
                    card_h = card_bounds.size.height.as_f32(),
                    "card placement computed"
                );

                let source_clone = source.clone();
                if let Err(error) = band_handle.update(cx, move |band, window, cx| {
                    let provider = cx
                        .global::<ShellRuntime>()
                        .shell_surfaces()
                        .card_coordinator
                        .providers
                        .get(&source_clone.owner)
                        .map(|_| ());

                    if provider.is_some() {
                        let content_source = source_clone.clone();
                        let ch = channel;
                        let content_box: super::provider::CardContentRenderFn =
                            Box::new(move |window, cx| {
                                let provider = cx
                                    .global::<ShellRuntime>()
                                    .shell_surfaces()
                                    .card_coordinator
                                    .providers
                                    .get(&content_source.owner)
                                    .cloned();

                                if let Some(provider) = provider {
                                    provider.render_content(ch, &content_source, window, cx)
                                } else {
                                    gpui::div().into_any_element()
                                }
                            });
                        match mode {
                            PositionMode::Show => {
                                band.show(band_local_bounds, source_clone.clone(), content_box, cx)
                            }
                            PositionMode::Reposition => band.reposition(band_local_bounds, cx),
                            PositionMode::Retarget => band.retarget(
                                band_local_bounds,
                                source_clone.clone(),
                                content_box,
                                cx,
                            ),
                        }

                        if channel == CardChannel::Persistent
                            && let Some(ref handle) = band.focus_handle.clone()
                        {
                            window.activate_window();
                            handle.focus(window, cx);
                        }
                    }
                }) {
                    tracing::debug!(
                        ?error,
                        window_id = ?band_handle.window_id(),
                        surface = "card-band",
                        "stale window handle on card position"
                    );
                }

                Self::dispatch(
                    cx,
                    CardRequest::PlacementUpdated {
                        source,
                        channel,
                        bounds: card_bounds,
                        display_id,
                    },
                );
            }
        }
    }

    fn ensure_band(
        cx: &mut App,
        channel: CardChannel,
        display_id: DisplayId,
        band_geometry: &BandGeometry,
        bar_edge: BarPosition,
    ) -> Option<WindowHandle<CardBandView>> {
        let existing = {
            let coordinator = &cx
                .global::<ShellRuntime>()
                .shell_surfaces()
                .card_coordinator;
            match channel {
                CardChannel::Persistent => coordinator.persistent_bands.get(&display_id).copied(),
                CardChannel::Preview => coordinator.preview_bands.get(&display_id).copied(),
            }
        };

        if let Some(handle) = existing {
            if handle.update(cx, |_, _, _| ()).is_ok() {
                tracing::debug!(channel = ?channel, display = ?display_id, "card band reused");
                return Some(handle);
            }
            tracing::warn!(channel = ?channel, display = ?display_id, "stale card band handle detected, re-creating");
            let coordinator = &mut cx
                .global_mut::<ShellRuntime>()
                .shell_surfaces_mut()
                .card_coordinator;
            match channel {
                CardChannel::Persistent => {
                    coordinator.persistent_bands.remove(&display_id);
                }
                CardChannel::Preview => {
                    coordinator.preview_bands.remove(&display_id);
                }
            }
        }

        tracing::debug!(channel = ?channel, display = ?display_id, "card band created");

        let reduced_motion = ShellRuntime::active_config(cx).theme.reduced_motion;
        let keyboard_interactivity = match channel {
            CardChannel::Persistent => KeyboardInteractivity::OnDemand,
            CardChannel::Preview => KeyboardInteractivity::None,
        };
        let surface_bounds = Bounds::new(gpui::point(px(0.0), px(0.0)), band_geometry.bounds.size);
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

        let surface_id = format!("band-{channel:?}-{display_id:?}");
        let band_view = match cx.open_window(options, move |window, cx| {
            let focus_handle = (channel == CardChannel::Persistent).then(|| cx.focus_handle());
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
                tracing::warn!(error = %error, channel = ?channel, display = ?display_id, "failed to open card band surface");
                return None;
            }
        };

        let coordinator = &mut cx
            .global_mut::<ShellRuntime>()
            .shell_surfaces_mut()
            .card_coordinator;
        match channel {
            CardChannel::Persistent => {
                coordinator.persistent_bands.insert(display_id, band_view);
            }
            CardChannel::Preview => {
                coordinator.preview_bands.insert(display_id, band_view);
            }
        }

        Some(band_view)
    }

    fn band_anchor(bar_edge: BarPosition) -> Anchor {
        match bar_edge {
            BarPosition::Top => Anchor::TOP | Anchor::LEFT | Anchor::RIGHT,
            BarPosition::Bottom => Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT,
            BarPosition::Left => Anchor::LEFT | Anchor::TOP | Anchor::BOTTOM,
            BarPosition::Right => Anchor::RIGHT | Anchor::TOP | Anchor::BOTTOM,
        }
    }

    fn band_margin(
        _bar_edge: BarPosition,
        _bar_thickness: Pixels,
    ) -> (Pixels, Pixels, Pixels, Pixels) {
        // Card placement already includes the bar thickness and configured gap.
        (px(0.0), px(0.0), px(0.0), px(0.0))
    }

    fn resolve_monitor_bounds_and_scale(
        cx: &App,
        display_id: DisplayId,
    ) -> (gpui::Bounds<Pixels>, Option<f32>) {
        let snapshot = ShellSurfaces::compositor_snapshot(cx);

        let gpui_display = cx
            .displays()
            .into_iter()
            .find(|d| d.id() == display_id)
            .or_else(|| cx.primary_display());

        let gpui_bounds = gpui_display.as_ref().map(|d| d.bounds());

        if let Some(ref bounds) = gpui_bounds
            && let Some(output_name) =
                ShellSurfaces::output_name_for_bounds(*bounds, &snapshot.outputs)
            && let Some(output) = snapshot.outputs.iter().find(|o| o.name == output_name)
        {
            let logical_bounds = gpui::Bounds::new(
                gpui::point(
                    px(output.logical_position.0 as f32),
                    px(output.logical_position.1 as f32),
                ),
                gpui::size(
                    px(output.logical_size.0 as f32),
                    px(output.logical_size.1 as f32),
                ),
            );
            return (logical_bounds, Some(output.scale as f32));
        }

        if let Some(output) = snapshot.outputs.first() {
            let logical_bounds = gpui::Bounds::new(
                gpui::point(
                    px(output.logical_position.0 as f32),
                    px(output.logical_position.1 as f32),
                ),
                gpui::size(
                    px(output.logical_size.0 as f32),
                    px(output.logical_size.1 as f32),
                ),
            );
            return (logical_bounds, Some(output.scale as f32));
        }

        if let Some(bounds) = gpui_bounds {
            return (bounds, None);
        }

        (
            gpui::Bounds::new(
                gpui::point(px(0.0), px(0.0)),
                gpui::size(px(1920.0), px(1080.0)),
            ),
            None,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::shell_surfaces::SurfaceLifecycle;
    use gpui::{Size, TestAppContext, size};

    fn test_source(owner_id: &str) -> CardSourceId {
        CardSourceId::singleton(owner_id)
    }

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

        fn preferred_size(
            &self,
            _channel: CardChannel,
            _source: &CardSourceId,
            _cx: &App,
        ) -> Size<Pixels> {
            size(px(280.0), px(240.0))
        }

        fn render_content(
            &self,
            _channel: CardChannel,
            _source: &CardSourceId,
            _window: &mut gpui::Window,
            _cx: &mut App,
        ) -> gpui::AnyElement {
            gpui::div().into_any_element()
        }
    }

    fn publish_anchor(cx: &mut App, source: &CardSourceId) {
        let display_id = cx
            .displays()
            .first()
            .map(|d| d.id())
            .unwrap_or_else(|| DisplayId::new(0));
        CardCoordinator::dispatch(
            cx,
            CardRequest::AnchorUpdate {
                source: source.clone(),
                bounds: gpui::Bounds::new(
                    gpui::point(px(100.0), px(0.0)),
                    gpui::size(px(40.0), px(30.0)),
                ),
                display_id,
            },
        );
    }

    fn setup(cx: &mut TestAppContext) {
        cx.update(|app| {
            shilpo_ui::init(app);
            ShellRuntime::install_for_test(app);
            for id in ["battery", "test-battery", "test-workspaces"] {
                let owner = CardOwnerId::new(id);
                CardCoordinator::register_provider(
                    app,
                    Arc::new(TestProvider {
                        owner: owner.clone(),
                        capabilities: super::super::model::CardCapabilities {
                            hover: true,
                            click: true,
                        },
                    }),
                );
                publish_anchor(app, &CardSourceId::singleton(owner));
            }
        });
    }

    #[gpui::test]
    fn unsupported_hover_is_rejected_before_reaching_the_reducer(cx: &mut TestAppContext) {
        setup(cx);
        let owner = CardOwnerId::new("click-only");
        let source = CardSourceId::singleton(owner.clone());
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
                    source: source.clone(),
                },
            );
            assert_eq!(
                CardCoordinator::source_state(app, &source),
                CardSourceState::Idle
            );
            assert!(!CardCoordinator::holds_bar_visibility(app));
        });
    }

    #[gpui::test]
    fn persistent_card_lifecycle_snapshot_reflects_open_state(cx: &mut TestAppContext) {
        setup(cx);

        cx.update(|cx| {
            let source = test_source("test-battery");
            assert_eq!(
                CardCoordinator::persistent_lifecycle(cx),
                SurfaceLifecycle::Closed
            );

            CardCoordinator::dispatch(
                cx,
                CardRequest::PersistentToggle {
                    source: source.clone(),
                },
            );
        });

        cx.update(|cx| {
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
            let source = test_source("test-workspaces");
            CardCoordinator::dispatch(
                cx,
                CardRequest::SourceEnter {
                    source: source.clone(),
                },
            );
        });

        cx.update(|cx| {
            let preview_lifecycle = CardCoordinator::preview_lifecycle(cx);
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
                    source: test_source("battery"),
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
            let source = test_source("battery");
            CardCoordinator::dispatch(
                cx,
                CardRequest::PersistentToggle {
                    source: source.clone(),
                },
            );
            CardCoordinator::dispatch(
                cx,
                CardRequest::PersistentToggle {
                    source: source.clone(),
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
            let source = test_source("battery");
            CardCoordinator::dispatch(cx, CardRequest::SourceEnter { source });
        });

        cx.executor()
            .advance_clock(std::time::Duration::from_millis(400));
        cx.run_until_parked();

        cx.update(|cx| {
            let preview = CardCoordinator::preview_lifecycle(cx);
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
            let source = test_source("battery");
            assert_eq!(
                CardCoordinator::source_state(cx, &source),
                CardSourceState::Idle
            );
        });
    }

    #[gpui::test]
    fn source_state_persistent_open_after_toggle(cx: &mut TestAppContext) {
        setup(cx);

        cx.update(|cx| {
            let source = test_source("battery");
            CardCoordinator::dispatch(
                cx,
                CardRequest::PersistentToggle {
                    source: source.clone(),
                },
            );
            assert_eq!(
                CardCoordinator::source_state(cx, &source),
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
                    source: test_source("battery"),
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
                    source: test_source("battery"),
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
