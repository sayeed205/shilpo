//! Pure deterministic placement engine for bar-card geometry.
//!
//! This module has no GPUI runtime dependency — all inputs and outputs are
//! geometry types that can be constructed in plain `#[test]` cases without a
//! display server.

use gpui::{Bounds, Pixels, Point, Size, px};
use shilpo_config::BarPosition;

use super::model::CardSizeTier;

// ────────────────────────────────────────────────────────────────
// Constants
// ────────────────────────────────────────────────────────────────

/// Minimum inset from monitor edges in logical pixels.
pub const SAFE_INSET: f32 = 8.0;
/// Gap between the card and the source widget, and between cards on collision.
pub const SOURCE_GAP: f32 = 8.0;
/// Minimum usable width before a preview is suppressed.
pub const MIN_USABLE_WIDTH: f32 = 160.0;
/// Minimum usable height before a preview is suppressed.
pub const MIN_USABLE_HEIGHT: f32 = 96.0;

/// Maximum inward depth of any card (Expanded height/width + gaps).
/// Used to size the edge-band surface.
const MAX_CARD_DEPTH_TALL: f32 = CardSizeTier::Expanded.max_height() + SOURCE_GAP + SAFE_INSET;
const MAX_CARD_DEPTH_WIDE: f32 = CardSizeTier::Expanded.max_width() + SOURCE_GAP + SAFE_INSET;

// ────────────────────────────────────────────────────────────────
// Public types
// ────────────────────────────────────────────────────────────────

/// All inputs the placement engine needs for a single card.
#[derive(Clone, Debug)]
pub struct PlacementInput {
    /// Full monitor bounds in global logical coordinates.
    pub monitor_bounds: Bounds<Pixels>,
    /// Which edge the bar sits on.
    pub bar_edge: BarPosition,
    /// Bar thickness (exclusive zone) in logical pixels.
    pub bar_thickness: Pixels,
    /// Source widget bounds in global logical coordinates.
    pub source_bounds: Bounds<Pixels>,
    /// Requested card size (tier maximum or smaller).
    pub requested_size: Size<Pixels>,
    /// Bounds of an already-placed card on the same monitor that the new card
    /// must not overlap (i.e. place preview around a persistent card).
    pub collision_bounds: Option<Bounds<Pixels>>,
}

/// Result of one placement computation.
#[derive(Clone, Debug, PartialEq)]
pub enum PlacementResult {
    Placed {
        /// Card position and size in global logical coordinates.
        card_bounds: Bounds<Pixels>,
        /// Edge-band surface geometry for this placement.
        band_geometry: BandGeometry,
    },
    Suppressed {
        /// Short diagnostic string — never contains sensitive content.
        reason: &'static str,
    },
}

/// Geometry of the edge-band layer-shell surface (per monitor, per channel).
#[derive(Clone, Debug, PartialEq)]
pub struct BandGeometry {
    /// Band position and size in global logical coordinates.
    pub bounds: Bounds<Pixels>,
    pub bar_edge: BarPosition,
    /// Thickness of the bar on this monitor (used for positioning the band).
    pub bar_thickness: Pixels,
}

// ────────────────────────────────────────────────────────────────
// Public API
// ────────────────────────────────────────────────────────────────

/// Compute card placement deterministically.
///
/// # Algorithm
///
/// 1. Open inward from the bar edge, centred on the source widget.
/// 2. Clamp to monitor safe area (`SAFE_INSET` from each edge).
/// 3. If `collision_bounds` is `Some`, try placing on the larger along-axis
///    side, then the smaller side, then one `SOURCE_GAP` farther inward.
/// 4. Clamp/shrink to available space.
/// 5. If the remaining space < `MIN_USABLE_WIDTH × MIN_USABLE_HEIGHT`,
///    return `PlacementResult::Suppressed`.
pub fn compute_placement(input: &PlacementInput) -> PlacementResult {
    let safe = px(SAFE_INSET);
    let gap = px(SOURCE_GAP);

    // ── Step 1: initial unconstrained placement ───────────────────
    let card = initial_placement(input);

    // ── Step 2: clamp to monitor safe area ────────────────────────
    let card = clamp_to_monitor(card, input.monitor_bounds, safe);

    // ── Step 3: collision avoidance ───────────────────────────────
    let card = if let Some(collision) = input.collision_bounds {
        resolve_collision(card, collision, input, safe, gap)
    } else {
        card
    };

    // ── Step 4: shrink to available space ─────────────────────────
    let card = shrink_to_monitor(card, input.monitor_bounds, safe);

    // ── Step 5: minimum size check ────────────────────────────────
    if card.size.width < px(MIN_USABLE_WIDTH) || card.size.height < px(MIN_USABLE_HEIGHT) {
        return PlacementResult::Suppressed {
            reason: "card shrunk below minimum usable size",
        };
    }

    let band = compute_band_geometry(input);
    PlacementResult::Placed {
        card_bounds: card,
        band_geometry: band,
    }
}

/// Compute the edge-band surface bounds for a given monitor + bar configuration.
///
/// The band is a transparent overlay surface that spans the full monitor width
/// (horizontal bars) or full monitor height (vertical bars).  Its depth is the
/// maximum inward extent any card could reach.
pub fn compute_band_geometry(input: &PlacementInput) -> BandGeometry {
    let m = input.monitor_bounds;
    let thickness = input.bar_thickness;

    let bounds = match input.bar_edge {
        BarPosition::Top => Bounds {
            origin: Point {
                x: m.origin.x,
                y: m.origin.y + thickness,
            },
            size: Size {
                width: m.size.width,
                height: px(MAX_CARD_DEPTH_TALL),
            },
        },
        BarPosition::Bottom => Bounds {
            origin: Point {
                x: m.origin.x,
                y: m.origin.y + m.size.height - thickness - px(MAX_CARD_DEPTH_TALL),
            },
            size: Size {
                width: m.size.width,
                height: px(MAX_CARD_DEPTH_TALL),
            },
        },
        BarPosition::Left => Bounds {
            origin: Point {
                x: m.origin.x + thickness,
                y: m.origin.y,
            },
            size: Size {
                width: px(MAX_CARD_DEPTH_WIDE),
                height: m.size.height,
            },
        },
        BarPosition::Right => Bounds {
            origin: Point {
                x: m.origin.x + m.size.width - thickness - px(MAX_CARD_DEPTH_WIDE),
                y: m.origin.y,
            },
            size: Size {
                width: px(MAX_CARD_DEPTH_WIDE),
                height: m.size.height,
            },
        },
    };

    BandGeometry {
        bounds,
        bar_edge: input.bar_edge,
        bar_thickness: thickness,
    }
}

/// Full monitor work-area surface used by persistent cards so a click anywhere
/// outside the card can be observed and dismiss it.
pub fn compute_persistent_band_geometry(input: &PlacementInput) -> BandGeometry {
    let m = input.monitor_bounds;
    let thickness = input.bar_thickness;
    let bounds = match input.bar_edge {
        BarPosition::Top => Bounds {
            origin: Point {
                x: m.origin.x,
                y: m.origin.y + thickness,
            },
            size: Size {
                width: m.size.width,
                height: m.size.height - thickness,
            },
        },
        BarPosition::Bottom => Bounds {
            origin: m.origin,
            size: Size {
                width: m.size.width,
                height: m.size.height - thickness,
            },
        },
        BarPosition::Left => Bounds {
            origin: Point {
                x: m.origin.x + thickness,
                y: m.origin.y,
            },
            size: Size {
                width: m.size.width - thickness,
                height: m.size.height,
            },
        },
        BarPosition::Right => Bounds {
            origin: m.origin,
            size: Size {
                width: m.size.width - thickness,
                height: m.size.height,
            },
        },
    };

    BandGeometry {
        bounds,
        bar_edge: input.bar_edge,
        bar_thickness: thickness,
    }
}

// ────────────────────────────────────────────────────────────────
// Internal helpers
// ────────────────────────────────────────────────────────────────

/// Compute initial placement: inward from bar edge, centred on source.
fn initial_placement(input: &PlacementInput) -> Bounds<Pixels> {
    let s = input.source_bounds;
    let r = input.requested_size;
    let gap = px(SOURCE_GAP);
    match input.bar_edge {
        BarPosition::Top => {
            let bar_inner_edge = input.monitor_bounds.origin.y + input.bar_thickness;
            let y = bar_inner_edge + gap;
            let x = s.origin.x + s.size.width / 2.0 - r.width / 2.0;
            Bounds {
                origin: Point { x, y },
                size: r,
            }
        }
        BarPosition::Bottom => {
            let bar_inner_edge = input.monitor_bounds.origin.y + input.monitor_bounds.size.height
                - input.bar_thickness;
            let y = bar_inner_edge - r.height - gap;
            let x = s.origin.x + s.size.width / 2.0 - r.width / 2.0;
            Bounds {
                origin: Point { x, y },
                size: r,
            }
        }
        BarPosition::Left => {
            let bar_inner_edge = input.monitor_bounds.origin.x + input.bar_thickness;
            let x = bar_inner_edge + gap;
            let y = s.origin.y + s.size.height / 2.0 - r.height / 2.0;
            Bounds {
                origin: Point { x, y },
                size: r,
            }
        }
        BarPosition::Right => {
            let bar_inner_edge = input.monitor_bounds.origin.x + input.monitor_bounds.size.width
                - input.bar_thickness;
            let x = bar_inner_edge - r.width - gap;
            let y = s.origin.y + s.size.height / 2.0 - r.height / 2.0;
            Bounds {
                origin: Point { x, y },
                size: r,
            }
        }
    }
}

/// Clamp card origin so it stays within `[monitor + safe_inset, monitor_max - safe_inset]`.
fn clamp_to_monitor(card: Bounds<Pixels>, monitor: Bounds<Pixels>, safe: Pixels) -> Bounds<Pixels> {
    let min_x = monitor.origin.x + safe;
    let min_y = monitor.origin.y + safe;
    let max_x = monitor.origin.x + monitor.size.width - safe - card.size.width;
    let max_y = monitor.origin.y + monitor.size.height - safe - card.size.height;

    let x = card.origin.x.max(min_x).min(max_x.max(min_x));
    let y = card.origin.y.max(min_y).min(max_y.max(min_y));

    Bounds {
        origin: Point { x, y },
        size: card.size,
    }
}

/// Shrink card to fit within the monitor safe area (last resort).
fn shrink_to_monitor(
    card: Bounds<Pixels>,
    monitor: Bounds<Pixels>,
    safe: Pixels,
) -> Bounds<Pixels> {
    let max_width = monitor.size.width - safe * 2.0;
    let max_height = monitor.size.height - safe * 2.0;

    let w = card.size.width.min(max_width);
    let h = card.size.height.min(max_height);

    let x = card.origin.x.max(monitor.origin.x + safe);
    let y = card.origin.y.max(monitor.origin.y + safe);

    // Re-clamp after shrink.
    let x = x.min(monitor.origin.x + monitor.size.width - safe - w);
    let y = y.min(monitor.origin.y + monitor.size.height - safe - h);

    Bounds {
        origin: Point {
            x: x.max(monitor.origin.x + safe),
            y: y.max(monitor.origin.y + safe),
        },
        size: Size {
            width: w,
            height: h,
        },
    }
}

/// Resolve collision between a candidate card and an existing persistent card.
///
/// Tries in order:
///   a. Larger along-axis side of collision.
///   b. Smaller along-axis side.
///   c. One `gap` farther inward (deeper from bar edge).
///
/// Returns the best non-colliding candidate (may still overlap if nothing fits).
fn resolve_collision(
    card: Bounds<Pixels>,
    collision: Bounds<Pixels>,
    input: &PlacementInput,
    safe: Pixels,
    gap: Pixels,
) -> Bounds<Pixels> {
    if !overlaps(card, collision) {
        return card;
    }

    match input.bar_edge {
        BarPosition::Top | BarPosition::Bottom => {
            // Along-axis = horizontal.  Try right side, then left side.
            let right_x = collision.origin.x + collision.size.width + gap;
            let left_x = collision.origin.x - card.size.width - gap;
            let candidate_right = Bounds {
                origin: Point {
                    x: right_x,
                    y: card.origin.y,
                },
                size: card.size,
            };
            let candidate_left = Bounds {
                origin: Point {
                    x: left_x,
                    y: card.origin.y,
                },
                size: card.size,
            };

            let monitor = input.monitor_bounds;

            // Determine which side has more space.
            let space_right = (monitor.origin.x + monitor.size.width - safe - right_x)
                .max(px(0.0))
                .as_f32();
            let space_left = (left_x - safe - monitor.origin.x).max(px(0.0)).as_f32();

            let (primary, secondary) = if space_right >= space_left {
                (candidate_right, candidate_left)
            } else {
                (candidate_left, candidate_right)
            };

            if fits_horizontally(primary, monitor, safe) {
                let c = clamp_to_monitor(primary, monitor, safe);
                if !overlaps(c, collision) {
                    return c;
                }
            }
            if fits_horizontally(secondary, monitor, safe) {
                let c = clamp_to_monitor(secondary, monitor, safe);
                if !overlaps(c, collision) {
                    return c;
                }
            }

            // Fallback: place inward (further from bar edge).
            let inward_y = match input.bar_edge {
                BarPosition::Top => collision.origin.y + collision.size.height + gap,
                BarPosition::Bottom => collision.origin.y - card.size.height - gap,
                _ => card.origin.y,
            };
            clamp_to_monitor(
                Bounds {
                    origin: Point {
                        x: card.origin.x,
                        y: inward_y,
                    },
                    size: card.size,
                },
                monitor,
                safe,
            )
        }
        BarPosition::Left | BarPosition::Right => {
            // Along-axis = vertical.  Try below, then above.
            let below_y = collision.origin.y + collision.size.height + gap;
            let above_y = collision.origin.y - card.size.height - gap;
            let candidate_below = Bounds {
                origin: Point {
                    x: card.origin.x,
                    y: below_y,
                },
                size: card.size,
            };
            let candidate_above = Bounds {
                origin: Point {
                    x: card.origin.x,
                    y: above_y,
                },
                size: card.size,
            };

            let monitor = input.monitor_bounds;

            let space_below = (monitor.origin.y + monitor.size.height - safe - below_y)
                .max(px(0.0))
                .as_f32();
            let space_above = (above_y - safe - monitor.origin.y).max(px(0.0)).as_f32();

            let (primary, secondary) = if space_below >= space_above {
                (candidate_below, candidate_above)
            } else {
                (candidate_above, candidate_below)
            };

            if fits_vertically(primary, monitor, safe) {
                let c = clamp_to_monitor(primary, monitor, safe);
                if !overlaps(c, collision) {
                    return c;
                }
            }
            if fits_vertically(secondary, monitor, safe) {
                let c = clamp_to_monitor(secondary, monitor, safe);
                if !overlaps(c, collision) {
                    return c;
                }
            }

            // Fallback: place inward.
            let inward_x = match input.bar_edge {
                BarPosition::Left => collision.origin.x + collision.size.width + gap,
                BarPosition::Right => collision.origin.x - card.size.width - gap,
                _ => card.origin.x,
            };
            clamp_to_monitor(
                Bounds {
                    origin: Point {
                        x: inward_x,
                        y: card.origin.y,
                    },
                    size: card.size,
                },
                monitor,
                safe,
            )
        }
    }
}

fn overlaps(a: Bounds<Pixels>, b: Bounds<Pixels>) -> bool {
    a.origin.x < b.origin.x + b.size.width
        && a.origin.x + a.size.width > b.origin.x
        && a.origin.y < b.origin.y + b.size.height
        && a.origin.y + a.size.height > b.origin.y
}

fn fits_horizontally(card: Bounds<Pixels>, monitor: Bounds<Pixels>, safe: Pixels) -> bool {
    card.origin.x >= monitor.origin.x + safe
        && card.origin.x + card.size.width <= monitor.origin.x + monitor.size.width - safe
}

fn fits_vertically(card: Bounds<Pixels>, monitor: Bounds<Pixels>, safe: Pixels) -> bool {
    card.origin.y >= monitor.origin.y + safe
        && card.origin.y + card.size.height <= monitor.origin.y + monitor.size.height - safe
}

// ────────────────────────────────────────────────────────────────
// Tests
// ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{Bounds, Pixels, Point, Size, px};

    fn monitor_1080p() -> Bounds<Pixels> {
        Bounds {
            origin: Point {
                x: px(0.0),
                y: px(0.0),
            },
            size: Size {
                width: px(1920.0),
                height: px(1080.0),
            },
        }
    }

    fn monitor_with_offset(ox: f32, oy: f32) -> Bounds<Pixels> {
        Bounds {
            origin: Point {
                x: px(ox),
                y: px(oy),
            },
            size: Size {
                width: px(1920.0),
                height: px(1080.0),
            },
        }
    }

    fn source_center(monitor: Bounds<Pixels>) -> Bounds<Pixels> {
        Bounds {
            origin: Point {
                x: monitor.origin.x + px(960.0 - 24.0),
                y: monitor.origin.y + px(40.0),
            },
            size: Size {
                width: px(48.0),
                height: px(36.0),
            },
        }
    }

    fn standard_size() -> Size<Pixels> {
        Size {
            width: px(CardSizeTier::Standard.max_width()),
            height: px(CardSizeTier::Standard.max_height()),
        }
    }

    fn compact_size() -> Size<Pixels> {
        Size {
            width: px(CardSizeTier::Compact.max_width()),
            height: px(CardSizeTier::Compact.max_height()),
        }
    }

    fn expanded_size() -> Size<Pixels> {
        Size {
            width: px(CardSizeTier::Expanded.max_width()),
            height: px(CardSizeTier::Expanded.max_height()),
        }
    }

    fn input(
        monitor: Bounds<Pixels>,
        bar_edge: BarPosition,
        source: Bounds<Pixels>,
        size: Size<Pixels>,
        collision: Option<Bounds<Pixels>>,
    ) -> PlacementInput {
        PlacementInput {
            monitor_bounds: monitor,
            bar_edge,
            bar_thickness: px(40.0),
            source_bounds: source,
            requested_size: size,
            collision_bounds: collision,
        }
    }

    fn placed(result: PlacementResult) -> Bounds<Pixels> {
        match result {
            PlacementResult::Placed { card_bounds, .. } => card_bounds,
            PlacementResult::Suppressed { reason } => {
                panic!("Expected Placed but got Suppressed: {reason}")
            }
        }
    }

    // ── All four bar edges ────────────────────────────────────────

    #[test]
    fn top_bar_places_card_below_source() {
        let monitor = monitor_1080p();
        let source = source_center(monitor);
        let i = input(monitor, BarPosition::Top, source, standard_size(), None);
        let card = placed(compute_placement(&i));
        // Card should be below the source
        assert!(
            card.origin.y > source.origin.y,
            "Card should be below source for Top bar"
        );
        assert_eq!(
            card.origin.y,
            monitor.origin.y + i.bar_thickness + px(SOURCE_GAP),
            "perpendicular placement must be stable regardless of pointer position"
        );
    }

    #[test]
    fn bottom_bar_places_card_above_source() {
        let monitor = monitor_1080p();
        let source = Bounds {
            origin: Point {
                x: px(936.0),
                y: px(1000.0),
            },
            size: Size {
                width: px(48.0),
                height: px(36.0),
            },
        };
        let i = input(monitor, BarPosition::Bottom, source, compact_size(), None);
        let card = placed(compute_placement(&i));
        assert!(
            card.origin.y < source.origin.y,
            "Card should be above source for Bottom bar"
        );
    }

    #[test]
    fn left_bar_places_card_right_of_source() {
        let monitor = monitor_1080p();
        let source = Bounds {
            origin: Point {
                x: px(40.0),
                y: px(500.0),
            },
            size: Size {
                width: px(36.0),
                height: px(48.0),
            },
        };
        let i = input(monitor, BarPosition::Left, source, compact_size(), None);
        let card = placed(compute_placement(&i));
        assert!(
            card.origin.x > source.origin.x,
            "Card should be right of source for Left bar"
        );
        assert_eq!(
            card.origin.x,
            monitor.origin.x + i.bar_thickness + px(SOURCE_GAP),
            "perpendicular placement must be stable regardless of pointer position"
        );
    }

    #[test]
    fn right_bar_places_card_left_of_source() {
        let monitor = monitor_1080p();
        let source = Bounds {
            origin: Point {
                x: px(1840.0),
                y: px(500.0),
            },
            size: Size {
                width: px(36.0),
                height: px(48.0),
            },
        };
        let i = input(monitor, BarPosition::Right, source, compact_size(), None);
        let card = placed(compute_placement(&i));
        assert!(
            card.origin.x < source.origin.x,
            "Card should be left of source for Right bar"
        );
    }

    // ── Non-zero monitor origins (multi-monitor) ──────────────────

    #[test]
    fn non_zero_monitor_origin_is_handled() {
        let monitor = monitor_with_offset(1920.0, 0.0);
        let source = Bounds {
            origin: Point {
                x: px(2880.0),
                y: px(40.0),
            },
            size: Size {
                width: px(48.0),
                height: px(36.0),
            },
        };
        let i = input(monitor, BarPosition::Top, source, standard_size(), None);
        let card = placed(compute_placement(&i));

        // Card must be within monitor bounds + safe inset.
        assert!(
            card.origin.x >= monitor.origin.x + px(SAFE_INSET),
            "Card x origin should be within monitor safe area"
        );
        assert!(
            card.origin.x + card.size.width
                <= monitor.origin.x + monitor.size.width - px(SAFE_INSET),
            "Card right edge should be within monitor safe area"
        );
    }

    // ── Size tier maxima ──────────────────────────────────────────

    #[test]
    fn compact_tier_max_respected() {
        let monitor = monitor_1080p();
        let source = source_center(monitor);
        let i = input(monitor, BarPosition::Top, source, compact_size(), None);
        let card = placed(compute_placement(&i));
        assert!(card.size.width <= px(CardSizeTier::Compact.max_width()));
        assert!(card.size.height <= px(CardSizeTier::Compact.max_height()));
    }

    #[test]
    fn expanded_tier_max_respected() {
        let monitor = monitor_1080p();
        let source = source_center(monitor);
        let i = input(monitor, BarPosition::Top, source, expanded_size(), None);
        let card = placed(compute_placement(&i));
        assert!(card.size.width <= px(CardSizeTier::Expanded.max_width()));
        assert!(card.size.height <= px(CardSizeTier::Expanded.max_height()));
    }

    // ── Monitor clamping ──────────────────────────────────────────

    #[test]
    fn source_at_left_edge_clamps_card_to_safe_inset() {
        let monitor = monitor_1080p();
        let source = Bounds {
            origin: Point {
                x: px(0.0),
                y: px(40.0),
            },
            size: Size {
                width: px(48.0),
                height: px(36.0),
            },
        };
        let i = input(monitor, BarPosition::Top, source, standard_size(), None);
        let card = placed(compute_placement(&i));
        assert!(
            card.origin.x >= px(SAFE_INSET),
            "Card should be clamped to SAFE_INSET from left edge"
        );
    }

    #[test]
    fn source_at_right_edge_clamps_card_to_safe_inset() {
        let monitor = monitor_1080p();
        let source = Bounds {
            origin: Point {
                x: px(1900.0),
                y: px(40.0),
            },
            size: Size {
                width: px(20.0),
                height: px(36.0),
            },
        };
        let i = input(monitor, BarPosition::Top, source, standard_size(), None);
        let card = placed(compute_placement(&i));
        assert!(
            card.origin.x + card.size.width <= px(1920.0 - SAFE_INSET),
            "Card right edge should be within monitor safe area"
        );
    }

    // ── Collision avoidance ───────────────────────────────────────

    #[test]
    fn collision_avoidance_moves_preview_off_persistent() {
        let monitor = monitor_1080p();
        let source = source_center(monitor);
        // Persistent card at same position as initial placement
        let persistent = Bounds {
            origin: Point {
                x: px(780.0),
                y: px(86.0),
            },
            size: Size {
                width: px(360.0),
                height: px(480.0),
            },
        };
        let i = input(
            monitor,
            BarPosition::Top,
            source,
            compact_size(),
            Some(persistent),
        );
        let card = placed(compute_placement(&i));
        // Preview card should not overlap persistent
        assert!(
            !overlaps(card, persistent),
            "Preview card should not overlap persistent card"
        );
    }

    #[test]
    fn collision_prefers_larger_side() {
        let monitor = monitor_1080p();
        // Source near left — collision on right forces left placement.
        let source = Bounds {
            origin: Point {
                x: px(400.0),
                y: px(40.0),
            },
            size: Size {
                width: px(48.0),
                height: px(36.0),
            },
        };
        let persistent = Bounds {
            origin: Point {
                x: px(350.0),
                y: px(86.0),
            },
            size: Size {
                width: px(280.0),
                height: px(240.0),
            },
        };
        let i = input(
            monitor,
            BarPosition::Top,
            source,
            compact_size(),
            Some(persistent),
        );
        let card = placed(compute_placement(&i));
        assert!(!overlaps(card, persistent));
    }

    // ── Shrinking below minimum size → Suppressed ─────────────────

    #[test]
    fn tiny_monitor_suppresses_card() {
        let tiny = Bounds {
            origin: Point {
                x: px(0.0),
                y: px(0.0),
            },
            size: Size {
                width: px(100.0),
                height: px(80.0),
            },
        };
        let source = Bounds {
            origin: Point {
                x: px(26.0),
                y: px(40.0),
            },
            size: Size {
                width: px(48.0),
                height: px(36.0),
            },
        };
        let i = input(tiny, BarPosition::Top, source, standard_size(), None);
        assert!(
            matches!(compute_placement(&i), PlacementResult::Suppressed { .. }),
            "Should suppress card when monitor is too small"
        );
    }

    // ── Band geometry ─────────────────────────────────────────────

    #[test]
    fn top_bar_band_geometry_starts_at_bar_bottom() {
        let monitor = monitor_1080p();
        let source = source_center(monitor);
        let i = input(monitor, BarPosition::Top, source, compact_size(), None);
        let band = compute_band_geometry(&i);
        assert_eq!(band.bounds.origin.x, px(0.0));
        assert_eq!(band.bounds.origin.y, px(40.0)); // bar thickness
        assert_eq!(band.bounds.size.width, px(1920.0));
    }

    #[test]
    fn persistent_top_band_covers_the_remaining_monitor_work_area() {
        let monitor = monitor_1080p();
        let source = source_center(monitor);
        let i = input(monitor, BarPosition::Top, source, compact_size(), None);
        let band = compute_persistent_band_geometry(&i);

        assert_eq!(
            band.bounds.origin,
            Point {
                x: px(0.0),
                y: i.bar_thickness
            }
        );
        assert_eq!(band.bounds.size.width, i.monitor_bounds.size.width);
        assert_eq!(
            band.bounds.size.height,
            i.monitor_bounds.size.height - i.bar_thickness
        );
    }

    #[test]
    fn bottom_bar_band_geometry_ends_at_bar_top() {
        let monitor = monitor_1080p();
        let source = Bounds {
            origin: Point {
                x: px(936.0),
                y: px(1000.0),
            },
            size: Size {
                width: px(48.0),
                height: px(36.0),
            },
        };
        let i = input(monitor, BarPosition::Bottom, source, compact_size(), None);
        let band = compute_band_geometry(&i);
        // band bottom = monitor bottom - bar thickness
        let band_bottom = band.bounds.origin.y + band.bounds.size.height;
        assert_eq!(band_bottom, monitor.size.height - i.bar_thickness);
    }

    #[test]
    fn left_bar_band_starts_at_bar_right_edge() {
        let monitor = monitor_1080p();
        let source = Bounds {
            origin: Point {
                x: px(40.0),
                y: px(500.0),
            },
            size: Size {
                width: px(36.0),
                height: px(48.0),
            },
        };
        let i = input(monitor, BarPosition::Left, source, compact_size(), None);
        let band = compute_band_geometry(&i);
        assert_eq!(band.bounds.origin.x, i.bar_thickness);
        assert_eq!(band.bounds.origin.y, px(0.0));
        assert_eq!(band.bounds.size.height, px(1080.0));
    }

    #[test]
    fn right_bar_band_ends_at_bar_left_edge() {
        let monitor = monitor_1080p();
        let source = Bounds {
            origin: Point {
                x: px(1840.0),
                y: px(500.0),
            },
            size: Size {
                width: px(36.0),
                height: px(48.0),
            },
        };
        let i = input(monitor, BarPosition::Right, source, compact_size(), None);
        let band = compute_band_geometry(&i);
        let band_right = band.bounds.origin.x + band.bounds.size.width;
        assert_eq!(
            band_right,
            monitor.size.width - i.bar_thickness,
            "Right-bar band right edge should align with bar left edge"
        );
    }
}
