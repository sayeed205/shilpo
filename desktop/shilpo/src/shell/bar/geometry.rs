use gpui::{Bounds, DisplayId, Pixels, layer_shell::Anchor, point, px, size};

use crate::config::{BarConfig, BarPosition, BarStyle};

pub(crate) const HUG_CORNER_RADIUS: f32 = 32.0;

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
        Self::calculate_with_scale(display_id, display_bounds, config, None)
    }

    pub fn calculate_with_scale(
        display_id: DisplayId,
        display_bounds: Bounds<Pixels>,
        config: &BarConfig,
        scale: Option<f32>,
    ) -> Self {
        let scale_factor = scale.unwrap_or(1.0).max(0.5);
        let thickness = px(config.height as f32 * scale_factor);
        let (horizontal_margin, vertical_margin) = if config.style == BarStyle::Float {
            (
                px(config.margin.horizontal as f32 * scale_factor),
                px(config.margin.vertical as f32 * scale_factor),
            )
        } else {
            (Pixels::ZERO, Pixels::ZERO)
        };

        let hug_extra = if config.style == BarStyle::Hug {
            px(HUG_CORNER_RADIUS * scale_factor)
        } else {
            Pixels::ZERO
        };

        let calculated_exclusive_zone = if let Some(zone) = config.exclusive_zone {
            px(zone as f32 * scale_factor)
        } else if config.style == BarStyle::Float {
            if matches!(config.position, BarPosition::Left | BarPosition::Right) {
                thickness + horizontal_margin
            } else {
                thickness + vertical_margin
            }
        } else {
            thickness
        };

        let (anchor, exclusive_edge, bounds, _, margin) = match config.position {
            BarPosition::Top => (
                Anchor::TOP | Anchor::LEFT | Anchor::RIGHT,
                Anchor::TOP,
                Bounds::new(
                    point(display_bounds.origin.x, display_bounds.origin.y),
                    size(
                        display_bounds.size.width,
                        thickness + vertical_margin * 2.0 + hug_extra,
                    ),
                ),
                calculated_exclusive_zone,
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
                            - vertical_margin * 2.0
                            - hug_extra,
                    ),
                    size(
                        display_bounds.size.width,
                        thickness + vertical_margin * 2.0 + hug_extra,
                    ),
                ),
                calculated_exclusive_zone,
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
                        thickness + horizontal_margin * 2.0 + hug_extra,
                        display_bounds.size.height,
                    ),
                ),
                calculated_exclusive_zone,
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
                            - horizontal_margin * 2.0
                            - hug_extra,
                        display_bounds.origin.y,
                    ),
                    size(
                        thickness + horizontal_margin * 2.0 + hug_extra,
                        display_bounds.size.height,
                    ),
                ),
                calculated_exclusive_zone,
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
            exclusive_zone: calculated_exclusive_zone,
            exclusive_edge,
            margin: (config.style == BarStyle::Float).then_some(margin),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::BarMargin;

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
            let geometry = BarGeometry::calculate(id, display, &config(position, BarStyle::Float));
            assert_eq!(geometry.display_id, id);
            assert_eq!(geometry.bounds.origin, origin);
            assert_eq!(geometry.bounds.size, size);
            assert_eq!(geometry.anchor, anchor);
            assert_eq!(geometry.exclusive_edge, edge);
            assert_eq!(
                geometry.exclusive_zone,
                px(
                    if matches!(position, BarPosition::Left | BarPosition::Right) {
                        50.
                    } else {
                        46.
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
            &config(BarPosition::Bottom, BarStyle::Rect),
        );
        assert_eq!(geometry.bounds.origin, point(px(100.), px(910.)));
        assert_eq!(geometry.bounds.size, size(px(1600.), px(40.)));
        assert_eq!(geometry.exclusive_zone, px(40.));
        assert_eq!(geometry.margin, None);
    }

    #[test]
    fn hug_reserves_a_thirty_two_pixel_shoulder() {
        let display = Bounds::new(point(px(0.), px(0.)), size(px(1920.), px(1080.)));
        let geometry = BarGeometry::calculate(
            DisplayId::new(1),
            display,
            &config(BarPosition::Top, BarStyle::Hug),
        );

        assert_eq!(geometry.bounds.size, size(px(1920.), px(72.)));
        assert_eq!(geometry.exclusive_zone, px(40.));
    }

    #[test]
    fn floating_margin_tuple_follows_layer_shell_order() {
        let display = Bounds::new(point(px(0.), px(0.)), size(px(100.), px(100.)));
        let geometry = BarGeometry::calculate(
            DisplayId::new(1),
            display,
            &config(BarPosition::Right, BarStyle::Float),
        );
        assert_eq!(geometry.margin, Some((px(10.), px(6.), px(10.), px(0.))));
    }

    #[test]
    fn explicit_exclusive_zone_overrides_calculated_default() {
        let display = Bounds::new(point(px(0.), px(0.)), size(px(1920.), px(1080.)));
        let mut cfg = config(BarPosition::Top, BarStyle::Float);
        cfg.exclusive_zone = Some(48);
        let geometry = BarGeometry::calculate(DisplayId::new(1), display, &cfg);
        assert_eq!(geometry.exclusive_zone, px(48.));
    }

    #[test]
    fn test_multi_monitor_bar_geometry_calculations() {
        let mon_1 = Bounds::new(point(px(0.), px(0.)), size(px(1920.), px(1080.)));
        let mon_2 = Bounds::new(point(px(1920.), px(0.)), size(px(3840.), px(2160.)));

        let geom1 = BarGeometry::calculate(
            DisplayId::new(1),
            mon_1,
            &config(BarPosition::Top, BarStyle::Rect),
        );
        let geom2 = BarGeometry::calculate_with_scale(
            DisplayId::new(2),
            mon_2,
            &config(BarPosition::Top, BarStyle::Rect),
            Some(2.0),
        );

        assert_eq!(geom1.bounds.origin, point(px(0.), px(0.)));
        assert_eq!(geom1.bounds.size.width, px(1920.));

        assert_eq!(geom2.bounds.origin, point(px(1920.), px(0.)));
        assert_eq!(geom2.bounds.size.width, px(3840.));
    }
}
