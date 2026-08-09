//! `CardProvider` trait — the contract built-in widgets implement to participate
//! in the two-channel card coordinator.
//!
//! Implementations are internal to `shilpo-shell`.  Extensions receive a
//! separately designed, capability-checked declarative API rather than
//! implementing this trait directly.

use gpui::{AnyElement, App, Pixels, Size, Window};

use super::model::{CardCapabilities, CardChannel, CardOwnerId, CardSourceId};

/// Type alias for lazy card content rendering closures.
pub(crate) type CardContentRenderFn = Box<dyn Fn(&mut Window, &mut App) -> AnyElement + Send>;

/// Interface a built-in card provider must implement.
///
/// Providers supply stable identity, independently declared capabilities,
/// preferred dimensions per channel/source, and lazy content rendering.
/// They never receive surface handles or implement placement or focus policy —
/// the coordinator owns all of that.
pub(crate) trait CardProvider: 'static + Send + Sync {
    /// Stable identity key for this provider.
    fn owner_id(&self) -> CardOwnerId;

    /// Which interaction channels this provider participates in.
    fn capabilities(&self) -> CardCapabilities;

    /// Preferred card size for the given channel and source.
    fn preferred_size(&self, channel: CardChannel, source: &CardSourceId, cx: &App)
    -> Size<Pixels>;

    /// Whether `source` still identifies content owned by this provider.
    /// Degraded providers may retain cached sources until authoritative data returns.
    fn source_available(&self, _source: &CardSourceId, _cx: &App) -> bool {
        true
    }

    /// Lazily render card content for `channel` and `source`.
    ///
    /// Called only when the coordinator has decided to open the channel for
    /// this source. The returned element is placed inside the M3 card shell
    /// rendered by `CardBandView`.
    fn render_content(
        &self,
        channel: CardChannel,
        source: &CardSourceId,
        window: &mut Window,
        cx: &mut App,
    ) -> AnyElement;
}
