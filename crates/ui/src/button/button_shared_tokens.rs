use gpui::{Pixels, px};

pub(crate) const STATE_HOVER: f32 = 0.08;
pub(crate) const STATE_FOCUS: f32 = 0.10;
pub(crate) const STATE_PRESSED: f32 = 0.10;
pub(crate) const STATE_DRAGGED: f32 = 0.16;
pub(crate) const COMMON_MIN_WIDTH: Pixels = px(58.);
pub(crate) const DISABLED_CONTAINER_OPACITY: f32 = 0.12;
pub(crate) const DISABLED_CONTENT_OPACITY: f32 = 0.38;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct StateOpacities {
    pub hover: f32,
    pub focus: f32,
    pub pressed: f32,
    pub dragged: f32,
}

pub(crate) const STATE_OPACITIES: StateOpacities = StateOpacities {
    hover: STATE_HOVER,
    focus: STATE_FOCUS,
    pressed: STATE_PRESSED,
    dragged: STATE_DRAGGED,
};
