#![allow(dead_code)]

use gpui::{Corners, Edges, Pixels};

#[cfg(test)]
use std::cell::RefCell;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum CornerToken {
    Full,
    Fixed(Pixels),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct CornerShape {
    pub top_left: CornerToken,
    pub top_right: CornerToken,
    pub bottom_right: CornerToken,
    pub bottom_left: CornerToken,
}

impl CornerShape {
    pub(crate) fn all(token: CornerToken) -> Self {
        Self {
            top_left: token,
            top_right: token,
            bottom_right: token,
            bottom_left: token,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct StateShape {
    pub resting: CornerShape,
    pub pressed: CornerShape,
    pub checked: CornerShape,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ButtonSlotGeometry {
    pub height: Pixels,
    pub min_width: Pixels,
    pub padding_start: Pixels,
    pub padding_end: Pixels,
    pub padding_top: Pixels,
    pub padding_bottom: Pixels,
    pub corners: CornerShape,
    pub border_edges: Edges<bool>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ResolvedButtonGeometry {
    pub height: Pixels,
    pub min_width: Pixels,
    pub padding_start: Pixels,
    pub padding_end: Pixels,
    pub padding_top: Pixels,
    pub padding_bottom: Pixels,
    pub corners: Corners<Pixels>,
    pub border_edges: Edges<bool>,
}

#[cfg(test)]
thread_local! {
    static CAPTURED_RENDER_GEOMETRY: RefCell<Option<ResolvedButtonGeometry>> =
        const { RefCell::new(None) };
}

#[cfg(test)]
pub(crate) struct RenderGeometryCapture {
    previous: Option<ResolvedButtonGeometry>,
}

#[cfg(test)]
impl Drop for RenderGeometryCapture {
    fn drop(&mut self) {
        CAPTURED_RENDER_GEOMETRY.with(|capture| {
            *capture.borrow_mut() = self.previous;
        });
    }
}

#[cfg(test)]
pub(crate) fn capture_render_geometry() -> RenderGeometryCapture {
    let previous = CAPTURED_RENDER_GEOMETRY.with(|capture| capture.borrow_mut().take());
    RenderGeometryCapture { previous }
}

#[cfg(test)]
pub(crate) fn captured_render_geometry() -> Option<ResolvedButtonGeometry> {
    CAPTURED_RENDER_GEOMETRY.with(|capture| *capture.borrow())
}

#[cfg(test)]
pub(crate) fn record_render_geometry(geometry: ResolvedButtonGeometry) {
    CAPTURED_RENDER_GEOMETRY.with(|capture| *capture.borrow_mut() = Some(geometry));
}

pub(crate) fn resolve(shape: CornerShape, height: Pixels) -> Corners<Pixels> {
    let resolve = |token| match token {
        CornerToken::Full => height * 0.5,
        CornerToken::Fixed(value) => value,
    };
    Corners {
        top_left: resolve(shape.top_left),
        top_right: resolve(shape.top_right),
        bottom_right: resolve(shape.bottom_right),
        bottom_left: resolve(shape.bottom_left),
    }
}

pub(crate) fn resolve_slot(slot: ButtonSlotGeometry) -> ResolvedButtonGeometry {
    ResolvedButtonGeometry {
        height: slot.height,
        min_width: slot.min_width,
        padding_start: slot.padding_start,
        padding_end: slot.padding_end,
        padding_top: slot.padding_top,
        padding_bottom: slot.padding_bottom,
        corners: resolve(slot.corners, slot.height),
        border_edges: slot.border_edges,
    }
}

pub(crate) fn assemble(
    height: Pixels,
    min_width: Pixels,
    padding: Edges<Pixels>,
    corners: CornerShape,
    border_edges: Edges<bool>,
    slot: Option<ButtonSlotGeometry>,
) -> ResolvedButtonGeometry {
    let base = ButtonSlotGeometry {
        height,
        min_width,
        padding_start: padding.left,
        padding_end: padding.right,
        padding_top: padding.top,
        padding_bottom: padding.bottom,
        corners,
        border_edges,
    };
    resolve_slot(slot.unwrap_or(base))
}

pub(crate) fn full() -> CornerToken {
    CornerToken::Full
}
pub(crate) fn fixed(value: Pixels) -> CornerToken {
    CornerToken::Fixed(value)
}

#[cfg(test)]
pub(crate) fn renderer_probe(
    height: Pixels,
    min_width: Pixels,
    padding: Edges<Pixels>,
    corners: CornerShape,
    border_edges: Edges<bool>,
    slot: Option<ButtonSlotGeometry>,
) -> ResolvedButtonGeometry {
    assemble(height, min_width, padding, corners, border_edges, slot)
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::px;

    #[test]
    fn full_uses_terminal_height() {
        assert_eq!(resolve(CornerShape::all(full()), px(40.)).top_left, px(20.));
        assert_eq!(resolve(CornerShape::all(full()), px(56.)).top_left, px(28.));
    }

    #[test]
    fn renderer_and_probe_share_resolved_geometry_source() {
        let shape = CornerShape {
            top_left: full(),
            top_right: fixed(px(4.)),
            bottom_right: fixed(px(8.)),
            bottom_left: full(),
        };
        let padding = Edges {
            left: px(1.),
            right: px(2.),
            top: px(3.),
            bottom: px(4.),
        };
        let edges = Edges {
            left: true,
            right: false,
            top: true,
            bottom: false,
        };
        let production = assemble(px(72.), px(80.), padding, shape, edges, None);
        assert_eq!(
            production,
            renderer_probe(px(72.), px(80.), padding, shape, edges, None)
        );
    }

    #[test]
    fn explicit_corners_and_slot_values_preserve_geometry() {
        let shape = CornerShape {
            top_left: fixed(px(1.)),
            top_right: fixed(px(2.)),
            bottom_right: fixed(px(3.)),
            bottom_left: fixed(px(4.)),
        };
        assert_eq!(
            resolve(shape, px(80.)),
            Corners {
                top_left: px(1.),
                top_right: px(2.),
                bottom_right: px(3.),
                bottom_left: px(4.)
            }
        );
        let slot = ButtonSlotGeometry {
            height: px(64.),
            min_width: px(48.),
            padding_start: px(12.),
            padding_end: px(16.),
            padding_top: px(4.),
            padding_bottom: px(6.),
            corners: CornerShape::all(full()),
            border_edges: Edges::all(true),
        };
        let resolved = resolve_slot(slot);
        assert_eq!(
            (
                resolved.height,
                resolved.min_width,
                resolved.corners.top_left
            ),
            (px(64.), px(48.), px(32.))
        );
    }

    #[test]
    fn state_shapes_are_static_metadata() {
        let shape = CornerShape::all(fixed(px(8.)));
        let states = StateShape {
            resting: shape,
            pressed: shape,
            checked: shape,
        };
        assert_eq!(states.resting, states.pressed);
        assert_eq!(states.resting, states.checked);
    }
}
