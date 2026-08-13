use super::geometry::BarGeometry;
use crate::config::{BarConfig, ShellConfig};
use gpui::{Bounds, DisplayId, Pixels};
use std::collections::HashMap;

/// Metadata describing a physical monitor output.
#[derive(Debug, Clone, PartialEq)]
pub struct OutputDescriptor {
    pub display_id: DisplayId,
    pub bounds: Bounds<Pixels>,
    pub is_primary: bool,
    pub name: Option<String>,
    pub scale: Option<f32>,
}

/// Target operational specification for a single bar instance.
#[derive(Debug, Clone, PartialEq)]
pub struct BarSpec {
    pub display_id: DisplayId,
    pub output_name: Option<String>,
    pub geometry: BarGeometry,
    pub config: BarConfig,
    pub with_display_geometry: bool,
}

impl BarSpec {
    pub fn new(geometry: BarGeometry, config: BarConfig, with_display_geometry: bool) -> Self {
        Self {
            display_id: geometry.display_id,
            output_name: None,
            geometry,
            config,
            with_display_geometry,
        }
    }
}

/// Reconciliation operation to transition current bar instances to desired state.
#[derive(Debug, Clone, PartialEq)]
pub enum ReconciliationOp {
    Create(BarSpec),
    Retain(BarSpec),
    Recreate(BarSpec),
    Remove(DisplayId),
}

/// Computes the minimal set of operations to align existing bar instances with active monitor outputs and configuration.
pub fn reconcile_output_bars(
    current_outputs: &[OutputDescriptor],
    config: &ShellConfig,
    current_bars: &HashMap<DisplayId, BarSpec>,
) -> Vec<ReconciliationOp> {
    let mut ops = Vec::new();
    let mut processed_displays = std::collections::HashSet::new();

    for output in current_outputs {
        processed_displays.insert(output.display_id);

        let resolved_config = config.bar_for_output(output.name.as_deref(), output.is_primary);

        match (resolved_config, current_bars.get(&output.display_id)) {
            (None, Some(_)) => {
                ops.push(ReconciliationOp::Remove(output.display_id));
            }
            (None, None) => {}
            (Some(bar_config), existing_spec) => {
                let geometry = BarGeometry::calculate_with_scale(
                    output.display_id,
                    output.bounds,
                    &bar_config,
                    output.scale,
                );
                let desired_spec = BarSpec {
                    display_id: output.display_id,
                    output_name: output.name.clone(),
                    geometry,
                    config: bar_config,
                    with_display_geometry: true,
                };

                match existing_spec {
                    None => ops.push(ReconciliationOp::Create(desired_spec)),
                    Some(existing) if existing == &desired_spec => {
                        ops.push(ReconciliationOp::Retain(desired_spec));
                    }
                    Some(_) => ops.push(ReconciliationOp::Recreate(desired_spec)),
                }
            }
        }
    }

    for &display_id in current_bars.keys() {
        if !processed_displays.contains(&display_id) {
            ops.push(ReconciliationOp::Remove(display_id));
        }
    }

    ops
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{OutputConfig, ShellConfig};
    use gpui::{point, px, size};

    fn sample_bounds(w: f32, h: f32) -> Bounds<Pixels> {
        Bounds::new(point(px(0.), px(0.)), size(px(w), px(h)))
    }

    #[test]
    fn reconciliation_create_initial_bar() {
        let outputs = vec![OutputDescriptor {
            display_id: DisplayId::new(1),
            bounds: sample_bounds(1920., 1080.),
            is_primary: true,
            name: Some("eDP-1".into()),
            scale: None,
        }];
        let config = ShellConfig::default();
        let current_bars = HashMap::new();

        let ops = reconcile_output_bars(&outputs, &config, &current_bars);
        assert_eq!(ops.len(), 1);
        assert!(
            matches!(&ops[0], ReconciliationOp::Create(spec) if spec.display_id == DisplayId::new(1)
                && spec.output_name.as_deref() == Some("eDP-1"))
        );
    }

    #[test]
    fn reconciliation_retain_unchanged_bar() {
        let outputs = vec![OutputDescriptor {
            display_id: DisplayId::new(1),
            bounds: sample_bounds(1920., 1080.),
            is_primary: true,
            name: Some("eDP-1".into()),
            scale: None,
        }];
        let config = ShellConfig::default();
        let bar_config = config.bar_for_output(Some("eDP-1"), true).unwrap();
        let geometry =
            BarGeometry::calculate(DisplayId::new(1), sample_bounds(1920., 1080.), &bar_config);
        let spec = BarSpec {
            display_id: DisplayId::new(1),
            output_name: Some("eDP-1".into()),
            geometry,
            config: bar_config,
            with_display_geometry: true,
        };
        let mut current_bars = HashMap::new();
        current_bars.insert(DisplayId::new(1), spec.clone());

        let ops = reconcile_output_bars(&outputs, &config, &current_bars);
        assert_eq!(ops, vec![ReconciliationOp::Retain(spec)]);
    }

    #[test]
    fn reconciliation_recreate_on_geometry_change() {
        let outputs = vec![OutputDescriptor {
            display_id: DisplayId::new(1),
            bounds: sample_bounds(2560., 1440.),
            is_primary: true,
            name: Some("eDP-1".into()),
            scale: None,
        }];
        let config = ShellConfig::default();
        let old_bar_config = config.bar_for_output(Some("eDP-1"), true).unwrap();
        let old_geometry = BarGeometry::calculate(
            DisplayId::new(1),
            sample_bounds(1920., 1080.),
            &old_bar_config,
        );
        let old_spec = BarSpec {
            display_id: DisplayId::new(1),
            output_name: Some("eDP-1".into()),
            geometry: old_geometry,
            config: old_bar_config,
            with_display_geometry: true,
        };
        let mut current_bars = HashMap::new();
        current_bars.insert(DisplayId::new(1), old_spec);

        let ops = reconcile_output_bars(&outputs, &config, &current_bars);
        assert_eq!(ops.len(), 1);
        assert!(
            matches!(&ops[0], ReconciliationOp::Recreate(spec) if spec.geometry.bounds.size.width == px(2560.))
        );
    }

    #[test]
    fn reconciliation_remove_disappeared_display() {
        let outputs = vec![];
        let config = ShellConfig::default();
        let bar_config = config.bar_for_output(None, true).unwrap();
        let geometry =
            BarGeometry::calculate(DisplayId::new(1), sample_bounds(1920., 1080.), &bar_config);
        let spec = BarSpec {
            display_id: DisplayId::new(1),
            output_name: None,
            geometry,
            config: bar_config,
            with_display_geometry: true,
        };
        let mut current_bars = HashMap::new();
        current_bars.insert(DisplayId::new(1), spec);

        let ops = reconcile_output_bars(&outputs, &config, &current_bars);
        assert_eq!(ops, vec![ReconciliationOp::Remove(DisplayId::new(1))]);
    }

    #[test]
    fn reconciliation_remove_disabled_output() {
        let outputs = vec![OutputDescriptor {
            display_id: DisplayId::new(2),
            bounds: sample_bounds(1920., 1080.),
            is_primary: false,
            name: Some("HDMI-A-1".into()),
            scale: None,
        }];
        let mut config = ShellConfig::default();
        config.outputs.insert(
            "HDMI-A-1".to_string(),
            OutputConfig {
                enabled: false,
                scale: None,
                position: None,
                style: None,
                height: None,
                padding: None,
                margin: None,
                widget_spacing: None,
                opacity: None,
                exclusive_zone: None,
                widgets: None,
            },
        );

        let bar_config = ShellConfig::default().bar;
        let geometry =
            BarGeometry::calculate(DisplayId::new(2), sample_bounds(1920., 1080.), &bar_config);
        let spec = BarSpec {
            display_id: DisplayId::new(2),
            output_name: Some("HDMI-A-1".into()),
            geometry,
            config: bar_config,
            with_display_geometry: true,
        };
        let mut current_bars = HashMap::new();
        current_bars.insert(DisplayId::new(2), spec);

        let ops = reconcile_output_bars(&outputs, &config, &current_bars);
        assert_eq!(ops, vec![ReconciliationOp::Remove(DisplayId::new(2))]);
    }

    #[test]
    fn reconciliation_scale_factor_adjustment() {
        let outputs = vec![OutputDescriptor {
            display_id: DisplayId::new(1),
            bounds: sample_bounds(3840., 2160.),
            is_primary: true,
            name: Some("DP-1".into()),
            scale: Some(2.0),
        }];
        let mut config = ShellConfig::default();
        config.bar.style = crate::config::BarStyle::Float;
        let current_bars = HashMap::new();

        let ops = reconcile_output_bars(&outputs, &config, &current_bars);
        assert_eq!(ops.len(), 1);
        if let ReconciliationOp::Create(spec) = &ops[0] {
            assert_eq!(spec.geometry.bounds.size.height, px(128.0));
        } else {
            panic!("expected ReconciliationOp::Create");
        }
    }
}
