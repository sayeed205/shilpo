use std::{cell::Cell, rc::Rc, time::Duration};

use gpui::{Pixels, px};

use crate::{animation::Lerp as _, motion::SpringSpec};

pub(super) const COLLAPSED_WIDTH: Pixels = px(80.);
pub(super) const EXPANDED_WIDTH: Pixels = px(240.);

const COLLAPSED_INDICATOR_WIDTH: Pixels = px(56.);
const COLLAPSED_INDICATOR_HEIGHT: Pixels = px(32.);
const COLLAPSED_ITEM_STRIDE: Pixels = px(48.);
const COLLAPSED_INDICATOR_INSET: Pixels = px(4.);

const EXPANDED_INDICATOR_WIDTH: Pixels = px(216.);
const EXPANDED_INDICATOR_HEIGHT: Pixels = px(48.);
const EXPANDED_ITEM_STRIDE: Pixels = px(56.);

pub(super) const SELECTION_MOTION_DURATION: Duration = Duration::from_millis(420);
pub(super) const LAYOUT_MOTION_DURATION: Duration = Duration::from_millis(500);

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct RailGeometry {
    pub(super) rail_width: Pixels,
    pub(super) indicator_top: Pixels,
    pub(super) indicator_width: Pixels,
    pub(super) indicator_height: Pixels,
}

impl RailGeometry {
    pub(super) fn resolve(selected_index: Option<usize>, collapsed: bool) -> Self {
        let index = selected_index.unwrap_or_default() as f32;
        if collapsed {
            Self {
                rail_width: COLLAPSED_WIDTH,
                indicator_top: COLLAPSED_ITEM_STRIDE * index + COLLAPSED_INDICATOR_INSET,
                indicator_width: COLLAPSED_INDICATOR_WIDTH,
                indicator_height: COLLAPSED_INDICATOR_HEIGHT,
            }
        } else {
            Self {
                rail_width: EXPANDED_WIDTH,
                indicator_top: EXPANDED_ITEM_STRIDE * index,
                indicator_width: EXPANDED_INDICATOR_WIDTH,
                indicator_height: EXPANDED_INDICATOR_HEIGHT,
            }
        }
    }

    /// Interpolates each dimension while limiting spring overshoot in pixels.
    ///
    /// A raw spring percentage makes long indicator journeys overshoot much
    /// farther than short ones. Pixel caps keep the response lively without
    /// allowing a destination several rows away to fly past its target.
    pub(super) fn spring_lerp(self, target: Self, progress: f32) -> Self {
        Self {
            rail_width: limited_spring_lerp(self.rail_width, target.rail_width, progress, px(4.)),
            indicator_top: limited_spring_lerp(
                self.indicator_top,
                target.indicator_top,
                progress,
                px(6.),
            ),
            indicator_width: limited_spring_lerp(
                self.indicator_width,
                target.indicator_width,
                progress,
                px(8.),
            ),
            indicator_height: limited_spring_lerp(
                self.indicator_height,
                target.indicator_height,
                progress,
                px(2.),
            ),
        }
    }
}

fn limited_spring_lerp(
    from: Pixels,
    target: Pixels,
    progress: f32,
    max_overshoot: Pixels,
) -> Pixels {
    if progress <= 1. {
        return from.lerp(&target, progress);
    }

    let distance: f32 = (target - from).into();
    if distance.abs() <= f32::EPSILON {
        return target;
    }

    let max_overshoot: f32 = max_overshoot.into();
    let limited_progress = progress.min(1. + max_overshoot / distance.abs());
    from.lerp(&target, limited_progress)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct RailTarget {
    pub(super) selected_index: Option<usize>,
    pub(super) collapsed: bool,
}

impl RailTarget {
    pub(super) fn geometry(self) -> RailGeometry {
        RailGeometry::resolve(self.selected_index, self.collapsed)
    }
}

#[derive(Clone)]
pub(super) struct RailMotionState {
    pub(super) target: RailTarget,
    pub(super) from: RailGeometry,
    pub(super) current: Rc<Cell<RailGeometry>>,
    pub(super) active_generation: Rc<Cell<u64>>,
    pub(super) generation: u64,
    pub(super) duration: Duration,
    pub(super) spring: SpringSpec,
    pub(super) active: bool,
}

impl RailMotionState {
    pub(super) fn new(target: RailTarget) -> Self {
        let geometry = target.geometry();
        Self {
            target,
            from: geometry,
            current: Rc::new(Cell::new(geometry)),
            active_generation: Rc::new(Cell::new(0)),
            generation: 0,
            duration: SELECTION_MOTION_DURATION,
            spring: SpringSpec::EXPRESSIVE_FAST_SPATIAL,
            active: false,
        }
    }

    pub(super) fn retarget(&mut self, target: RailTarget) -> u64 {
        let layout_changed = self.target.collapsed != target.collapsed;
        self.generation = self.generation.wrapping_add(1);
        self.from = self.current.get();

        // There is no meaningful previous indicator position when selection
        // first appears. Seed it at its destination so it never flies in from
        // row zero while the rail itself can still morph between widths.
        if self.target.selected_index.is_none() || target.selected_index.is_none() {
            let target_geometry = target.geometry();
            self.from.indicator_top = target_geometry.indicator_top;
            self.from.indicator_width = target_geometry.indicator_width;
            self.from.indicator_height = target_geometry.indicator_height;
            self.current.set(self.from);
        }

        self.target = target;
        self.duration = if layout_changed {
            LAYOUT_MOTION_DURATION
        } else {
            SELECTION_MOTION_DURATION
        };
        self.spring = if layout_changed {
            SpringSpec::EXPRESSIVE_DEFAULT_SPATIAL
        } else {
            SpringSpec::EXPRESSIVE_FAST_SPATIAL
        };
        self.active = true;
        self.active_generation.set(self.generation);
        self.generation
    }
}

pub(super) fn spring_progress(delta: f32, duration: Duration, spring: SpringSpec) -> f32 {
    if delta >= 1. {
        1.
    } else {
        spring.evaluate(delta * duration.as_secs_f32())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn value(pixels: Pixels) -> f32 {
        pixels.into()
    }

    #[test]
    fn geometry_keeps_the_selected_row_when_layout_changes() {
        let collapsed = RailGeometry::resolve(Some(4), true);
        let expanded = RailGeometry::resolve(Some(4), false);

        assert_eq!(value(collapsed.indicator_top), 196.);
        assert_eq!(value(expanded.indicator_top), 224.);
        assert_eq!(collapsed.indicator_width, px(56.));
        assert_eq!(expanded.indicator_width, px(216.));
    }

    #[test]
    fn layout_retarget_starts_from_the_same_selected_row() {
        let mut state = RailMotionState::new(RailTarget {
            selected_index: Some(4),
            collapsed: false,
        });
        state.retarget(RailTarget {
            selected_index: Some(4),
            collapsed: true,
        });

        assert_eq!(value(state.from.indicator_top), 224.);
        assert_eq!(value(state.target.geometry().indicator_top), 196.);
    }

    #[test]
    fn first_selection_is_seeded_at_its_destination() {
        let mut state = RailMotionState::new(RailTarget {
            selected_index: None,
            collapsed: true,
        });
        state.retarget(RailTarget {
            selected_index: Some(7),
            collapsed: true,
        });

        assert_eq!(
            state.from.indicator_top,
            state.target.geometry().indicator_top
        );
    }

    #[test]
    fn long_journeys_have_bounded_overshoot() {
        let from = RailGeometry::resolve(Some(0), true);
        let target = RailGeometry::resolve(Some(8), true);
        let overshot = from.spring_lerp(target, 1.1);

        assert_eq!(
            value(overshot.indicator_top),
            value(target.indicator_top) + 6.
        );
    }

    #[test]
    fn spring_finishes_at_the_exact_target() {
        assert_eq!(
            spring_progress(
                1.,
                SELECTION_MOTION_DURATION,
                SpringSpec::EXPRESSIVE_FAST_SPATIAL,
            ),
            1.
        );
    }
}
