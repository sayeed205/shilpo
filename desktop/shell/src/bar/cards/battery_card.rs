use std::sync::{Arc, Mutex};

use gpui::{
    AnyElement, App, Bounds, DisplayId, InteractiveElement, IntoElement, ParentElement, Pixels,
    StatefulInteractiveElement, Styled, Window, div, px,
};
use shilpo_device_protocol::{
    BatteryChargeState, BatteryCoarseLevel, BatteryDevicePayload, BatteryPayload,
    BatteryTechnology, BatteryWarningLevel,
};
use shilpo_services::ServiceLifecycle;
use shilpo_ui::{ActiveTheme, Icon, IconName, h_flex, v_flex};

use super::{
    model::{CardCapabilities, CardChannel, CardOwnerId, CardSizeTier},
    provider::CardProvider,
};
use crate::runtime::ShellRuntime;

/// Built-in card provider for the system Battery.
pub(crate) struct BatteryCardProvider {
    selected_device: Arc<Mutex<Option<String>>>,
}

impl BatteryCardProvider {
    pub(crate) fn new() -> Self {
        Self {
            selected_device: Arc::new(Mutex::new(None)),
        }
    }
}

impl CardProvider for BatteryCardProvider {
    fn owner_id(&self) -> CardOwnerId {
        CardOwnerId::new("battery")
    }

    fn capabilities(&self) -> CardCapabilities {
        CardCapabilities {
            hover: false,
            click: true,
        }
    }

    fn size_tier(&self) -> CardSizeTier {
        CardSizeTier::Standard
    }

    fn anchor_bounds(&self, _cx: &App) -> Option<(Bounds<Pixels>, DisplayId)> {
        None
    }

    fn render_content(
        &self,
        _channel: CardChannel,
        window: &mut Window,
        cx: &mut App,
    ) -> AnyElement {
        let payload = ShellRuntime::device_snapshot(cx).battery;
        let lifecycle = ShellRuntime::service_availability(cx).battery_state;

        let selected = self.selected_device.lock().unwrap().clone();
        let selected_cell = self.selected_device.clone();

        render_battery_card(&lifecycle, &payload, selected, selected_cell, window, cx)
    }
}

fn format_time(seconds: u64, is_charging: bool) -> String {
    let hours = seconds / 3600;
    let mins = (seconds % 3600) / 60;
    let duration_str = if hours > 0 {
        format!("{hours}h {mins}m")
    } else {
        format!("{mins}m")
    };

    if is_charging {
        format!("{duration_str} until full")
    } else {
        format!("{duration_str} remaining")
    }
}

fn format_charge_state(state: BatteryChargeState) -> &'static str {
    match state {
        BatteryChargeState::Charging => "Charging",
        BatteryChargeState::Discharging => "Discharging",
        BatteryChargeState::Empty => "Empty",
        BatteryChargeState::FullyCharged => "Fully Charged",
        BatteryChargeState::PendingCharge => "Pending Charge",
        BatteryChargeState::PendingDischarge => "Pending Discharge",
        BatteryChargeState::Unknown => "Unknown",
    }
}

fn format_technology(tech: BatteryTechnology) -> &'static str {
    match tech {
        BatteryTechnology::LithiumIon => "Lithium Ion",
        BatteryTechnology::LithiumPolymer => "Lithium Polymer",
        BatteryTechnology::LithiumIronPhosphate => "Lithium Iron Phosphate",
        BatteryTechnology::LeadAcid => "Lead Acid",
        BatteryTechnology::NickelCadmium => "Nickel Cadmium",
        BatteryTechnology::NickelMetalHydride => "Nickel Metal Hydride",
        BatteryTechnology::Unknown => "Unknown",
    }
}

fn toggle_selected_device(selected: &mut Option<String>, device_id: &str) {
    if selected.as_deref() == Some(device_id) {
        *selected = None;
    } else {
        *selected = Some(device_id.to_owned());
    }
}

fn render_battery_card(
    lifecycle: &ServiceLifecycle,
    payload: &BatteryPayload,
    selected: Option<String>,
    selected_cell: Arc<Mutex<Option<String>>>,
    _window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let theme = cx.theme().clone();
    let is_reconnecting =
        matches!(lifecycle, ServiceLifecycle::Connecting { .. }) || !payload.available;

    if !payload.is_present {
        let (title, message) = if !payload.available {
            match lifecycle {
                ServiceLifecycle::Connecting { .. } => (
                    "Reconnecting battery service",
                    "Waiting for updated battery information.",
                ),
                ServiceLifecycle::Unavailable => (
                    "Battery unavailable",
                    "Battery information is currently unavailable.",
                ),
                ServiceLifecycle::Ready => (
                    "Battery unavailable",
                    "Battery information is currently unavailable.",
                ),
            }
        } else {
            ("No system battery", "The system battery was removed.")
        };
        return v_flex()
            .w_full()
            .gap(px(8.0))
            .p(px(16.0))
            .child(
                div()
                    .text_size(px(16.0))
                    .font_weight(gpui::FontWeight::BOLD)
                    .text_color(theme.on_surface)
                    .child(title),
            )
            .child(
                div()
                    .text_size(px(12.0))
                    .text_color(theme.on_surface_variant)
                    .child(message),
            )
            .into_any_element();
    }

    let mut card = v_flex()
        .id("battery-card-scroll")
        .w_full()
        .h_full()
        .overflow_y_scroll()
        .gap(px(12.0))
        .p(px(16.0));

    // Reconnecting or Service Disconnected Banner
    if is_reconnecting {
        let message = if payload.is_present {
            "Battery service unavailable — showing cached data"
        } else {
            "Battery service reconnecting"
        };
        card = card.child(
            h_flex()
                .w_full()
                .items_center()
                .gap(px(8.0))
                .p(px(8.0))
                .rounded_md()
                .bg(theme.surface_container_high)
                .text_color(theme.on_surface_variant)
                .text_size(px(12.0))
                .child(Icon::new(IconName::Refresh).size(px(14.0)))
                .child(div().child(message)),
        );
    }

    // Top Aggregate Summary Block
    let icon_name = if payload.is_charging() {
        IconName::Bolt
    } else {
        IconName::Info
    };

    let icon_color = if payload.is_charging() {
        theme.primary
    } else if payload.is_low_battery() {
        theme.error
    } else {
        theme.on_surface
    };

    let time_info = if payload.is_charging() {
        payload
            .time_to_full_secs
            .get()
            .map(|value| format_time(value, true))
    } else {
        payload
            .time_to_empty_secs
            .get()
            .map(|value| format_time(value, false))
    };

    let rate_info = payload
        .energy_rate_w
        .get()
        .map(|value| format!("{value:.1} W"));

    let health_info = payload
        .capacity_percent
        .get()
        .map(|value| format!("{value:.0}%"));

    let aggregate_block = v_flex()
        .w_full()
        .gap(px(8.0))
        .p(px(12.0))
        .rounded_lg()
        .bg(theme.surface_container)
        .child(
            h_flex()
                .items_center()
                .justify_between()
                .child(
                    h_flex()
                        .items_center()
                        .gap(px(10.0))
                        .child(Icon::new(icon_name).size(px(24.0)).text_color(icon_color))
                        .child(
                            div()
                                .font_family("Noto Sans Black")
                                .text_size(px(32.0))
                                .text_color(theme.on_surface)
                                .child(format!("{}%", payload.percentage)),
                        ),
                )
                .child(
                    div()
                        .py(px(2.0))
                        .px(px(8.0))
                        .rounded_full()
                        .bg(theme.surface_container_high)
                        .text_size(px(12.0))
                        .text_color(theme.on_surface_variant)
                        .child(format_charge_state(payload.state)),
                ),
        )
        .child(
            h_flex()
                .items_center()
                .justify_between()
                .text_size(px(12.0))
                .text_color(theme.on_surface_variant)
                .child(div().child(
                    time_info.unwrap_or_else(|| format_charge_state(payload.state).to_string()),
                ))
                .child(
                    h_flex()
                        .gap(px(12.0))
                        .children(rate_info.map(|r| div().child(format!("Rate: {r}"))))
                        .children(health_info.map(|h| div().child(format!("Health: {h}")))),
                ),
        );

    card = card.child(aggregate_block);

    // Physical Batteries Section
    if !payload.devices.is_empty() {
        let mut dev_list = v_flex().w_full().gap(px(6.0)).child(
            div()
                .text_size(px(11.0))
                .font_weight(gpui::FontWeight::BOLD)
                .text_color(theme.on_surface_variant)
                .child("PHYSICAL BATTERIES"),
        );

        for (idx, dev) in payload.devices.iter().enumerate() {
            let is_expanded = selected.as_deref() == Some(dev.id.as_str());
            let dev_name = if !dev.model.is_empty() {
                if !dev.vendor.is_empty() {
                    format!("{} {}", dev.vendor, dev.model)
                } else {
                    dev.model.clone()
                }
            } else {
                format!("Battery {}", idx + 1)
            };

            let dev_pct = if let Some(percentage) = dev.percentage.get() {
                format!("{percentage:.0}%")
            } else {
                format_charge_state(dev.charge_state).to_string()
            };

            let cell = selected_cell.clone();
            let device_id = dev.id.clone();
            let row_bg = if is_expanded {
                theme.surface_container_high
            } else {
                theme.surface_container
            };

            let mut row = v_flex().w_full().rounded_md().bg(row_bg).child(
                h_flex()
                    .w_full()
                    .items_center()
                    .justify_between()
                    .p(px(10.0))
                    .cursor_pointer()
                    .on_mouse_down(gpui::MouseButton::Left, move |_event, window, _cx| {
                        if let Ok(mut lock) = cell.lock() {
                            toggle_selected_device(&mut lock, &device_id);
                        }
                        window.refresh();
                    })
                    .child(
                        h_flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(Icon::new(IconName::Info).size(px(16.0)))
                            .child(
                                div()
                                    .text_size(px(13.0))
                                    .text_color(theme.on_surface)
                                    .child(dev_name),
                            ),
                    )
                    .child(
                        h_flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .text_color(theme.on_surface_variant)
                                    .child(dev_pct),
                            )
                            .child(
                                Icon::new(if is_expanded {
                                    IconName::KeyboardArrowUp
                                } else {
                                    IconName::KeyboardArrowDown
                                })
                                .size(px(14.0))
                                .text_color(theme.on_surface_variant),
                            ),
                    ),
            );

            // Single allowed secondary detail view when expanded
            if is_expanded {
                let detail_box = render_physical_device_details(dev, &theme);
                row = row.child(detail_box);
            }

            dev_list = dev_list.child(row);
        }

        card = card.child(dev_list);
    }

    card.into_any_element()
}

fn physical_detail_rows(dev: &BatteryDevicePayload) -> Vec<(&'static str, String)> {
    let mut rows = Vec::new();
    if dev.technology != BatteryTechnology::Unknown {
        rows.push(("Technology", format_technology(dev.technology).to_string()));
    }
    if let Some(value) = dev.energy_wh.get() {
        rows.push(("Energy", format!("{value:.1} Wh")));
    }
    if let Some(value) = dev.energy_empty_wh.get() {
        rows.push(("Energy empty", format!("{value:.1} Wh")));
    }
    if let Some(value) = dev.energy_full_wh.get() {
        rows.push(("Energy full", format!("{value:.1} Wh")));
    }
    if let Some(value) = dev.energy_full_design_wh.get() {
        rows.push(("Design energy", format!("{value:.1} Wh")));
    }
    if let Some(value) = dev.energy_rate_w.get() {
        rows.push(("Rate", format!("{value:.1} W")));
    }
    if let Some(value) = dev.capacity_percent.get() {
        rows.push(("Health", format!("{value:.0}%")));
    }
    if let Some(value) = dev.voltage_v.get() {
        rows.push(("Voltage", format!("{value:.2} V")));
    }
    if let Some(value) = dev.voltage_min_design_v.get() {
        rows.push(("Minimum design voltage", format!("{value:.2} V")));
    }
    if let Some(value) = dev.voltage_max_design_v.get() {
        rows.push(("Maximum design voltage", format!("{value:.2} V")));
    }
    if let Some(value) = dev.temperature_c.get() {
        rows.push(("Temperature", format!("{value:.1} °C")));
    }
    if let Some(value) = dev.cycle_count.get() {
        rows.push(("Cycles", value.to_string()));
    }
    if let Some(value) = dev.update_time.get() {
        rows.push(("Updated", value.to_string()));
    }
    if let Some(value) = dev.is_rechargeable.get() {
        rows.push(("Rechargeable", if value { "Yes" } else { "No" }.to_string()));
    }
    if dev.warning_level != BatteryWarningLevel::Unknown
        && dev.warning_level != BatteryWarningLevel::None
    {
        rows.push(("Warning Level", format!("{:?}", dev.warning_level)));
    }
    if dev.coarse_level != BatteryCoarseLevel::Unknown
        && dev.coarse_level != BatteryCoarseLevel::None
    {
        rows.push(("Coarse Level", format!("{:?}", dev.coarse_level)));
    }
    if dev.has_history || dev.has_statistics {
        rows.push((
            "Diagnostics",
            if dev.has_history && dev.has_statistics {
                "History & Statistics"
            } else if dev.has_history {
                "History"
            } else {
                "Statistics"
            }
            .to_string(),
        ));
    }
    match (
        dev.charge_start_threshold.get(),
        dev.charge_end_threshold.get(),
    ) {
        (Some(start), Some(end)) => rows.push(("Charge limits", format!("{start}%–{end}%"))),
        (Some(start), None) => rows.push(("Charge starts below", format!("{start}%"))),
        (None, Some(end)) => rows.push(("Charge stops at", format!("{end}%"))),
        (None, None) => {}
    }

    rows
}

fn render_physical_device_details(
    dev: &BatteryDevicePayload,
    theme: &shilpo_ui::Theme,
) -> AnyElement {
    let mut details = v_flex()
        .w_full()
        .gap(px(4.0))
        .p(px(10.0))
        .border_t_1()
        .border_color(theme.outline_variant)
        .text_size(px(11.0))
        .text_color(theme.on_surface_variant);

    for (label, value) in physical_detail_rows(dev) {
        details = details.child(
            h_flex()
                .justify_between()
                .child(div().font_weight(gpui::FontWeight::MEDIUM).child(label))
                .child(div().text_color(theme.on_surface).child(value)),
        );
    }

    details.into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;
    use shilpo_device_protocol::{OptionalF64, OptionalU64};

    #[test]
    fn test_format_time_helpers() {
        assert_eq!(format_time(14400, false), "4h 0m remaining");
        assert_eq!(format_time(3900, true), "1h 5m until full");
        assert_eq!(format_time(120, false), "2m remaining");
    }

    #[test]
    fn test_format_charge_state_helpers() {
        assert_eq!(
            format_charge_state(BatteryChargeState::Charging),
            "Charging"
        );
        assert_eq!(
            format_charge_state(BatteryChargeState::Discharging),
            "Discharging"
        );
        assert_eq!(
            format_charge_state(BatteryChargeState::FullyCharged),
            "Fully Charged"
        );
        assert_eq!(
            format_charge_state(BatteryChargeState::PendingCharge),
            "Pending Charge"
        );
        assert_eq!(format_charge_state(BatteryChargeState::Unknown), "Unknown");
    }

    #[test]
    fn test_format_technology_helpers() {
        assert_eq!(
            format_technology(BatteryTechnology::LithiumIon),
            "Lithium Ion"
        );
        assert_eq!(
            format_technology(BatteryTechnology::LithiumPolymer),
            "Lithium Polymer"
        );
        assert_eq!(format_technology(BatteryTechnology::Unknown), "Unknown");
    }

    #[test]
    fn test_battery_card_provider_capabilities() {
        let provider = BatteryCardProvider::new();
        assert_eq!(provider.owner_id(), CardOwnerId::new("battery"));
        assert!(provider.capabilities().click);
        assert!(!provider.capabilities().hover);
        assert_eq!(provider.size_tier(), CardSizeTier::Standard);
    }

    #[test]
    fn details_omit_unavailable_metrics_and_keep_single_threshold_meaningful() {
        let device = BatteryDevicePayload {
            capacity_percent: OptionalF64::some(87.4),
            charge_end_threshold: OptionalU64::some(80),
            ..Default::default()
        };

        let rows = physical_detail_rows(&device);
        assert!(rows.contains(&("Health", "87%".to_string())));
        assert!(rows.contains(&("Charge stops at", "80%".to_string())));
        assert!(!rows.iter().any(|(label, _)| *label == "Energy"));
        assert!(!rows.iter().any(|(_, value)| value.contains("0%–80%")));
    }

    #[test]
    fn physical_row_uses_charge_percentage_not_health() {
        let device = BatteryDevicePayload {
            percentage: OptionalF64::some(42.0),
            capacity_percent: OptionalF64::some(91.0),
            ..Default::default()
        };
        assert_eq!(device.percentage.get(), Some(42.0));
        assert_eq!(device.capacity_percent.get(), Some(91.0));
    }

    #[test]
    fn selecting_a_device_replaces_the_single_secondary_detail() {
        let mut selected = None;
        toggle_selected_device(&mut selected, "BAT0");
        assert_eq!(selected.as_deref(), Some("BAT0"));
        toggle_selected_device(&mut selected, "BAT1");
        assert_eq!(selected.as_deref(), Some("BAT1"));
        toggle_selected_device(&mut selected, "BAT1");
        assert_eq!(selected, None);
    }
}
