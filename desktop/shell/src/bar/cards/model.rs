//! Pure card state machine: types, two-channel state, and reducer.
//!
//! This module is intentionally free of GPUI runtime dependencies so that all
//! transition logic can be covered by plain `#[test]` cases without a display
//! server or app context.

use gpui::{Bounds, DisplayId, Pixels, SharedString};

// ────────────────────────────────────────────────────────────────
// Core identity / configuration types
// ────────────────────────────────────────────────────────────────

/// Stable identity key for a card provider (e.g. `"battery"`, `"workspaces"`).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct CardOwnerId(pub SharedString);

impl std::fmt::Display for CardOwnerId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl CardOwnerId {
    #[allow(
        dead_code,
        reason = "constructed by built-in providers added after the coordinator foundation"
    )]
    pub fn new(id: impl Into<SharedString>) -> Self {
        Self(id.into())
    }
}

/// Which surface channel carries a card.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CardChannel {
    /// User-clicked card — accepts keyboard focus, globally exclusive.
    Persistent,
    /// Hover-preview card — non-interactive, globally exclusive.
    Preview,
}

/// Independently declared interaction capabilities for a provider.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CardCapabilities {
    /// Widget participates in hover-preview.
    pub hover: bool,
    /// Widget participates in persistent-click.
    pub click: bool,
}

/// Maximum logical-pixel card dimensions per tier.
///
/// Content may shrink below its tier maximum.  All dimensions are later
/// clamped to available monitor space by the placement engine.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
#[allow(
    dead_code,
    reason = "all tiers are part of the built-in provider contract"
)]
pub enum CardSizeTier {
    /// 280 × 240
    Compact,
    /// 360 × 480
    #[default]
    Standard,
    /// 480 × 640
    Expanded,
}

impl CardSizeTier {
    /// Maximum width in logical pixels for this tier.
    pub const fn max_width(self) -> f32 {
        match self {
            CardSizeTier::Compact => 280.0,
            CardSizeTier::Standard => 360.0,
            CardSizeTier::Expanded => 480.0,
        }
    }

    /// Maximum height in logical pixels for this tier.
    pub const fn max_height(self) -> f32 {
        match self {
            CardSizeTier::Compact => 240.0,
            CardSizeTier::Standard => 480.0,
            CardSizeTier::Expanded => 640.0,
        }
    }
}

/// Source-widget state visible to later widget rendering code.
///
/// Widgets can use this to render their icon/text/ghost-button with the
/// correct selection / active cues without knowing placement internals.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum CardSourceState {
    #[default]
    Idle,
    HoverPending,
    PreviewOpen,
    PersistentOpen,
}

/// Why a channel was dismissed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CardDismissReason {
    SourceToggle,
    Escape,
    FocusLost,
    OutsideClick,
    OverviewOpened,
    BarClosed,
    DisplayRemoved,
    Shutdown,
    OwnerRemoved,
    SourceDisappeared,
    Explicit,
}

// ────────────────────────────────────────────────────────────────
// State machine input
// ────────────────────────────────────────────────────────────────

/// Semantic events the adapter dispatches into the state machine.
#[derive(Clone, Debug, PartialEq)]
#[allow(
    dead_code,
    reason = "source events are the integration seam for follow-up widget providers"
)]
pub enum CardRequest {
    // ── pointer / source events ──────────────────────────────────
    SourceEnter {
        owner: CardOwnerId,
    },
    SourceLeave {
        owner: CardOwnerId,
    },
    PreviewEnter {
        owner: CardOwnerId,
    },
    PreviewLeave {
        owner: CardOwnerId,
    },
    PersistentToggle {
        owner: CardOwnerId,
    },

    // ── anchor geometry update ───────────────────────────────────
    AnchorUpdate {
        owner: CardOwnerId,
        bounds: Bounds<Pixels>,
        display_id: DisplayId,
    },
    AnchorRemoved {
        owner: CardOwnerId,
    },
    PlacementUpdated {
        owner: CardOwnerId,
        channel: CardChannel,
        bounds: Bounds<Pixels>,
        display_id: DisplayId,
    },

    // ── system lifecycle ─────────────────────────────────────────
    Dismiss {
        channel: CardChannel,
        reason: CardDismissReason,
    },
    DisplayRemoved {
        display_id: DisplayId,
    },
    OwnerRemoved {
        owner: CardOwnerId,
    },
    OverviewOpened,
    BarClosed,
    Shutdown,

    // ── timer completions (generation guarded) ───────────────────
    PreviewOpenTimer {
        generation: u64,
    },
    PreviewCloseTimer {
        generation: u64,
    },
    CloseAnimationFinished {
        channel: CardChannel,
        generation: u64,
    },
}

// ────────────────────────────────────────────────────────────────
// Pure side-effects emitted by the reducer
// ────────────────────────────────────────────────────────────────

/// Signals the GPUI adapter must execute after each `CardState::reduce` call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CardEffect {
    OpenChannel {
        channel: CardChannel,
        owner: CardOwnerId,
        generation: u64,
    },
    CloseChannel {
        channel: CardChannel,
        reason: CardDismissReason,
        display_id: Option<DisplayId>,
        generation: u64,
    },
    RepositionChannel {
        channel: CardChannel,
        owner: CardOwnerId,
    },
    StartTimer {
        kind: TimerKind,
        generation: u64,
    },
    CancelTimers {
        kind: TimerKind,
    },
    CaptureFocus,
    RestoreFocus,
    UpdateBarHold {
        hold: bool,
    },
    Diagnostic(CardDiagnostic),
}

/// Which timer class to start or cancel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimerKind {
    PreviewOpen,
    PreviewClose,
}

/// Structured diagnostic payload (never contains sensitive domain values).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CardDiagnostic {
    pub kind: DiagnosticKind,
    pub owner: Option<CardOwnerId>,
    pub channel: Option<CardChannel>,
    pub generation: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DiagnosticKind {
    ChannelOpened,
    ChannelClosed,
    TimerStarted,
    TimerStale,
    FocusCaptured,
    FocusRestored,
}

// ────────────────────────────────────────────────────────────────
// Channel slot
// ────────────────────────────────────────────────────────────────

/// Lifecycle of a single channel slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ChannelLifecycle {
    #[default]
    Closed,
    Open,
    Closing,
}

/// State for one channel (persistent or preview).
#[derive(Clone, Debug, Default)]
pub struct ChannelSlot {
    pub owner: Option<CardOwnerId>,
    pub lifecycle: ChannelLifecycle,
    /// Monotonically increasing generation token for staleness checks.
    pub generation: u64,
    pub display_id: Option<DisplayId>,
    pub anchor_bounds: Option<Bounds<Pixels>>,
    pub card_bounds: Option<Bounds<Pixels>>,
    /// Set when a preview-open timer is in flight.
    pub pending_open_generation: Option<u64>,
    /// Prospective hover owner while the open-intent timer is in flight.
    pub pending_open_owner: Option<CardOwnerId>,
    /// Set when a preview-close timer is in flight.
    pub pending_close_generation: Option<u64>,
}

impl ChannelSlot {
    pub fn is_open(&self) -> bool {
        self.lifecycle == ChannelLifecycle::Open
    }

    pub fn is_closed(&self) -> bool {
        self.lifecycle == ChannelLifecycle::Closed
    }

    fn holds_visibility(&self) -> bool {
        self.lifecycle != ChannelLifecycle::Closed
    }
}

// ────────────────────────────────────────────────────────────────
// Card state + reducer
// ────────────────────────────────────────────────────────────────

/// Full two-channel card state owned by `CardCoordinator`.
#[derive(Default)]
pub struct CardState {
    pub persistent: ChannelSlot,
    pub preview: ChannelSlot,
    /// Whether a compositor focus capture has been performed for the current
    /// persistent card session (cleared after final persistent dismiss).
    pub focus_captured: bool,
    /// Holds bar visibility whenever any channel is non-Closed.
    pub bar_visibility_hold: bool,
    /// Monotonically incrementing counter shared across all generations.
    generation_counter: u64,
}

impl CardState {
    fn next_generation(&mut self) -> u64 {
        self.generation_counter = self.generation_counter.wrapping_add(1);
        self.generation_counter
    }

    /// Process one `CardRequest` and return all resulting `CardEffect`s.
    pub fn reduce(&mut self, request: CardRequest) -> Vec<CardEffect> {
        let mut effects: Vec<CardEffect> = Vec::new();

        match request {
            // ─── Pointer / source events ──────────────────────────────
            CardRequest::SourceEnter { owner } => {
                // If persistent for this same owner is open, suppress preview.
                let persistent_blocks = self.persistent.owner.as_ref().is_some_and(|o| o == &owner)
                    && self.persistent.is_open();

                if persistent_blocks {
                    return effects;
                }

                // Only schedule preview if preview channel is closed and
                // owner supports hover.
                if self.preview.is_closed() || self.preview.owner.as_ref() != Some(&owner) {
                    let tok = self.next_generation();
                    self.preview.pending_open_generation = Some(tok);
                    self.preview.pending_open_owner = Some(owner.clone());
                    self.preview.pending_close_generation = None;
                    effects.push(CardEffect::CancelTimers {
                        kind: TimerKind::PreviewClose,
                    });
                    effects.push(CardEffect::StartTimer {
                        kind: TimerKind::PreviewOpen,
                        generation: tok,
                    });
                    effects.append(&mut self.update_hold());
                    effects.push(CardEffect::Diagnostic(CardDiagnostic {
                        kind: DiagnosticKind::TimerStarted,
                        owner: Some(owner),
                        channel: Some(CardChannel::Preview),
                        generation: tok,
                    }));
                }
            }

            CardRequest::SourceLeave { owner } => {
                // Cancel pending open if it's for this owner.
                if self.preview.pending_open_generation.is_some()
                    && self.preview.pending_open_owner.as_ref() == Some(&owner)
                {
                    self.preview.pending_open_generation = None;
                    self.preview.pending_open_owner = None;
                    effects.push(CardEffect::CancelTimers {
                        kind: TimerKind::PreviewOpen,
                    });
                    effects.append(&mut self.update_hold());
                }

                // If preview is open for this owner, start close timer.
                if self.preview.owner.as_ref() == Some(&owner) && self.preview.is_open() {
                    let tok = self.next_generation();
                    self.preview.pending_close_generation = Some(tok);
                    effects.push(CardEffect::StartTimer {
                        kind: TimerKind::PreviewClose,
                        generation: tok,
                    });
                }
            }

            CardRequest::PreviewEnter { owner } => {
                // Pointer entered the preview card surface — cancel pending close.
                if self.preview.owner.as_ref() == Some(&owner)
                    && self.preview.pending_close_generation.is_some()
                {
                    self.preview.pending_close_generation = None;
                    effects.push(CardEffect::CancelTimers {
                        kind: TimerKind::PreviewClose,
                    });
                }
                // Also cancel any pending open (already open).
                if self.preview.pending_open_owner.as_ref() == Some(&owner)
                    && self.preview.pending_open_generation.is_some()
                {
                    self.preview.pending_open_generation = None;
                    self.preview.pending_open_owner = None;
                    effects.push(CardEffect::CancelTimers {
                        kind: TimerKind::PreviewOpen,
                    });
                    effects.append(&mut self.update_hold());
                }
            }

            CardRequest::PreviewLeave { owner } => {
                // Treat like source leave — start a close timer.
                if self.preview.owner.as_ref() == Some(&owner) && self.preview.is_open() {
                    let tok = self.next_generation();
                    self.preview.pending_close_generation = Some(tok);
                    effects.push(CardEffect::StartTimer {
                        kind: TimerKind::PreviewClose,
                        generation: tok,
                    });
                }
            }

            CardRequest::PersistentToggle { owner } => {
                if self.persistent.owner.as_ref() == Some(&owner) && self.persistent.is_open() {
                    // Toggle off — close persistent channel.
                    effects.append(&mut self.close_persistent(CardDismissReason::SourceToggle));
                } else {
                    // Toggle on — replace any existing persistent card.
                    effects.append(&mut self.open_persistent(owner));
                }
            }

            // ─── Anchor geometry update ───────────────────────────────
            CardRequest::AnchorUpdate {
                owner,
                bounds,
                display_id,
            } => {
                if self.persistent.owner.as_ref() == Some(&owner) {
                    let changed = self.persistent.anchor_bounds != Some(bounds)
                        || self.persistent.display_id != Some(display_id);
                    self.persistent.anchor_bounds = Some(bounds);
                    self.persistent.display_id = Some(display_id);
                    if changed && self.persistent.is_open() {
                        effects.push(CardEffect::RepositionChannel {
                            channel: CardChannel::Persistent,
                            owner: owner.clone(),
                        });
                    }
                }
                if self.preview.owner.as_ref() == Some(&owner) {
                    let changed = self.preview.anchor_bounds != Some(bounds)
                        || self.preview.display_id != Some(display_id);
                    self.preview.anchor_bounds = Some(bounds);
                    self.preview.display_id = Some(display_id);
                    if changed && self.preview.is_open() {
                        effects.push(CardEffect::RepositionChannel {
                            channel: CardChannel::Preview,
                            owner,
                        });
                    }
                }
            }

            CardRequest::PlacementUpdated {
                owner,
                channel,
                bounds,
                display_id,
            } => {
                let slot = match channel {
                    CardChannel::Persistent => &mut self.persistent,
                    CardChannel::Preview => &mut self.preview,
                };
                if slot.owner.as_ref() == Some(&owner) && slot.is_open() {
                    slot.card_bounds = Some(bounds);
                    slot.display_id = Some(display_id);
                }
            }

            CardRequest::AnchorRemoved { owner } => {
                if self.persistent.owner.as_ref() == Some(&owner) {
                    effects
                        .append(&mut self.close_persistent(CardDismissReason::SourceDisappeared));
                }
                if self.preview.owner.as_ref() == Some(&owner)
                    || self.preview.pending_open_owner.as_ref() == Some(&owner)
                {
                    effects.append(&mut self.close_preview(CardDismissReason::SourceDisappeared));
                }
            }

            // ─── System lifecycle ─────────────────────────────────────
            CardRequest::Dismiss { channel, reason } => match channel {
                CardChannel::Persistent => {
                    effects.append(&mut self.close_persistent(reason));
                }
                CardChannel::Preview => {
                    effects.append(&mut self.close_preview(reason));
                }
            },

            CardRequest::DisplayRemoved { display_id } => {
                if self.persistent.display_id == Some(display_id) {
                    effects.append(&mut self.close_persistent(CardDismissReason::DisplayRemoved));
                }
                if self.preview.display_id == Some(display_id) {
                    effects.append(&mut self.close_preview(CardDismissReason::DisplayRemoved));
                }
            }

            CardRequest::OwnerRemoved { owner } => {
                if self.persistent.owner.as_ref() == Some(&owner) {
                    effects.append(&mut self.close_persistent(CardDismissReason::OwnerRemoved));
                }
                if self.preview.owner.as_ref() == Some(&owner)
                    || self.preview.pending_open_owner.as_ref() == Some(&owner)
                {
                    effects.append(&mut self.close_preview(CardDismissReason::OwnerRemoved));
                }
            }

            CardRequest::OverviewOpened => {
                effects.append(&mut self.close_all(CardDismissReason::OverviewOpened));
            }

            CardRequest::BarClosed => {
                effects.append(&mut self.close_all(CardDismissReason::BarClosed));
            }

            CardRequest::Shutdown => {
                effects.append(&mut self.close_all(CardDismissReason::Shutdown));
            }

            // ─── Timer completions ────────────────────────────────────
            CardRequest::PreviewOpenTimer { generation } => {
                if self.preview.pending_open_generation != Some(generation) {
                    effects.push(CardEffect::Diagnostic(CardDiagnostic {
                        kind: DiagnosticKind::TimerStale,
                        owner: self.preview.owner.clone(),
                        channel: Some(CardChannel::Preview),
                        generation,
                    }));
                    return effects;
                }
                self.preview.pending_open_generation = None;
                let pending_owner = self.preview.pending_open_owner.take();

                // Get the owner queued at SourceEnter — we need a provider
                // registration to find the right owner here. For now the
                // adapter stores the pending owner before dispatching timers.
                // The timer carries no owner; the adapter must have stored the
                // pending preview owner when scheduling. We use what's in the
                // slot (may be `None` for brand-new opens).
                let owner = match pending_owner {
                    Some(o) => o,
                    None => {
                        // No owner recorded yet; timer is orphaned.
                        effects.push(CardEffect::Diagnostic(CardDiagnostic {
                            kind: DiagnosticKind::TimerStale,
                            owner: None,
                            channel: Some(CardChannel::Preview),
                            generation,
                        }));
                        return effects;
                    }
                };

                // Check persistent suppression for same owner.
                let persistent_blocks = self.persistent.owner.as_ref().is_some_and(|o| o == &owner)
                    && self.persistent.is_open();

                if persistent_blocks {
                    return effects;
                }

                if self.preview.is_open() && self.preview.owner.as_ref() != Some(&owner) {
                    let old_display = self.preview.display_id;
                    self.preview.lifecycle = ChannelLifecycle::Closed;
                    self.preview.owner = None;
                    self.preview.display_id = None;
                    self.preview.anchor_bounds = None;
                    self.preview.card_bounds = None;
                    effects.push(CardEffect::CloseChannel {
                        channel: CardChannel::Preview,
                        reason: CardDismissReason::Explicit,
                        display_id: old_display,
                        generation: self.preview.generation,
                    });
                }

                let tok = self.next_generation();
                self.preview.owner = Some(owner.clone());
                self.preview.lifecycle = ChannelLifecycle::Open;
                self.preview.generation = tok;
                effects.push(CardEffect::OpenChannel {
                    channel: CardChannel::Preview,
                    owner: owner.clone(),
                    generation: tok,
                });
                effects.append(&mut self.update_hold());
                effects.push(CardEffect::Diagnostic(CardDiagnostic {
                    kind: DiagnosticKind::ChannelOpened,
                    owner: Some(owner),
                    channel: Some(CardChannel::Preview),
                    generation: tok,
                }));
            }

            CardRequest::PreviewCloseTimer { generation } => {
                if self.preview.pending_close_generation != Some(generation) {
                    effects.push(CardEffect::Diagnostic(CardDiagnostic {
                        kind: DiagnosticKind::TimerStale,
                        owner: self.preview.owner.clone(),
                        channel: Some(CardChannel::Preview),
                        generation,
                    }));
                    return effects;
                }
                self.preview.pending_close_generation = None;
                effects.append(&mut self.close_preview(CardDismissReason::FocusLost));
            }
            CardRequest::CloseAnimationFinished {
                channel,
                generation,
            } => effects.append(&mut self.finish_close(channel, generation)),
        }

        effects
    }

    // ─── Internal helpers ─────────────────────────────────────────

    fn open_persistent(&mut self, owner: CardOwnerId) -> Vec<CardEffect> {
        let mut effects = Vec::new();

        // Close existing persistent without restoring focus between cards
        // (atomic replacement).
        if self.persistent.is_open() {
            let tok = self.persistent.generation;
            let display_id = self.persistent.display_id;
            self.persistent.lifecycle = ChannelLifecycle::Closed;
            self.persistent.owner = None;
            self.persistent.display_id = None;
            self.persistent.anchor_bounds = None;
            self.persistent.card_bounds = None;
            effects.push(CardEffect::CloseChannel {
                channel: CardChannel::Persistent,
                reason: CardDismissReason::SourceToggle,
                display_id,
                generation: tok,
            });
            effects.push(CardEffect::Diagnostic(CardDiagnostic {
                kind: DiagnosticKind::ChannelClosed,
                owner: None,
                channel: Some(CardChannel::Persistent),
                generation: tok,
            }));
        }

        // Capture focus only if we haven't already for this session.
        if !self.focus_captured {
            self.focus_captured = true;
            effects.push(CardEffect::CaptureFocus);
            effects.push(CardEffect::Diagnostic(CardDiagnostic {
                kind: DiagnosticKind::FocusCaptured,
                owner: Some(owner.clone()),
                channel: Some(CardChannel::Persistent),
                generation: self.generation_counter,
            }));
        }

        let tok = self.next_generation();
        self.persistent.owner = Some(owner.clone());
        self.persistent.lifecycle = ChannelLifecycle::Open;
        self.persistent.generation = tok;

        effects.push(CardEffect::OpenChannel {
            channel: CardChannel::Persistent,
            owner: owner.clone(),
            generation: tok,
        });
        effects.append(&mut self.update_hold());
        effects.push(CardEffect::Diagnostic(CardDiagnostic {
            kind: DiagnosticKind::ChannelOpened,
            owner: Some(owner),
            channel: Some(CardChannel::Persistent),
            generation: tok,
        }));
        effects
    }

    fn close_persistent(&mut self, reason: CardDismissReason) -> Vec<CardEffect> {
        let mut effects = Vec::new();
        if !self.persistent.is_open() {
            return effects;
        }

        let tok = self.persistent.generation;
        let display_id = self.persistent.display_id;
        self.persistent.lifecycle = ChannelLifecycle::Closing;

        effects.push(CardEffect::CloseChannel {
            channel: CardChannel::Persistent,
            reason: reason.clone(),
            display_id,
            generation: tok,
        });
        effects
    }

    fn close_preview(&mut self, reason: CardDismissReason) -> Vec<CardEffect> {
        let mut effects = Vec::new();
        if self.preview.is_closed() && self.preview.pending_open_generation.is_none() {
            return effects;
        }

        // Cancel any in-flight timers.
        if self.preview.pending_open_generation.take().is_some() {
            effects.push(CardEffect::CancelTimers {
                kind: TimerKind::PreviewOpen,
            });
        }
        self.preview.pending_open_owner = None;
        if self.preview.pending_close_generation.take().is_some() {
            effects.push(CardEffect::CancelTimers {
                kind: TimerKind::PreviewClose,
            });
        }

        if self.preview.is_open() {
            let tok = self.preview.generation;
            let display_id = self.preview.display_id;
            self.preview.lifecycle = ChannelLifecycle::Closing;

            effects.push(CardEffect::CloseChannel {
                channel: CardChannel::Preview,
                reason,
                display_id,
                generation: tok,
            });
        }
        effects.append(&mut self.update_hold());
        effects
    }

    fn close_all(&mut self, reason: CardDismissReason) -> Vec<CardEffect> {
        let mut effects = Vec::new();
        effects.append(&mut self.close_persistent(reason.clone()));
        effects.append(&mut self.close_preview(reason));
        effects
    }

    fn finish_close(&mut self, channel: CardChannel, generation: u64) -> Vec<CardEffect> {
        let slot = match channel {
            CardChannel::Persistent => &mut self.persistent,
            CardChannel::Preview => &mut self.preview,
        };
        if slot.lifecycle != ChannelLifecycle::Closing || slot.generation != generation {
            return Vec::new();
        }

        let owner = slot.owner.take();
        slot.lifecycle = ChannelLifecycle::Closed;
        slot.generation = 0;
        slot.display_id = None;
        slot.anchor_bounds = None;
        slot.card_bounds = None;

        let mut effects = Vec::new();
        if channel == CardChannel::Persistent && self.focus_captured {
            self.focus_captured = false;
            effects.push(CardEffect::RestoreFocus);
            effects.push(CardEffect::Diagnostic(CardDiagnostic {
                kind: DiagnosticKind::FocusRestored,
                owner: owner.clone(),
                channel: Some(channel),
                generation,
            }));
        }
        effects.append(&mut self.update_hold());
        effects.push(CardEffect::Diagnostic(CardDiagnostic {
            kind: DiagnosticKind::ChannelClosed,
            owner,
            channel: Some(channel),
            generation,
        }));
        effects
    }

    fn update_hold(&mut self) -> Vec<CardEffect> {
        let hold = self.persistent.holds_visibility()
            || self.preview.holds_visibility()
            || self.preview.pending_open_generation.is_some();

        if hold != self.bar_visibility_hold {
            self.bar_visibility_hold = hold;
            return vec![CardEffect::UpdateBarHold { hold }];
        }
        vec![]
    }

    // ─── Public queries ───────────────────────────────────────────

    /// Current source-widget state for a given owner.
    pub fn source_state(&self, owner: &CardOwnerId) -> CardSourceState {
        if self.persistent.owner.as_ref() == Some(owner) && self.persistent.is_open() {
            return CardSourceState::PersistentOpen;
        }
        if self.preview.owner.as_ref() == Some(owner) && self.preview.is_open() {
            return CardSourceState::PreviewOpen;
        }
        if self.preview.pending_open_generation.is_some() {
            // Only report HoverPending for the current prospective owner.
            // We track the pending owner in the slot's `owner` field once set.
            // If the slot owner matches, report pending.
            if self.preview.pending_open_owner.as_ref() == Some(owner) {
                return CardSourceState::HoverPending;
            }
        }
        CardSourceState::Idle
    }

    /// Whether the card system currently requires bar visibility to be held.
    pub fn holds_bar_visibility(&self) -> bool {
        self.bar_visibility_hold
    }

    /// Test helper for directly priming a pending hover owner.
    #[cfg(test)]
    pub fn set_pending_preview_owner(&mut self, owner: CardOwnerId) {
        self.preview.pending_open_owner = Some(owner);
    }
}

// ────────────────────────────────────────────────────────────────
// Tests
// ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{Bounds, DisplayId, Pixels, Point, Size, px};

    fn owner(id: &str) -> CardOwnerId {
        CardOwnerId::new(id)
    }

    fn bounds() -> Bounds<Pixels> {
        Bounds {
            origin: Point {
                x: px(100.0),
                y: px(50.0),
            },
            size: Size {
                width: px(48.0),
                height: px(40.0),
            },
        }
    }

    fn has_effect(effects: &[CardEffect], predicate: impl Fn(&CardEffect) -> bool) -> bool {
        effects.iter().any(predicate)
    }

    fn open_timer_gen(effects: &[CardEffect]) -> Option<u64> {
        effects.iter().find_map(|e| {
            if let CardEffect::StartTimer {
                kind: TimerKind::PreviewOpen,
                generation,
            } = e
            {
                Some(*generation)
            } else {
                None
            }
        })
    }

    fn close_generation(effects: &[CardEffect]) -> u64 {
        effects
            .iter()
            .find_map(|effect| match effect {
                CardEffect::CloseChannel { generation, .. } => Some(*generation),
                _ => None,
            })
            .expect("close animation generation")
    }

    fn close_timer_gen(effects: &[CardEffect]) -> Option<u64> {
        effects.iter().find_map(|e| {
            if let CardEffect::StartTimer {
                kind: TimerKind::PreviewClose,
                generation,
            } = e
            {
                Some(*generation)
            } else {
                None
            }
        })
    }

    // ── Hover open timing ─────────────────────────────────────────

    #[test]
    fn source_enter_starts_preview_open_timer() {
        let mut state = CardState::default();
        state.set_pending_preview_owner(owner("battery"));
        let effects = state.reduce(CardRequest::SourceEnter {
            owner: owner("battery"),
        });
        assert!(
            open_timer_gen(&effects).is_some(),
            "SourceEnter should start a preview open timer"
        );
    }

    #[test]
    fn preview_open_timer_with_correct_generation_opens_channel() {
        let mut state = CardState::default();
        state.set_pending_preview_owner(owner("battery"));
        let effects = state.reduce(CardRequest::SourceEnter {
            owner: owner("battery"),
        });
        let tok = open_timer_gen(&effects).unwrap();

        let open_effects = state.reduce(CardRequest::PreviewOpenTimer { generation: tok });
        assert!(
            has_effect(&open_effects, |e| matches!(
                e,
                CardEffect::OpenChannel {
                    channel: CardChannel::Preview,
                    ..
                }
            )),
            "Correct-generation timer should open Preview channel"
        );
    }

    #[test]
    fn preview_open_timer_stale_generation_is_noop() {
        let mut state = CardState::default();
        state.set_pending_preview_owner(owner("battery"));
        let effects = state.reduce(CardRequest::SourceEnter {
            owner: owner("battery"),
        });
        let tok = open_timer_gen(&effects).unwrap();

        // Fire with wrong generation
        let noop_effects = state.reduce(CardRequest::PreviewOpenTimer {
            generation: tok + 99,
        });
        assert!(
            !has_effect(&noop_effects, |e| matches!(
                e,
                CardEffect::OpenChannel { .. }
            )),
            "Stale generation timer should not open channel"
        );
        assert!(has_effect(&noop_effects, |e| matches!(
            e,
            CardEffect::Diagnostic(CardDiagnostic {
                kind: DiagnosticKind::TimerStale,
                ..
            })
        )));
    }

    // ── Hover close timing ────────────────────────────────────────

    #[test]
    fn source_leave_after_open_starts_preview_close_timer() {
        let mut state = CardState::default();
        state.set_pending_preview_owner(owner("battery"));
        let effects = state.reduce(CardRequest::SourceEnter {
            owner: owner("battery"),
        });
        let tok = open_timer_gen(&effects).unwrap();
        state.reduce(CardRequest::PreviewOpenTimer { generation: tok });

        let leave_effects = state.reduce(CardRequest::SourceLeave {
            owner: owner("battery"),
        });
        assert!(
            close_timer_gen(&leave_effects).is_some(),
            "SourceLeave after open should start a preview close timer"
        );
    }

    #[test]
    fn preview_close_timer_closes_channel() {
        let mut state = CardState::default();
        state.set_pending_preview_owner(owner("battery"));
        let effects = state.reduce(CardRequest::SourceEnter {
            owner: owner("battery"),
        });
        let tok = open_timer_gen(&effects).unwrap();
        state.reduce(CardRequest::PreviewOpenTimer { generation: tok });

        let leave_effects = state.reduce(CardRequest::SourceLeave {
            owner: owner("battery"),
        });
        let close_gen = close_timer_gen(&leave_effects).unwrap();

        let close_effects = state.reduce(CardRequest::PreviewCloseTimer {
            generation: close_gen,
        });
        assert!(
            has_effect(&close_effects, |e| matches!(
                e,
                CardEffect::CloseChannel {
                    channel: CardChannel::Preview,
                    ..
                }
            )),
            "Close timer should close Preview channel"
        );
    }

    #[test]
    fn source_leave_before_timer_fires_cancels_open() {
        let mut state = CardState::default();
        state.set_pending_preview_owner(owner("battery"));
        let effects = state.reduce(CardRequest::SourceEnter {
            owner: owner("battery"),
        });
        let tok = open_timer_gen(&effects).unwrap();

        let leave_effects = state.reduce(CardRequest::SourceLeave {
            owner: owner("battery"),
        });
        assert!(
            has_effect(&leave_effects, |e| matches!(
                e,
                CardEffect::CancelTimers {
                    kind: TimerKind::PreviewOpen
                }
            )),
            "SourceLeave before timer fires should cancel open timer"
        );

        // Stale timer should be no-op
        let late_effects = state.reduce(CardRequest::PreviewOpenTimer { generation: tok });
        assert!(!has_effect(&late_effects, |e| matches!(
            e,
            CardEffect::OpenChannel { .. }
        )));
    }

    // ── Pointer bridge (source → preview) ────────────────────────

    #[test]
    fn preview_enter_cancels_pending_close() {
        let mut state = CardState::default();
        state.set_pending_preview_owner(owner("battery"));
        let effects = state.reduce(CardRequest::SourceEnter {
            owner: owner("battery"),
        });
        let tok = open_timer_gen(&effects).unwrap();
        state.reduce(CardRequest::PreviewOpenTimer { generation: tok });

        // Leave source
        state.reduce(CardRequest::SourceLeave {
            owner: owner("battery"),
        });

        // Enter preview card
        let enter_effects = state.reduce(CardRequest::PreviewEnter {
            owner: owner("battery"),
        });
        assert!(
            has_effect(&enter_effects, |e| matches!(
                e,
                CardEffect::CancelTimers {
                    kind: TimerKind::PreviewClose
                }
            )),
            "PreviewEnter should cancel pending close timer"
        );
    }

    // ── Persistent toggle ─────────────────────────────────────────

    #[test]
    fn persistent_toggle_opens_channel() {
        let mut state = CardState::default();
        let effects = state.reduce(CardRequest::PersistentToggle {
            owner: owner("workspaces"),
        });
        assert!(has_effect(&effects, |e| matches!(
            e,
            CardEffect::OpenChannel {
                channel: CardChannel::Persistent,
                ..
            }
        )));
    }

    #[test]
    fn persistent_toggle_closes_open_channel() {
        let mut state = CardState::default();
        state.reduce(CardRequest::PersistentToggle {
            owner: owner("workspaces"),
        });
        let effects = state.reduce(CardRequest::PersistentToggle {
            owner: owner("workspaces"),
        });
        assert!(has_effect(&effects, |e| matches!(
            e,
            CardEffect::CloseChannel {
                channel: CardChannel::Persistent,
                ..
            }
        )));
    }

    #[test]
    fn persistent_replacement_skips_focus_restore_between_cards() {
        let mut state = CardState::default();
        // Open card A
        state.reduce(CardRequest::PersistentToggle {
            owner: owner("battery"),
        });
        // Open card B (atomic replacement)
        let effects = state.reduce(CardRequest::PersistentToggle {
            owner: owner("workspaces"),
        });
        // Should NOT restore focus on replacement
        assert!(
            !has_effect(&effects, |e| matches!(e, CardEffect::RestoreFocus)),
            "Replacement should not restore focus between cards"
        );
        // Should open the new card
        assert!(has_effect(&effects, |e| matches!(
            e,
            CardEffect::OpenChannel {
                channel: CardChannel::Persistent,
                owner: target_owner,
                ..
            } if target_owner == &owner("workspaces")
        )));
    }

    // ── Preview suppression ───────────────────────────────────────

    #[test]
    fn same_owner_persistent_suppresses_preview() {
        let mut state = CardState::default();
        // Open persistent for "battery"
        state.reduce(CardRequest::PersistentToggle {
            owner: owner("battery"),
        });

        // SourceEnter from same owner — should NOT start preview timer.
        state.set_pending_preview_owner(owner("battery"));
        let effects = state.reduce(CardRequest::SourceEnter {
            owner: owner("battery"),
        });
        assert!(
            !has_effect(&effects, |e| matches!(
                e,
                CardEffect::StartTimer {
                    kind: TimerKind::PreviewOpen,
                    ..
                }
            )),
            "Same-owner persistent should suppress preview"
        );
    }

    #[test]
    fn different_owner_persistent_allows_preview() {
        let mut state = CardState::default();
        // Open persistent for "battery"
        state.reduce(CardRequest::PersistentToggle {
            owner: owner("battery"),
        });

        // SourceEnter from different owner — should start preview timer.
        state.set_pending_preview_owner(owner("workspaces"));
        let effects = state.reduce(CardRequest::SourceEnter {
            owner: owner("workspaces"),
        });
        assert!(
            has_effect(&effects, |e| matches!(
                e,
                CardEffect::StartTimer {
                    kind: TimerKind::PreviewOpen,
                    ..
                }
            )),
            "Different-owner persistent should allow preview"
        );
    }

    // ── Focus capture / restore ───────────────────────────────────

    #[test]
    fn first_persistent_open_emits_capture_focus() {
        let mut state = CardState::default();
        let effects = state.reduce(CardRequest::PersistentToggle {
            owner: owner("battery"),
        });
        assert!(has_effect(&effects, |e| matches!(
            e,
            CardEffect::CaptureFocus
        )));
    }

    #[test]
    fn second_persistent_open_does_not_re_capture_focus() {
        let mut state = CardState::default();
        state.reduce(CardRequest::PersistentToggle {
            owner: owner("battery"),
        });
        // Replace with new owner
        let effects = state.reduce(CardRequest::PersistentToggle {
            owner: owner("workspaces"),
        });
        assert!(
            !has_effect(&effects, |e| matches!(e, CardEffect::CaptureFocus)),
            "Second open should not re-capture focus"
        );
    }

    #[test]
    fn persistent_close_emits_restore_focus() {
        let mut state = CardState::default();
        state.reduce(CardRequest::PersistentToggle {
            owner: owner("battery"),
        });
        let effects = state.reduce(CardRequest::PersistentToggle {
            owner: owner("battery"),
        });
        assert!(!has_effect(&effects, |e| matches!(
            e,
            CardEffect::RestoreFocus
        )));
        let completion = state.reduce(CardRequest::CloseAnimationFinished {
            channel: CardChannel::Persistent,
            generation: close_generation(&effects),
        });
        assert!(has_effect(&completion, |e| matches!(
            e,
            CardEffect::RestoreFocus
        )));
    }

    // ── Dismissal paths ───────────────────────────────────────────

    #[test]
    fn overview_opened_closes_all_channels() {
        let mut state = CardState::default();
        state.reduce(CardRequest::PersistentToggle {
            owner: owner("battery"),
        });
        state.set_pending_preview_owner(owner("workspaces"));
        state.reduce(CardRequest::SourceEnter {
            owner: owner("workspaces"),
        });
        let open_gen = state.preview.pending_open_generation.unwrap();
        state.reduce(CardRequest::PreviewOpenTimer {
            generation: open_gen,
        });

        let effects = state.reduce(CardRequest::OverviewOpened);
        let close_persistent = effects.iter().any(|e| {
            matches!(
                e,
                CardEffect::CloseChannel {
                    channel: CardChannel::Persistent,
                    reason: CardDismissReason::OverviewOpened,
                    ..
                }
            )
        });
        let close_preview = effects.iter().any(|e| {
            matches!(
                e,
                CardEffect::CloseChannel {
                    channel: CardChannel::Preview,
                    reason: CardDismissReason::OverviewOpened,
                    ..
                }
            )
        });
        assert!(close_persistent, "OverviewOpened should close persistent");
        assert!(close_preview, "OverviewOpened should close preview");
    }

    #[test]
    fn bar_closed_closes_all_channels() {
        let mut state = CardState::default();
        state.reduce(CardRequest::PersistentToggle {
            owner: owner("battery"),
        });
        let effects = state.reduce(CardRequest::BarClosed);
        assert!(has_effect(&effects, |e| matches!(
            e,
            CardEffect::CloseChannel {
                channel: CardChannel::Persistent,
                reason: CardDismissReason::BarClosed,
                ..
            }
        )));
    }

    #[test]
    fn shutdown_closes_all_channels() {
        let mut state = CardState::default();
        state.reduce(CardRequest::PersistentToggle {
            owner: owner("battery"),
        });
        let effects = state.reduce(CardRequest::Shutdown);
        assert!(has_effect(&effects, |e| matches!(
            e,
            CardEffect::CloseChannel {
                channel: CardChannel::Persistent,
                reason: CardDismissReason::Shutdown,
                ..
            }
        )));
    }

    #[test]
    fn display_removed_closes_only_matching_display() {
        let mut state = CardState::default();
        state.reduce(CardRequest::PersistentToggle {
            owner: owner("battery"),
        });
        state.reduce(CardRequest::AnchorUpdate {
            owner: owner("battery"),
            bounds: bounds(),
            display_id: DisplayId::new(1),
        });

        let effects = state.reduce(CardRequest::DisplayRemoved {
            display_id: DisplayId::new(1),
        });
        assert!(has_effect(&effects, |e| matches!(
            e,
            CardEffect::CloseChannel {
                channel: CardChannel::Persistent,
                reason: CardDismissReason::DisplayRemoved,
                ..
            }
        )));
    }

    #[test]
    fn display_removed_different_display_noop_for_persistent() {
        let mut state = CardState::default();
        state.reduce(CardRequest::PersistentToggle {
            owner: owner("battery"),
        });
        state.reduce(CardRequest::AnchorUpdate {
            owner: owner("battery"),
            bounds: bounds(),
            display_id: DisplayId::new(1),
        });

        let effects = state.reduce(CardRequest::DisplayRemoved {
            display_id: DisplayId::new(2),
        });
        assert!(
            !has_effect(&effects, |e| matches!(
                e,
                CardEffect::CloseChannel {
                    channel: CardChannel::Persistent,
                    ..
                }
            )),
            "DisplayRemoved for a different display should not close persistent"
        );
    }

    // ── Source state projections ──────────────────────────────────

    #[test]
    fn source_state_idle_when_nothing_open() {
        let state = CardState::default();
        assert_eq!(state.source_state(&owner("battery")), CardSourceState::Idle);
    }

    #[test]
    fn source_state_persistent_open_when_persistent_active() {
        let mut state = CardState::default();
        state.reduce(CardRequest::PersistentToggle {
            owner: owner("battery"),
        });
        assert_eq!(
            state.source_state(&owner("battery")),
            CardSourceState::PersistentOpen
        );
    }

    #[test]
    fn source_state_preview_open_when_preview_active() {
        let mut state = CardState::default();
        state.set_pending_preview_owner(owner("battery"));
        let effects = state.reduce(CardRequest::SourceEnter {
            owner: owner("battery"),
        });
        let tok = open_timer_gen(&effects).unwrap();
        state.reduce(CardRequest::PreviewOpenTimer { generation: tok });
        assert_eq!(
            state.source_state(&owner("battery")),
            CardSourceState::PreviewOpen
        );
    }

    #[test]
    fn source_state_hover_pending_when_timer_queued() {
        let mut state = CardState::default();
        state.set_pending_preview_owner(owner("battery"));
        state.reduce(CardRequest::SourceEnter {
            owner: owner("battery"),
        });
        assert_eq!(
            state.source_state(&owner("battery")),
            CardSourceState::HoverPending
        );
    }

    // ── Visibility hold ───────────────────────────────────────────

    #[test]
    fn hold_set_when_persistent_open() {
        let mut state = CardState::default();
        let effects = state.reduce(CardRequest::PersistentToggle {
            owner: owner("battery"),
        });
        assert!(
            has_effect(&effects, |e| matches!(
                e,
                CardEffect::UpdateBarHold { hold: true }
            )),
            "Opening persistent should set bar hold"
        );
        assert!(state.holds_bar_visibility());
    }

    #[test]
    fn hold_is_retained_until_close_animation_finishes() {
        let mut state = CardState::default();
        state.reduce(CardRequest::PersistentToggle {
            owner: owner("battery"),
        });
        let effects = state.reduce(CardRequest::PersistentToggle {
            owner: owner("battery"),
        });
        let generation = effects
            .iter()
            .find_map(|effect| match effect {
                CardEffect::CloseChannel { generation, .. } => Some(*generation),
                _ => None,
            })
            .expect("close animation generation");
        assert!(state.holds_bar_visibility());

        let completion = state.reduce(CardRequest::CloseAnimationFinished {
            channel: CardChannel::Persistent,
            generation,
        });
        assert!(has_effect(&completion, |effect| matches!(
            effect,
            CardEffect::UpdateBarHold { hold: false }
        )));
        assert!(!state.holds_bar_visibility());
    }

    #[test]
    fn hold_set_when_preview_timer_pending() {
        let mut state = CardState::default();
        state.set_pending_preview_owner(owner("battery"));
        let effects = state.reduce(CardRequest::SourceEnter {
            owner: owner("battery"),
        });
        // Timer pending — hold should be set
        assert!(
            has_effect(&effects, |e| matches!(
                e,
                CardEffect::UpdateBarHold { hold: true }
            )),
            "Pending preview timer should set bar hold"
        );
    }

    #[test]
    fn source_enter_records_owner_without_test_priming() {
        let mut state = CardState::default();
        let effects = state.reduce(CardRequest::SourceEnter {
            owner: owner("workspaces"),
        });
        let generation = open_timer_gen(&effects).expect("hover intent timer");
        assert_eq!(
            state.preview.pending_open_owner.as_ref(),
            Some(&owner("workspaces"))
        );
        let effects = state.reduce(CardRequest::PreviewOpenTimer { generation });
        assert!(has_effect(&effects, |effect| matches!(
            effect,
            CardEffect::OpenChannel {
                channel: CardChannel::Preview,
                owner,
                ..
            } if owner == &CardOwnerId::new("workspaces")
        )));
    }

    #[test]
    fn close_effect_preserves_surface_display() {
        let mut state = CardState::default();
        let battery = owner("battery");
        state.reduce(CardRequest::PersistentToggle {
            owner: battery.clone(),
        });
        state.reduce(CardRequest::AnchorUpdate {
            owner: battery.clone(),
            bounds: bounds(),
            display_id: DisplayId::new(7),
        });
        let effects = state.reduce(CardRequest::PersistentToggle { owner: battery });
        assert!(has_effect(&effects, |effect| matches!(
            effect,
            CardEffect::CloseChannel {
                channel: CardChannel::Persistent,
                display_id: Some(id),
                ..
            } if *id == DisplayId::new(7)
        )));
    }

    #[test]
    fn anchor_change_requests_live_reposition() {
        let mut state = CardState::default();
        let battery = owner("battery");
        state.reduce(CardRequest::PersistentToggle {
            owner: battery.clone(),
        });
        let effects = state.reduce(CardRequest::AnchorUpdate {
            owner: battery.clone(),
            bounds: bounds(),
            display_id: DisplayId::new(1),
        });
        assert!(has_effect(&effects, |effect| matches!(
            effect,
            CardEffect::RepositionChannel {
                channel: CardChannel::Persistent,
                owner,
            } if owner == &battery
        )));
    }

    #[test]
    fn removing_an_owner_does_not_close_another_owners_channels() {
        let mut state = CardState::default();
        let battery = owner("battery");
        state.reduce(CardRequest::PersistentToggle {
            owner: battery.clone(),
        });

        let effects = state.reduce(CardRequest::OwnerRemoved {
            owner: owner("workspaces"),
        });

        assert!(effects.is_empty());
        assert_eq!(state.persistent.owner.as_ref(), Some(&battery));
        assert!(state.persistent.is_open());
    }

    #[test]
    fn removing_pending_preview_owner_cancels_the_hold() {
        let mut state = CardState::default();
        let workspaces = owner("workspaces");
        state.reduce(CardRequest::SourceEnter {
            owner: workspaces.clone(),
        });

        let effects = state.reduce(CardRequest::OwnerRemoved { owner: workspaces });

        assert!(has_effect(&effects, |effect| matches!(
            effect,
            CardEffect::CancelTimers {
                kind: TimerKind::PreviewOpen
            }
        )));
        assert!(!state.holds_bar_visibility());
    }

    #[test]
    fn disappearing_anchor_closes_only_its_owner() {
        let mut state = CardState::default();
        let battery = owner("battery");
        state.reduce(CardRequest::PersistentToggle {
            owner: battery.clone(),
        });

        let effects = state.reduce(CardRequest::AnchorRemoved {
            owner: battery.clone(),
        });

        assert!(has_effect(&effects, |effect| matches!(
            effect,
            CardEffect::CloseChannel {
                channel: CardChannel::Persistent,
                reason: CardDismissReason::SourceDisappeared,
                ..
            }
        )));
        assert_eq!(state.source_state(&battery), CardSourceState::Idle);
        assert!(state.holds_bar_visibility());
        state.reduce(CardRequest::CloseAnimationFinished {
            channel: CardChannel::Persistent,
            generation: close_generation(&effects),
        });
        assert!(!state.holds_bar_visibility());
    }
}
