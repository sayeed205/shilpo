use gpui::layer_shell::Anchor;
use gpui::{Bounds, DisplayId, Pixels, point, px, size};
use shilpo_config::{BarConfig, BarPosition, BarStyle};

#[derive(Clone, Debug, PartialEq)]
pub struct BarGeometry {
    pub bounds: Bounds<Pixels>,
    pub display_id: DisplayId,
    pub anchor: Anchor,
    pub exclusive_zone: Pixels,
    pub exclusive_edge: Anchor,
    pub margin: Option<(Pixels, Pixels, Pixels, Pixels)>,
}

impl BarGeometry {
    pub fn calculate(
        display_id: DisplayId,
        display_bounds: Bounds<Pixels>,
        config: &BarConfig,
    ) -> Self {
        let thickness = px(config.height as f32);
        let (horizontal_margin, vertical_margin) = if config.style == BarStyle::FloatingCapsule {
            (
                px(config.margin.horizontal as f32),
                px(config.margin.vertical as f32),
            )
        } else {
            (Pixels::ZERO, Pixels::ZERO)
        };

        let (anchor, exclusive_edge, bounds, exclusive_zone, margin) = match config.position {
            BarPosition::Top => (
                Anchor::TOP | Anchor::LEFT | Anchor::RIGHT,
                Anchor::TOP,
                Bounds::new(
                    point(display_bounds.origin.x, display_bounds.origin.y),
                    size(display_bounds.size.width, thickness + vertical_margin * 2.0),
                ),
                thickness + vertical_margin * 2.0,
                (
                    vertical_margin,
                    horizontal_margin,
                    Pixels::ZERO,
                    horizontal_margin,
                ),
            ),
            BarPosition::Bottom => (
                Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT,
                Anchor::BOTTOM,
                Bounds::new(
                    point(
                        display_bounds.origin.x,
                        display_bounds.origin.y + display_bounds.size.height
                            - thickness
                            - vertical_margin * 2.0,
                    ),
                    size(display_bounds.size.width, thickness + vertical_margin * 2.0),
                ),
                thickness + vertical_margin * 2.0,
                (
                    Pixels::ZERO,
                    horizontal_margin,
                    vertical_margin,
                    horizontal_margin,
                ),
            ),
            BarPosition::Left => (
                Anchor::LEFT | Anchor::TOP | Anchor::BOTTOM,
                Anchor::LEFT,
                Bounds::new(
                    point(display_bounds.origin.x, display_bounds.origin.y),
                    size(
                        thickness + horizontal_margin * 2.0,
                        display_bounds.size.height,
                    ),
                ),
                thickness + horizontal_margin * 2.0,
                (
                    horizontal_margin,
                    Pixels::ZERO,
                    horizontal_margin,
                    vertical_margin,
                ),
            ),
            BarPosition::Right => (
                Anchor::RIGHT | Anchor::TOP | Anchor::BOTTOM,
                Anchor::RIGHT,
                Bounds::new(
                    point(
                        display_bounds.origin.x + display_bounds.size.width
                            - thickness
                            - horizontal_margin * 2.0,
                        display_bounds.origin.y,
                    ),
                    size(
                        thickness + horizontal_margin * 2.0,
                        display_bounds.size.height,
                    ),
                ),
                thickness + horizontal_margin * 2.0,
                (
                    horizontal_margin,
                    vertical_margin,
                    horizontal_margin,
                    Pixels::ZERO,
                ),
            ),
        };

        Self {
            bounds,
            display_id,
            anchor,
            exclusive_zone,
            exclusive_edge,
            margin: (config.style == BarStyle::FloatingCapsule).then_some(margin),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shilpo_config::BarMargin;

    fn config(position: BarPosition, style: BarStyle) -> BarConfig {
        BarConfig {
            position,
            style,
            height: 40,
            margin: BarMargin {
                horizontal: 10,
                vertical: 6,
            },
            ..BarConfig::default()
        }
    }

    #[test]
    fn maps_all_positions_with_display_origin_and_margins() {
        let display = Bounds::new(point(px(100.), px(50.)), size(px(1600.), px(900.)));
        let id = DisplayId::new(7);
        let expected = [
            (
                BarPosition::Top,
                point(px(100.), px(50.)),
                size(px(1600.), px(52.)),
                Anchor::TOP | Anchor::LEFT | Anchor::RIGHT,
                Anchor::TOP,
            ),
            (
                BarPosition::Bottom,
                point(px(100.), px(898.)),
                size(px(1600.), px(52.)),
                Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT,
                Anchor::BOTTOM,
            ),
            (
                BarPosition::Left,
                point(px(100.), px(50.)),
                size(px(60.), px(900.)),
                Anchor::LEFT | Anchor::TOP | Anchor::BOTTOM,
                Anchor::LEFT,
            ),
            (
                BarPosition::Right,
                point(px(1640.), px(50.)),
                size(px(60.), px(900.)),
                Anchor::RIGHT | Anchor::TOP | Anchor::BOTTOM,
                Anchor::RIGHT,
            ),
        ];

        for (position, origin, size, anchor, edge) in expected {
            let geometry =
                BarGeometry::calculate(id, display, &config(position, BarStyle::FloatingCapsule));
            assert_eq!(geometry.display_id, id);
            assert_eq!(geometry.bounds.origin, origin);
            assert_eq!(geometry.bounds.size, size);
            assert_eq!(geometry.anchor, anchor);
            assert_eq!(geometry.exclusive_edge, edge);
            assert_eq!(
                geometry.exclusive_zone,
                px(
                    if matches!(position, BarPosition::Left | BarPosition::Right) {
                        60.
                    } else {
                        52.
                    }
                )
            );
        }
    }

    #[test]
    fn full_edge_uses_thickness_and_no_margin() {
        let display = Bounds::new(point(px(100.), px(50.)), size(px(1600.), px(900.)));
        let geometry = BarGeometry::calculate(
            DisplayId::new(1),
            display,
            &config(BarPosition::Bottom, BarStyle::FullEdge),
        );
        assert_eq!(geometry.bounds.origin, point(px(100.), px(910.)));
        assert_eq!(geometry.bounds.size, size(px(1600.), px(40.)));
        assert_eq!(geometry.exclusive_zone, px(40.));
        assert_eq!(geometry.margin, None);
    }

    #[test]
    fn floating_margin_tuple_follows_layer_shell_order() {
        let display = Bounds::new(point(px(0.), px(0.)), size(px(100.), px(100.)));
        let geometry = BarGeometry::calculate(
            DisplayId::new(1),
            display,
            &config(BarPosition::Right, BarStyle::FloatingCapsule),
        );
        assert_eq!(geometry.margin, Some((px(10.), px(6.), px(10.), px(0.))));
    }
}
