use std::ops::Range;

use crate::{ActiveTheme, AxisExt, ElementExt, Sizable, StyledExt, h_flex};
use gpui::{
    AccessibleAction, Along, App, AppContext as _, Axis, Background, Bounds, Context, Corners,
    DefiniteLength, DragMoveEvent, Empty, Entity, EntityId, EventEmitter, InteractiveElement,
    IntoElement, IsZero, MouseButton, MouseDownEvent, Orientation, ParentElement as _, Pixels,
    Point, Render, RenderOnce, Role, StatefulInteractiveElement as _, StyleRefinement, Styled,
    Window, div, prelude::FluentBuilder as _, px, relative,
};

#[derive(Clone)]
struct DragThumb((EntityId, bool));

impl Render for DragThumb {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        Empty
    }
}

#[derive(Clone)]
struct DragSlider(EntityId);

impl Render for DragSlider {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        Empty
    }
}

/// Events emitted by the [`SliderState`].
pub enum SliderEvent {
    /// Emitted continuously while the slider value is being changed by the user.
    Change(SliderValue),
    /// Emitted once when the user releases the slider after a drag or click.
    Release(SliderValue),
}

/// The value of the slider, can be a single value or a range of values.
///
/// - Can from a f32 value, which will be treated as a single value.
/// - Or from a (f32, f32) tuple, which will be treated as a range of values.
///
/// The default value is `SliderValue::Single(0.0)`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SliderValue {
    Single(f32),
    Range(f32, f32),
}

impl std::fmt::Display for SliderValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SliderValue::Single(value) => write!(f, "{}", value),
            SliderValue::Range(start, end) => write!(f, "{}..{}", start, end),
        }
    }
}

impl From<f32> for SliderValue {
    fn from(value: f32) -> Self {
        SliderValue::Single(value)
    }
}

impl From<(f32, f32)> for SliderValue {
    fn from(value: (f32, f32)) -> Self {
        SliderValue::Range(value.0, value.1)
    }
}

impl From<Range<f32>> for SliderValue {
    fn from(value: Range<f32>) -> Self {
        SliderValue::Range(value.start, value.end)
    }
}

impl Default for SliderValue {
    fn default() -> Self {
        SliderValue::Single(0.)
    }
}

impl SliderValue {
    /// Clamp the value to the given range.
    pub fn clamp(self, min: f32, max: f32) -> Self {
        match self {
            SliderValue::Single(value) => SliderValue::Single(value.clamp(min, max)),
            SliderValue::Range(start, end) => {
                SliderValue::Range(start.clamp(min, max), end.clamp(min, max))
            }
        }
    }

    /// Check if the value is a single value.
    #[inline]
    pub fn is_single(&self) -> bool {
        matches!(self, SliderValue::Single(_))
    }

    /// Check if the value is a range of values.
    #[inline]
    pub fn is_range(&self) -> bool {
        matches!(self, SliderValue::Range(_, _))
    }

    /// Get the start value.
    pub fn start(&self) -> f32 {
        match self {
            SliderValue::Single(value) => *value,
            SliderValue::Range(start, _) => *start,
        }
    }

    /// Get the end value.
    pub fn end(&self) -> f32 {
        match self {
            SliderValue::Single(value) => *value,
            SliderValue::Range(_, end) => *end,
        }
    }

    fn set_start(&mut self, value: f32) {
        if let SliderValue::Range(_, end) = self {
            *self = SliderValue::Range(value.min(*end), *end);
        } else {
            *self = SliderValue::Single(value);
        }
    }

    fn set_end(&mut self, value: f32) {
        if let SliderValue::Range(start, _) = self {
            *self = SliderValue::Range(*start, value.max(*start));
        } else {
            *self = SliderValue::Single(value);
        }
    }
}

/// The scale mode of the slider.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SliderScale {
    /// Linear scale where values change uniformly across the slider range.
    /// This is the default mode.
    #[default]
    Linear,
    /// Logarithmic scale where the distance between values increases exponentially.
    ///
    /// This is useful for parameters that have a large range of values where smaller
    /// changes are more significant at lower values. Common examples include:
    ///
    /// - Volume controls (human hearing perception is logarithmic)
    /// - Frequency controls (musical notes follow a logarithmic scale)
    /// - Zoom levels
    /// - Any parameter where you want finer control at lower values
    ///
    /// # For example
    ///
    /// ```
    /// use shilpo_ui::slider::{SliderState, SliderScale};
    ///
    /// let slider = SliderState::new()
    ///     .min(1.0)    // Must be > 0 for logarithmic scale
    ///     .max(1000.0)
    ///     .scale(SliderScale::Logarithmic);
    /// ```
    ///
    /// - Moving the slider 1/3 of the way will yield ~10
    /// - Moving it 2/3 of the way will yield ~100
    /// - The full range covers 3 orders of magnitude evenly
    Logarithmic,
}

impl SliderScale {
    #[inline]
    pub fn is_linear(&self) -> bool {
        matches!(self, SliderScale::Linear)
    }

    #[inline]
    pub fn is_logarithmic(&self) -> bool {
        matches!(self, SliderScale::Logarithmic)
    }
}

/// State of the [`Slider`].
pub struct SliderState {
    min: f32,
    max: f32,
    step: f32,
    value: SliderValue,
    /// When is single value mode, only `end` is used, the start is always 0.0.
    percentage: Range<f32>,
    /// The bounds of the slider after rendered.
    bounds: Bounds<Pixels>,
    scale: SliderScale,
    /// Tracks whether the user is currently interacting with the slider so we
    /// only emit [`SliderEvent::Release`] after a real press/drag.
    dragging: bool,
    centered: bool,
    dragging_thumb: Option<bool>,
}

impl SliderState {
    /// Create a new [`SliderState`].
    pub fn new() -> Self {
        Self {
            min: 0.0,
            max: 100.0,
            step: 1.0,
            value: SliderValue::default(),
            percentage: (0.0..0.0),
            bounds: Bounds::default(),
            scale: SliderScale::default(),
            dragging: false,
            centered: false,
            dragging_thumb: None,
        }
    }

    /// Set the centered state of the slider (bi-directional centered mode), default: false
    pub fn centered(mut self, centered: bool) -> Self {
        self.centered = centered;
        self.update_thumb_pos();
        self
    }

    /// Set the minimum value of the slider, default: 0.0
    pub fn min(mut self, min: f32) -> Self {
        if self.scale.is_logarithmic() {
            assert!(
                min > 0.0,
                "`min` must be greater than 0 for SliderScale::Logarithmic"
            );
            assert!(
                min < self.max,
                "`min` must be less than `max` for Logarithmic scale"
            );
        }
        self.min = min;
        self.update_thumb_pos();
        self
    }

    /// Set the maximum value of the slider, default: 100.0
    pub fn max(mut self, max: f32) -> Self {
        if self.scale.is_logarithmic() {
            assert!(
                max > self.min,
                "`max` must be greater than `min` for Logarithmic scale"
            );
        }
        self.max = max;
        self.update_thumb_pos();
        self
    }

    /// Set the step value of the slider, default: 1.0
    pub fn step(mut self, step: f32) -> Self {
        self.step = step;
        self
    }

    /// Set the scale of the slider, default: [`SliderScale::Linear`].
    pub fn scale(mut self, scale: SliderScale) -> Self {
        if scale.is_logarithmic() {
            assert!(
                self.min > 0.0,
                "`min` must be greater than 0 for Logarithmic scale"
            );
            assert!(
                self.max > self.min,
                "`max` must be greater than `min` for Logarithmic scale"
            );
        }
        self.scale = scale;
        self.update_thumb_pos();
        self
    }

    /// Set the default value of the slider, default: 0.0
    pub fn default_value(mut self, value: impl Into<SliderValue>) -> Self {
        self.value = value.into();
        self.update_thumb_pos();
        self
    }

    /// Set the value of the slider.
    pub fn set_value(
        &mut self,
        value: impl Into<SliderValue>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.value = value.into();
        self.update_thumb_pos();
        cx.notify();
    }

    /// Get the value of the slider.
    pub fn value(&self) -> SliderValue {
        self.value
    }

    /// Steps up the slider value by step.
    pub fn step_up(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self.value {
            SliderValue::Single(val) => {
                let next = (val + self.step).min(self.max);
                self.set_value(next, window, cx);
                cx.emit(SliderEvent::Change(self.value));
            }
            SliderValue::Range(start, end) => {
                let next_end = (end + self.step).min(self.max);
                self.set_value((start, next_end), window, cx);
                cx.emit(SliderEvent::Change(self.value));
            }
        }
    }

    /// Steps down the slider value by step.
    pub fn step_down(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self.value {
            SliderValue::Single(val) => {
                let next = (val - self.step).max(self.min);
                self.set_value(next, window, cx);
                cx.emit(SliderEvent::Change(self.value));
            }
            SliderValue::Range(start, end) => {
                let next_start = (start - self.step).max(self.min);
                self.set_value((next_start, end), window, cx);
                cx.emit(SliderEvent::Change(self.value));
            }
        }
    }

    /// Sets slider to min.
    pub fn jump_to_min(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self.value {
            SliderValue::Single(_) => {
                self.set_value(self.min, window, cx);
                cx.emit(SliderEvent::Change(self.value));
            }
            SliderValue::Range(_, end) => {
                self.set_value((self.min, end), window, cx);
                cx.emit(SliderEvent::Change(self.value));
            }
        }
    }

    /// Sets slider to max.
    pub fn jump_to_max(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self.value {
            SliderValue::Single(_) => {
                self.set_value(self.max, window, cx);
                cx.emit(SliderEvent::Change(self.value));
            }
            SliderValue::Range(start, _) => {
                self.set_value((start, self.max), window, cx);
                cx.emit(SliderEvent::Change(self.value));
            }
        }
    }

    /// Get the minimum value.
    pub fn min_value(&self) -> f32 {
        self.min
    }

    /// Get the maximum value.
    pub fn max_value(&self) -> f32 {
        self.max
    }

    /// Get the step value.
    pub fn step_value(&self) -> f32 {
        self.step
    }

    /// Check if centered mode is enabled.
    pub fn is_centered(&self) -> bool {
        self.centered
    }

    /// Converts a value between 0.0 and 1.0 to a value between the minimum and maximum value,
    /// depending on the chosen scale.
    fn percentage_to_value(&self, percentage: f32) -> f32 {
        match self.scale {
            SliderScale::Linear => self.min + (self.max - self.min) * percentage,
            SliderScale::Logarithmic => {
                // when percentage is 0, this simplifies to (max/min)^0 * min = 1 * min = min
                // when percentage is 1, this simplifies to (max/min)^1 * min = (max*min)/min = max
                // we clamp just to make sure we don't have issue with floating point precision
                let base = self.max / self.min;
                (base.powf(percentage) * self.min).clamp(self.min, self.max)
            }
        }
    }

    /// Converts a value between the minimum and maximum value to a value between 0.0 and 1.0,
    /// depending on the chosen scale.
    fn value_to_percentage(&self, value: f32) -> f32 {
        match self.scale {
            SliderScale::Linear => {
                let range = self.max - self.min;
                if range <= 0.0 {
                    0.0
                } else {
                    (value - self.min) / range
                }
            }
            SliderScale::Logarithmic => {
                let base = self.max / self.min;
                (value / self.min).log(base).clamp(0.0, 1.0)
            }
        }
    }

    fn update_thumb_pos(&mut self) {
        match self.value {
            SliderValue::Single(value) => {
                let percentage = self.value_to_percentage(value.clamp(self.min, self.max));
                self.percentage = 0.0..percentage;
            }
            SliderValue::Range(start, end) => {
                let clamped_start = start.clamp(self.min, self.max);
                let clamped_end = end.clamp(self.min, self.max);
                self.percentage =
                    self.value_to_percentage(clamped_start)..self.value_to_percentage(clamped_end);
            }
        }
    }

    /// Update value by mouse position
    fn update_value_by_position(
        &mut self,
        axis: Axis,
        position: Point<Pixels>,
        is_start: bool,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.dragging = true;
        self.dragging_thumb = Some(is_start);
        let bounds = self.bounds;
        let step = self.step;

        let inner_pos = if axis.is_horizontal() {
            position.x - bounds.left()
        } else {
            bounds.bottom() - position.y
        };
        let total_size = bounds.size.along(axis);
        let percentage = inner_pos.clamp(px(0.), total_size) / total_size;

        let percentage = if is_start {
            percentage.clamp(0.0, self.percentage.end)
        } else {
            percentage.clamp(self.percentage.start, 1.0)
        };

        let value = self.percentage_to_value(percentage);
        let value = (value / step).round() * step;
        let snapped_percentage = self.value_to_percentage(value);

        if is_start {
            self.percentage.start = snapped_percentage;
            self.value.set_start(value);
        } else {
            self.percentage.end = snapped_percentage;
            self.value.set_end(value);
        }
        cx.emit(SliderEvent::Change(self.value));
        cx.notify();
    }

    /// Emit [`SliderEvent::Release`] if the user was actively interacting
    /// with the slider. Called on mouse-up both inside and outside the slider.
    fn handle_release(&mut self, cx: &mut Context<Self>) {
        if !self.dragging {
            return;
        }
        self.dragging = false;
        self.dragging_thumb = None;
        cx.emit(SliderEvent::Release(self.value));
    }
}

impl EventEmitter<SliderEvent> for SliderState {}

/// A Slider element.
#[derive(IntoElement)]
pub struct Slider {
    state: Entity<SliderState>,
    axis: Axis,
    style: StyleRefinement,
    disabled: bool,
    reverse: bool,
    size: crate::Size,
}

impl Sizable for Slider {
    fn with_size(mut self, size: impl Into<crate::Size>) -> Self {
        self.size = size.into();
        self
    }
}

impl Slider {
    /// Create a new [`Slider`] element bind to the [`SliderState`].
    pub fn new(state: &Entity<SliderState>) -> Self {
        Self {
            axis: Axis::Horizontal,
            state: state.clone(),
            style: StyleRefinement::default(),
            disabled: false,
            reverse: false,
            size: crate::Size::Medium,
        }
    }

    /// As a horizontal slider.
    pub fn horizontal(mut self) -> Self {
        self.axis = Axis::Horizontal;
        self
    }

    /// As a vertical slider.
    pub fn vertical(mut self) -> Self {
        self.axis = Axis::Vertical;
        self
    }

    /// Set the disabled state of the slider, default: false
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// Reverse the filled (highlighted) side of the track, default: false.
    ///
    /// By default the track is filled from the min end to the thumb. With
    /// `reverse`, the fill goes from the thumb to the max end instead — useful
    /// when the slider represents a remaining amount (e.g. time left).
    ///
    /// This only changes the visual fill; values, events and interactions are
    /// unaffected. It applies to single-value sliders and is ignored for
    /// range sliders.
    pub fn reverse(mut self) -> Self {
        self.reverse = true;
        self
    }

    #[allow(clippy::too_many_arguments)]
    fn render_thumb(
        &self,
        start: DefiniteLength,
        is_start: bool,
        _bar_color: Background,
        thumb_bg: Background,
        _radius: Corners<Pixels>,
        window: &mut Window,
        cx: &mut App,
    ) -> impl gpui::IntoElement {
        let entity_id = self.state.entity_id();
        let axis = self.axis;
        let id = ("slider-thumb", is_start as u32);

        if self.disabled {
            return div().id(id);
        }

        let state = self.state.read(cx);
        let is_dragging = state.dragging && state.dragging_thumb == Some(is_start);

        let (thumb_width, thumb_height, top_offset, left_offset, margin_left, margin_bottom) =
            if axis.is_horizontal() {
                let tw = if is_dragging { px(8.) } else { px(4.) };
                let th = match self.size {
                    crate::Size::XSmall => px(20.),
                    crate::Size::Small => px(26.),
                    crate::Size::Medium => px(32.),
                    crate::Size::Large => px(38.),
                    _ => px(32.),
                };
                let track_h = match self.size {
                    crate::Size::XSmall => px(8.),
                    crate::Size::Small => px(12.),
                    crate::Size::Medium => px(16.),
                    crate::Size::Large => px(20.),
                    _ => px(16.),
                };
                let top = (track_h - th) * 0.5;
                let ml = tw * -0.5;
                (tw, th, top, start, ml, px(0.))
            } else {
                let th = if is_dragging { px(8.) } else { px(4.) };
                let tw = match self.size {
                    crate::Size::XSmall => px(20.),
                    crate::Size::Small => px(26.),
                    crate::Size::Medium => px(32.),
                    crate::Size::Large => px(38.),
                    _ => px(32.),
                };
                let track_w = match self.size {
                    crate::Size::XSmall => px(8.),
                    crate::Size::Small => px(12.),
                    crate::Size::Medium => px(16.),
                    crate::Size::Large => px(20.),
                    _ => px(16.),
                };
                let left = (track_w - tw) * 0.5;
                let mb = th * -0.5;
                (tw, th, left, start, px(0.), mb)
            };

        div()
            .id(id)
            .absolute()
            .when(axis.is_horizontal(), |this| {
                this.top(top_offset).left(left_offset).ml(margin_left)
            })
            .when(axis.is_vertical(), |this| {
                this.bottom(left_offset).left(top_offset).mb(margin_bottom)
            })
            .w(thumb_width)
            .h(thumb_height)
            .flex_shrink_0()
            .rounded_full()
            .bg(thumb_bg)
            .when(cx.theme().shadow, |this| this.shadow_xs())
            .on_mouse_down(MouseButton::Left, |_, _, cx| {
                cx.stop_propagation();
            })
            .on_drag(DragThumb((entity_id, is_start)), |drag, _, _, cx| {
                cx.stop_propagation();
                cx.new(|_| drag.clone())
            })
            .on_drag_move(window.listener_for(
                &self.state,
                move |view, e: &DragMoveEvent<DragThumb>, window, cx| {
                    match e.drag(cx) {
                        DragThumb((id, is_start)) => {
                            if *id != entity_id {
                                return;
                            }

                            // set value by mouse position
                            view.update_value_by_position(
                                axis,
                                e.event.position,
                                *is_start,
                                window,
                                cx,
                            )
                        }
                    }
                },
            ))
    }
}

impl Styled for Slider {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for Slider {
    fn render(self, window: &mut Window, cx: &mut gpui::App) -> impl IntoElement {
        let axis = self.axis;
        let entity_id = self.state.entity_id();
        let state = self.state.read(cx);
        let is_range = state.value().is_range();
        let percentage = state.percentage.clone();
        let centered = state.centered;
        let rem_size = window.rem_size();

        let active_color: Background = self
            .style
            .background
            .clone()
            .and_then(|bg| bg.color())
            .unwrap_or(cx.theme().primary.into())
            .into();
        let inactive_color: Background = cx.theme().surface_container_highest.into();
        let thumb_bg: Background = self
            .style
            .text
            .color
            .map(Into::into)
            .unwrap_or_else(|| cx.theme().primary.into());
        let corner_radii = self.style.corner_radii.clone();
        let default_radius = px(999.);
        let mut radius = Corners {
            top_left: corner_radii
                .top_left
                .map(|v| v.to_pixels(rem_size))
                .unwrap_or(default_radius),
            top_right: corner_radii
                .top_right
                .map(|v| v.to_pixels(rem_size))
                .unwrap_or(default_radius),
            bottom_left: corner_radii
                .bottom_left
                .map(|v| v.to_pixels(rem_size))
                .unwrap_or(default_radius),
            bottom_right: corner_radii
                .bottom_right
                .map(|v| v.to_pixels(rem_size))
                .unwrap_or(default_radius),
        };
        if cx.theme().radius.is_zero() {
            radius.top_left = px(0.);
            radius.top_right = px(0.);
            radius.bottom_left = px(0.);
            radius.bottom_right = px(0.);
        }

        let track_size = match self.size {
            crate::Size::XSmall => px(8.),
            crate::Size::Small => px(12.),
            crate::Size::Medium => px(16.),
            crate::Size::Large => px(20.),
            _ => px(16.),
        };

        // Stop Dots (discrete stop ticks or start/end bounds)
        let mut dots = Vec::new();
        let step_count = if state.step > 0.0 && state.step < (state.max - state.min) {
            let count = ((state.max - state.min) / state.step).round() as usize;
            if count <= 30 { Some(count) } else { None }
        } else {
            None
        };

        if let Some(count) = step_count {
            for i in 0..=count {
                let val = (state.min + (i as f32) * state.step).min(state.max);
                let pct = state.value_to_percentage(val);
                dots.push(pct);
            }
        } else {
            dots.push(0.0);
            dots.push(1.0);
        }

        let dot_elements: Vec<_> = dots
            .into_iter()
            .map(|dot_pct| {
                let is_active_region = if centered {
                    if percentage.end < 0.5 {
                        dot_pct >= percentage.end && dot_pct <= 0.5
                    } else {
                        dot_pct >= 0.5 && dot_pct <= percentage.end
                    }
                } else if is_range {
                    dot_pct >= percentage.start && dot_pct <= percentage.end
                } else if self.reverse {
                    dot_pct >= percentage.end
                } else {
                    dot_pct <= percentage.end
                };

                let dot_color = if is_active_region {
                    cx.theme().on_primary.opacity(0.38)
                } else {
                    cx.theme().primary.opacity(0.38)
                };

                let margin = (track_size * 0.5) * (1.0 - 2.0 * dot_pct) - px(2.0);

                div()
                    .absolute()
                    .bg(dot_color)
                    .rounded_full()
                    .size(px(4.))
                    .when(axis.is_horizontal(), |this| {
                        this.left(relative(dot_pct))
                            .top((track_size - px(4.)) * 0.5)
                            .ml(margin)
                    })
                    .when(axis.is_vertical(), |this| {
                        this.bottom(relative(dot_pct))
                            .left((track_size - px(4.)) * 0.5)
                            .mb(margin)
                    })
            })
            .collect();

        // Centered zero-point tick mark
        let center_tick = if centered {
            let tick_size = track_size + px(4.);
            Some(
                div()
                    .absolute()
                    .bg(cx.theme().on_surface.opacity(0.5))
                    .when(axis.is_horizontal(), |this| {
                        this.left(relative(0.5))
                            .top((track_size - tick_size) * 0.5)
                            .w(px(2.))
                            .h(tick_size)
                            .ml(px(-1.))
                    })
                    .when(axis.is_vertical(), |this| {
                        this.bottom(relative(0.5))
                            .left((track_size - tick_size) * 0.5)
                            .h(px(2.))
                            .w(tick_size)
                            .mb(px(-1.))
                    }),
            )
        } else {
            None
        };

        struct TrackSegment {
            start: f32,
            end: f32,
            gap_start: bool,
            gap_end: bool,
            color: gpui::Background,
            round_start: bool,
            round_end: bool,
        }

        let create_segment = |start: f32, end: f32, color: gpui::Background| {
            if end - start <= 0.001 {
                return None;
            }
            let (gap_start, round_start) = if start <= 0.001 {
                (false, true)
            } else {
                (true, false)
            };
            let (gap_end, round_end) = if end >= 0.999 {
                (false, true)
            } else {
                (true, false)
            };
            Some(TrackSegment {
                start,
                end,
                gap_start,
                gap_end,
                color,
                round_start,
                round_end,
            })
        };

        let mut segments = Vec::new();

        if centered {
            let val = percentage.end;
            if val < 0.5 {
                if let Some(s) = create_segment(0.0, val, inactive_color) {
                    segments.push(s);
                }
                if let Some(s) = create_segment(val, 0.5, active_color) {
                    segments.push(s);
                }
                if let Some(s) = create_segment(0.5, 1.0, inactive_color) {
                    segments.push(s);
                }
            } else {
                if let Some(s) = create_segment(0.0, 0.5, inactive_color) {
                    segments.push(s);
                }
                if let Some(s) = create_segment(0.5, val, active_color) {
                    segments.push(s);
                }
                if let Some(s) = create_segment(val, 1.0, inactive_color) {
                    segments.push(s);
                }
            }
        } else if is_range {
            let start = percentage.start;
            let end = percentage.end;
            if let Some(s) = create_segment(0.0, start, inactive_color) {
                segments.push(s);
            }
            if let Some(s) = create_segment(start, end, active_color) {
                segments.push(s);
            }
            if let Some(s) = create_segment(end, 1.0, inactive_color) {
                segments.push(s);
            }
        } else if self.reverse {
            let val = percentage.end;
            if let Some(s) = create_segment(0.0, val, inactive_color) {
                segments.push(s);
            }
            if let Some(s) = create_segment(val, 1.0, active_color) {
                segments.push(s);
            }
        } else {
            let val = percentage.end;
            if let Some(s) = create_segment(0.0, val, active_color) {
                segments.push(s);
            }
            if let Some(s) = create_segment(val, 1.0, inactive_color) {
                segments.push(s);
            }
        }

        let is_dragging = state.dragging && state.dragging_thumb.is_some();
        let gap_margin = if is_dragging { px(8.0) } else { px(6.0) };
        let inner_radius = if cx.theme().radius.is_zero() {
            px(0.0)
        } else {
            track_size * 0.25
        };

        let rendered_segments: Vec<_> = segments
            .into_iter()
            .map(|segment| {
                let margin_start = if segment.gap_start {
                    gap_margin
                } else {
                    px(0.0)
                };
                let margin_end = if segment.gap_end { gap_margin } else { px(0.0) };

                let segment_radius = if axis.is_horizontal() {
                    Corners {
                        top_left: if segment.round_start {
                            radius.top_left
                        } else {
                            inner_radius
                        },
                        bottom_left: if segment.round_start {
                            radius.bottom_left
                        } else {
                            inner_radius
                        },
                        top_right: if segment.round_end {
                            radius.top_right
                        } else {
                            inner_radius
                        },
                        bottom_right: if segment.round_end {
                            radius.bottom_right
                        } else {
                            inner_radius
                        },
                    }
                } else {
                    Corners {
                        bottom_left: if segment.round_start {
                            radius.bottom_left
                        } else {
                            inner_radius
                        },
                        bottom_right: if segment.round_start {
                            radius.bottom_right
                        } else {
                            inner_radius
                        },
                        top_left: if segment.round_end {
                            radius.top_left
                        } else {
                            inner_radius
                        },
                        top_right: if segment.round_end {
                            radius.top_right
                        } else {
                            inner_radius
                        },
                    }
                };

                div()
                    .absolute()
                    .bg(segment.color)
                    .when(axis.is_horizontal(), |this| {
                        this.h_full()
                            .left(relative(segment.start))
                            .right(relative(1.0 - segment.end))
                            .ml(margin_start)
                            .mr(margin_end)
                    })
                    .when(axis.is_vertical(), |this| {
                        this.w_full()
                            .bottom(relative(segment.start))
                            .top(relative(1.0 - segment.end))
                            .mb(margin_start)
                            .mt(margin_end)
                    })
                    .corner_radii(segment_radius)
            })
            .collect();

        let slider_min = state.min_value() as f64;
        let slider_max = state.max_value() as f64;
        let _slider_step = state.step_value() as f64;
        let slider_value = state.value().end() as f64;
        let slider_state_ref = self.state.clone();

        div()
            .id(("slider", self.state.entity_id()))
            .role(Role::Slider)
            .aria_numeric_value(slider_value)
            .aria_min_numeric_value(slider_min)
            .aria_max_numeric_value(slider_max)
            .aria_orientation(if axis.is_vertical() {
                Orientation::Vertical
            } else {
                Orientation::Horizontal
            })
            .on_a11y_action(AccessibleAction::Increment, {
                let state = slider_state_ref.clone();
                move |_, window, cx| {
                    state.update(cx, |state, cx| {
                        let new_val =
                            (state.value().end() + state.step_value()).min(state.max_value());
                        state.set_value(new_val, window, cx);
                    });
                }
            })
            .on_a11y_action(AccessibleAction::Decrement, {
                let state = slider_state_ref.clone();
                move |_, window, cx| {
                    state.update(cx, |state, cx| {
                        let new_val =
                            (state.value().end() - state.step_value()).max(state.min_value());
                        state.set_value(new_val, window, cx);
                    });
                }
            })
            .flex()
            .flex_1()
            .items_center()
            .justify_center()
            .when(axis.is_vertical(), |this| this.h(px(120.)))
            .when(axis.is_horizontal(), |this| this.w_full())
            .refine_style(&self.style)
            .bg(cx.theme().transparent)
            .text_color(cx.theme().on_surface)
            .when(!self.disabled, |this| {
                this.on_mouse_up(
                    MouseButton::Left,
                    window.listener_for(&self.state, |state, _, _, cx| {
                        state.handle_release(cx);
                    }),
                )
                .on_mouse_up_out(
                    MouseButton::Left,
                    window.listener_for(&self.state, |state, _, _, cx| {
                        state.handle_release(cx);
                    }),
                )
            })
            .child(
                h_flex()
                    .id("slider-bar-container")
                    .when(!self.disabled, |this| {
                        this.on_mouse_down(
                            MouseButton::Left,
                            window.listener_for(
                                &self.state,
                                move |state, e: &MouseDownEvent, window, cx| {
                                    let mut is_start = false;
                                    if is_range {
                                        let bar_size = state.bounds.size.along(axis);
                                        let inner_pos = if axis.is_horizontal() {
                                            e.position.x - state.bounds.left()
                                        } else {
                                            state.bounds.bottom() - e.position.y
                                        };
                                        let center = ((percentage.end - percentage.start) / 2.0
                                            + percentage.start)
                                            * bar_size;
                                        is_start = inner_pos < center;
                                    }

                                    state.update_value_by_position(
                                        axis, e.position, is_start, window, cx,
                                    )
                                },
                            ),
                        )
                    })
                    .when(!self.disabled && !is_range, |this| {
                        this.on_drag(DragSlider(entity_id), |drag, _, _, cx| {
                            cx.stop_propagation();
                            cx.new(|_| drag.clone())
                        })
                        .on_drag_move(window.listener_for(
                            &self.state,
                            move |view, e: &DragMoveEvent<DragSlider>, window, cx| match e.drag(cx)
                            {
                                DragSlider(id) => {
                                    if *id != entity_id {
                                        return;
                                    }

                                    view.update_value_by_position(
                                        axis,
                                        e.event.position,
                                        false,
                                        window,
                                        cx,
                                    )
                                }
                            },
                        ))
                    })
                    .when(axis.is_horizontal(), |this| {
                        this.items_center().h_8().w_full()
                    })
                    .when(axis.is_vertical(), |this| {
                        this.justify_center().w_8().h_full()
                    })
                    .flex_shrink_0()
                    .child(
                        div()
                            .id("slider-bar")
                            .relative()
                            .when(axis.is_horizontal(), |this| this.w_full().h(track_size))
                            .when(axis.is_vertical(), |this| this.h_full().w(track_size))
                            .children(rendered_segments)
                            .children(center_tick)
                            .children(dot_elements)
                            .when(is_range, |this| {
                                this.child(self.render_thumb(
                                    relative(percentage.start),
                                    true,
                                    active_color.into(),
                                    thumb_bg,
                                    radius,
                                    window,
                                    cx,
                                ))
                            })
                            .child(self.render_thumb(
                                relative(percentage.end),
                                false,
                                active_color.into(),
                                thumb_bg,
                                radius,
                                window,
                                cx,
                            ))
                            .on_prepaint({
                                let state = self.state.clone();
                                move |bounds, _, cx| state.update(cx, |r, _| r.bounds = bounds)
                            }),
                    ),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slider_state_new() {
        let state = SliderState::new();
        assert_eq!(state.min_value(), 0.0);
        assert_eq!(state.max_value(), 100.0);
        assert_eq!(state.step_value(), 1.0);
        assert!(!state.is_centered());
    }

    #[test]
    fn test_slider_state_centered() {
        let state = SliderState::new().centered(true);
        assert!(state.is_centered());
    }

    #[test]
    fn test_slider_state_snapping() {
        let state = SliderState::new().min(0.0).max(5.0).step(1.0);
        let val = state.percentage_to_value(0.3);
        let snapped_val = (val / 1.0).round() * 1.0;
        let pct = state.value_to_percentage(snapped_val);
        assert_eq!(snapped_val, 2.0);
        assert_eq!(pct, 0.4);
    }

    #[test]
    fn test_slider_state_keyboard_step() {
        let state = SliderState::new().min(0.0).max(100.0).step(5.0);
        assert_eq!(state.value(), SliderValue::Single(0.0));
        assert_eq!(state.step_value(), 5.0);
    }
}
