use std::{cell::Cell, rc::Rc, time::Duration};

use gpui::{
    Animation, AnimationExt as _, App, ElementId, IntoElement, Pixels, Styled, Window, div, px,
};
use shilpo_ui::ActiveTheme;

pub const PILL_SLOT_SIZE: f32 = 26.;
pub const PILL_ACTIVE_MARGIN: f32 = 2.;
pub const PILL_INDICATOR_SIZE: f32 = PILL_SLOT_SIZE - (PILL_ACTIVE_MARGIN * 2.);
pub const PILL_MOTION_DURATION: Duration = Duration::from_millis(300);
pub const PILL_LEADING_EDGE_DURATION: Duration = Duration::from_millis(100);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PillOrientation {
    Horizontal,
    Vertical,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PillIndicatorGeometry {
    pub position: Pixels,
    pub size: Pixels,
}

pub fn indicator_target(index: usize) -> PillIndicatorGeometry {
    PillIndicatorGeometry {
        position: px(index as f32 * PILL_SLOT_SIZE + PILL_ACTIVE_MARGIN),
        size: px(PILL_INDICATOR_SIZE),
    }
}

pub fn out_sine(progress: f32) -> f32 {
    (progress.clamp(0., 1.) * std::f32::consts::FRAC_PI_2).sin()
}

pub fn lerp_pixels(from: Pixels, target: Pixels, progress: f32) -> Pixels {
    let from: f32 = from.into();
    let target: f32 = target.into();
    px(from + (target - from) * progress)
}

pub fn calculate_stretching_geometry(
    from: PillIndicatorGeometry,
    target: PillIndicatorGeometry,
    delta: f32,
) -> PillIndicatorGeometry {
    if delta >= 1. {
        return target;
    }

    let elapsed = PILL_MOTION_DURATION.as_secs_f32() * delta.clamp(0., 1.);
    let fast = out_sine(elapsed / PILL_LEADING_EDGE_DURATION.as_secs_f32());
    let slow = out_sine(elapsed / PILL_MOTION_DURATION.as_secs_f32());

    let from_start = from.position;
    let from_end = from.position + from.size;
    let target_start = target.position;
    let target_end = target.position + target.size;
    let moving_forward = target.position >= from.position;

    let (start, end) = if moving_forward {
        (
            lerp_pixels(from_start, target_start, slow),
            lerp_pixels(from_end, target_end, fast),
        )
    } else {
        (
            lerp_pixels(from_start, target_start, fast),
            lerp_pixels(from_end, target_end, slow),
        )
    };

    PillIndicatorGeometry {
        position: start,
        size: (end - start).max(px(PILL_INDICATOR_SIZE)),
    }
}

#[derive(Clone)]
pub struct PillMotionState {
    pub target_index: usize,
    pub from: PillIndicatorGeometry,
    pub target: PillIndicatorGeometry,
    pub current: Rc<Cell<PillIndicatorGeometry>>,
    pub active_generation: Rc<Cell<u64>>,
    pub generation: u64,
    pub duration: Duration,
    pub active: bool,
}

impl PillMotionState {
    pub fn new(target_index: usize, target: PillIndicatorGeometry) -> Self {
        Self {
            target_index,
            from: target,
            target,
            current: Rc::new(Cell::new(target)),
            active_generation: Rc::new(Cell::new(0)),
            generation: 0,
            duration: PILL_MOTION_DURATION,
            active: false,
        }
    }

    pub fn retarget(&mut self, target_index: usize, target: PillIndicatorGeometry) -> u64 {
        if self.target_index == target_index && self.target == target {
            return self.generation;
        }

        self.generation = self.generation.wrapping_add(1);
        self.from = self.current.get();
        self.target_index = target_index;
        self.target = target;
        self.duration = PILL_MOTION_DURATION;
        self.active = true;
        self.active_generation.set(self.generation);
        self.generation
    }
}

pub fn render_active_pill_indicator(
    id: &ElementId,
    active_idx: Option<usize>,
    orientation: PillOrientation,
    reduced_motion: bool,
    window: &mut Window,
    cx: &mut App,
) -> gpui::AnyElement {
    let Some(active_idx) = active_idx else {
        return div().into_any_element();
    };

    let target_geom = indicator_target(active_idx);
    let motion_key = format!("pill-motion:{}", id);
    let animation_name = format!("pill-indicator-motion:{}", id);

    let motion = window.use_keyed_state(motion_key, cx, |_, _| {
        PillMotionState::new(active_idx, target_geom)
    });

    let snapshot = motion.read(cx).clone();

    if snapshot.target_index != active_idx || snapshot.target != target_geom {
        let generation = motion.update(cx, |state, _| state.retarget(active_idx, target_geom));
        let duration = motion.read(cx).duration;
        let motion = motion.clone();
        cx.spawn(async move |cx| {
            cx.background_executor().timer(duration).await;
            motion.update(cx, |state, cx| {
                if state.generation == generation {
                    state.active = false;
                    state.current.set(target_geom);
                    cx.notify();
                }
            });
        })
        .detach();
    }

    let state = motion.read(cx);
    let from_geometry = state.from;
    let current_geometry = state.current.clone();
    let active_generation = state.active_generation.clone();
    let generation = state.generation;
    let duration = state.duration;
    let active = state.active && !reduced_motion;

    let pill = div().absolute().rounded_full().bg(cx.theme().primary);

    let pill = match orientation {
        PillOrientation::Horizontal => pill
            .top(px(PILL_ACTIVE_MARGIN))
            .left(target_geom.position)
            .w(target_geom.size)
            .h(px(PILL_INDICATOR_SIZE)),
        PillOrientation::Vertical => pill
            .left(px(PILL_ACTIVE_MARGIN))
            .top(target_geom.position)
            .h(target_geom.size)
            .w(px(PILL_INDICATOR_SIZE)),
    };

    if active {
        pill.with_animation(
            ElementId::NamedInteger(animation_name.into(), generation),
            Animation::new(duration),
            move |pill, delta| {
                let geometry = calculate_stretching_geometry(from_geometry, target_geom, delta);
                if active_generation.get() == generation {
                    current_geometry.set(geometry);
                }
                match orientation {
                    PillOrientation::Horizontal => pill.left(geometry.position).w(geometry.size),
                    PillOrientation::Vertical => pill.top(geometry.position).h(geometry.size),
                }
            },
        )
        .into_any_element()
    } else {
        pill.into_any_element()
    }
}
