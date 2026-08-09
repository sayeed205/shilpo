//! `CardProvider` trait — the contract built-in widgets implement to participate
//! in the two-channel card coordinator.
//!
//! Implementations are internal to `shilpo-shell`.  Extensions receive a
//! separately designed, capability-checked declarative API rather than
//! implementing this trait directly.

use gpui::{AnyElement, App, Bounds, DisplayId, Pixels, Window};

use super::model::{CardCapabilities, CardChannel, CardOwnerId, CardSizeTier};

/// Type alias for lazy card content rendering closures.
pub(crate) type CardContentRenderFn = Box<dyn Fn(&mut Window, &mut App) -> AnyElement + Send>;

/// Interface a built-in card provider must implement.
///
/// Providers supply stable identity, independently declared capabilities,
/// preferred size tier, live anchor geometry, and lazy content rendering.
/// They never receive surface handles or implement placement or focus policy —
/// the coordinator owns all of that.
pub(crate) trait CardProvider: 'static + Send + Sync {
    /// Stable identity key for this provider.
    fn owner_id(&self) -> CardOwnerId;

    /// Which interaction channels this provider participates in.
    fn capabilities(&self) -> CardCapabilities;

    /// Preferred maximum card size tier.
    fn size_tier(&self) -> CardSizeTier;

    /// Current global anchor bounds of the source widget in logical coordinates,
    /// and the display it lives on.  Returns `None` when the widget is not
    /// currently mounted (e.g. not in any bar section).
    fn anchor_bounds(&self, cx: &App) -> Option<(Bounds<Pixels>, DisplayId)>;

    /// Lazily render card content for `channel`.
    ///
    /// Called only when the coordinator has decided to open the channel for
    /// this provider.  The returned element is placed inside the M3 card shell
    /// rendered by `CardBandView`.
    fn render_content(&self, channel: CardChannel, window: &mut Window, cx: &mut App)
    -> AnyElement;
}
