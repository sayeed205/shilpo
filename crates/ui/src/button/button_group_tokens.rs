use gpui::{Axis, Corners, Pixels, px};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ButtonGroupMode {
    Standard,
    #[default]
    Connected,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ButtonGroupTokens {
    pub spacing: Pixels,
    pub height: Pixels,
    pub outer_radius: Pixels,
    pub inner_radius: Pixels,
    pub pressed_inner_radius: Pixels,
    pub selected_inner_radius: Pixels,
}

pub(crate) const fn tokens(mode: ButtonGroupMode) -> ButtonGroupTokens {
    let spacing = match mode {
        ButtonGroupMode::Standard => px(12.),
        ButtonGroupMode::Connected => px(2.),
    };
    ButtonGroupTokens {
        spacing,
        height: px(40.),
        outer_radius: px(20.),
        inner_radius: px(8.),
        pressed_inner_radius: px(4.),
        selected_inner_radius: px(4.),
    }
}

pub(crate) fn corner_radii(
    mode: ButtonGroupMode,
    axis: Axis,
    index: usize,
    count: usize,
    selected: bool,
) -> Corners<Pixels> {
    let token = tokens(mode);
    if selected {
        return Corners {
            top_left: token.outer_radius,
            top_right: token.outer_radius,
            bottom_left: token.outer_radius,
            bottom_right: token.outer_radius,
        };
    }
    if mode == ButtonGroupMode::Standard || count <= 1 {
        return Corners {
            top_left: token.outer_radius,
            top_right: token.outer_radius,
            bottom_left: token.outer_radius,
            bottom_right: token.outer_radius,
        };
    }
    let first = index == 0;
    let last = index + 1 == count;
    let inner = token.inner_radius;
    let outer = token.outer_radius;
    match axis {
        Axis::Horizontal => Corners {
            top_left: if first { outer } else { inner },
            bottom_left: if first { outer } else { inner },
            top_right: if last { outer } else { inner },
            bottom_right: if last { outer } else { inner },
        },
        Axis::Vertical => Corners {
            top_left: if first { outer } else { inner },
            top_right: if first { outer } else { inner },
            bottom_left: if last { outer } else { inner },
            bottom_right: if last { outer } else { inner },
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn material_group_geometry_is_static() {
        let standard = tokens(ButtonGroupMode::Standard);
        let connected = tokens(ButtonGroupMode::Connected);
        assert_eq!(standard.spacing, px(12.));
        assert_eq!(connected.spacing, px(2.));
        assert_eq!(standard.height, px(40.));
        assert_eq!(connected.outer_radius, px(20.));
        assert_eq!(connected.inner_radius, px(8.));
        assert_eq!(connected.pressed_inner_radius, px(4.));
        assert_eq!(connected.selected_inner_radius, px(4.));
    }

    #[test]
    fn connected_radii_follow_position_and_axis() {
        let first = corner_radii(ButtonGroupMode::Connected, Axis::Horizontal, 0, 3, false);
        let middle = corner_radii(ButtonGroupMode::Connected, Axis::Horizontal, 1, 3, false);
        let last = corner_radii(ButtonGroupMode::Connected, Axis::Vertical, 2, 3, false);
        let sel = corner_radii(ButtonGroupMode::Connected, Axis::Horizontal, 1, 3, true);
        assert_eq!(first.top_left, px(20.));
        assert_eq!(first.top_right, px(8.));
        assert_eq!(middle.top_left, px(8.));
        assert_eq!(last.bottom_left, px(20.));
        assert_eq!(sel.top_left, px(20.));
        assert_eq!(sel.top_right, px(20.));
    }
}
