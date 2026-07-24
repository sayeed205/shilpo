use crate::runtime::ShellRuntime;
use gpui::{
    App, AppContext, Context, Entity, FocusHandle, Focusable, InteractiveElement, IntoElement,
    KeyDownEvent, ParentElement, Render, Role, StatefulInteractiveElement, Styled, Window, div, px,
    relative,
};
use shilpo_services::{AudioService, BatteryService, BrightnessService, NetworkService};
use shilpo_ui::{
    ActiveTheme, Colorize, FocusTrapElement, Icon, IconName, Sizable, StyledExt, h_flex,
    slider::{Slider, SliderEvent, SliderState, SliderValue},
    v_flex,
};
use std::sync::Arc;

/// M3 Expressive Control Center Panel Overlay View.
pub struct ControlCenterView {
    pub battery_service: Option<BatteryService>,
    pub network_service: Option<NetworkService>,
    pub audio_service: Option<AudioService>,
    pub brightness_service: Option<Arc<BrightnessService>>,
    volume_state: Entity<SliderState>,
    brightness_state: Entity<SliderState>,
    dnd_active: bool,
    night_light_active: bool,
    focus_handle: FocusHandle,
}

impl ControlCenterView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let battery_service = service_or_warn(BatteryService::new, "battery");
        let network_service = service_or_warn(NetworkService::new, "network");
        let audio_service = service_or_warn(AudioService::new, "audio");
        let brightness_service = match BrightnessService::new() {
            Ok(service) => Some(Arc::new(service)),
            Err(error) => {
                tracing::warn!(error = %error, "brightness service unavailable");
                None
            }
        };

        let audio_available = audio_service
            .as_ref()
            .map(|service| service.audio_info().available)
            .unwrap_or(false);
        let initial_volume = audio_service
            .as_ref()
            .map(|service| service.audio_info().volume as f32)
            .unwrap_or(0.0);
        let initial_brightness = brightness_service
            .as_ref()
            .map(|service| service.brightness_info().percentage as f32)
            .unwrap_or(0.0);

        let volume_state = cx.new(|_| {
            SliderState::new()
                .min(0.0)
                .max(100.0)
                .step(1.0)
                .default_value(initial_volume)
        });

        let brightness_state = cx.new(|_| {
            SliderState::new()
                .min(0.0)
                .max(100.0)
                .step(1.0)
                .default_value(initial_brightness)
        });

        let focus_handle = cx.focus_handle();
        window.focus(&focus_handle, cx);

        window.on_window_should_close(cx, |_, cx| {
            ShellRuntime::forget_control_center(cx);
            true
        });

        // Dynamic theme synchronization with OS appearance
        shilpo_ui::Theme::sync_system_appearance(Some(window), cx);
        cx.observe_window_appearance(window, |_, window, cx| {
            shilpo_ui::Theme::sync_system_appearance(Some(window), cx);
            window.refresh();
        })
        .detach();

        // Subscribe to volume changes
        cx.subscribe(&volume_state, move |_, _, event: &SliderEvent, _| {
            if audio_available && let SliderEvent::Change(SliderValue::Single(val)) = event {
                let _ = std::process::Command::new("pactl")
                    .args([
                        "set-sink-volume",
                        "@DEFAULT_SINK@",
                        &format!("{}%", val.round() as u8),
                    ])
                    .spawn();
            }
        })
        .detach();

        // Subscribe to brightness changes
        let brightness_srv_clone = brightness_service.clone();
        cx.subscribe(&brightness_state, move |_, _, event: &SliderEvent, _| {
            if let Some(service) = &brightness_srv_clone
                && service.brightness_info().available
                && let SliderEvent::Change(SliderValue::Single(val)) = event
            {
                service.set_brightness(val.round() as u8);
            }
        })
        .detach();

        Self {
            battery_service,
            network_service,
            audio_service,
            brightness_service,
            volume_state,
            brightness_state,
            dnd_active: false,
            night_light_active: false,
            focus_handle,
        }
    }

    pub fn view(window: &mut Window, cx: &mut App) -> Entity<shilpo_ui::Root> {
        let control_center = cx.new(|cx| Self::new(window, cx));
        cx.new(|cx| {
            shilpo_ui::Root::new(control_center, window, cx)
                .bordered(false)
                .bg(cx.theme().transparent)
        })
    }

    fn toggle_dnd(&mut self, cx: &mut Context<Self>) {
        self.dnd_active = !self.dnd_active;
        cx.notify();
    }

    fn toggle_night_light(&mut self, cx: &mut Context<Self>) {
        self.night_light_active = !self.night_light_active;
        cx.notify();
    }

    fn handle_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.keystroke.key.as_str() == "escape" {
            ShellRuntime::forget_control_center(cx);
            window.remove_window();
        }
    }
}

fn service_or_warn<T>(create: impl FnOnce() -> anyhow::Result<T>, name: &str) -> Option<T> {
    match create() {
        Ok(service) => Some(service),
        Err(error) => {
            tracing::warn!(error = %error, service = name, "control-center service unavailable");
            None
        }
    }
}

impl Focusable for ControlCenterView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for ControlCenterView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let battery = self
            .battery_service
            .as_ref()
            .map(BatteryService::battery_info)
            .unwrap_or_default();
        let network = self
            .network_service
            .as_ref()
            .map(NetworkService::network_info)
            .unwrap_or_default();
        let audio_available = self
            .audio_service
            .as_ref()
            .map(|service| service.audio_info().available)
            .unwrap_or(false);
        let brightness_available = self
            .brightness_service
            .as_ref()
            .map(|service| service.brightness_info().available)
            .unwrap_or(false);

        // Grid toggles
        let wifi_bg = if network.available && network.is_connected {
            cx.theme().primary_container
        } else {
            cx.theme().surface_container_highest
        };
        let wifi_fg = if network.available && network.is_connected {
            cx.theme().on_primary_container
        } else {
            cx.theme().on_surface_variant
        };

        let dnd_bg = if self.dnd_active {
            cx.theme().primary_container
        } else {
            cx.theme().surface_container_highest
        };
        let dnd_fg = if self.dnd_active {
            cx.theme().on_primary_container
        } else {
            cx.theme().on_surface_variant
        };

        let night_bg = if self.night_light_active {
            cx.theme().primary_container
        } else {
            cx.theme().surface_container_highest
        };
        let night_fg = if self.night_light_active {
            cx.theme().on_primary_container
        } else {
            cx.theme().on_surface_variant
        };

        div()
            .w_full()
            .h_full()
            .flex()
            .items_center()
            .justify_center()
            .bg(cx.theme().scrim.opacity(0.4))
            .id("control-center-backdrop")
            .on_mouse_down(gpui::MouseButton::Left, |_, window, cx| {
                ShellRuntime::forget_control_center(cx);
                window.remove_window();
            })
            .child(
                v_flex()
                    .id("control-center-card")
                    .role(Role::Dialog)
                    .aria_label("Control Center Panel")
                    .track_focus(&self.focus_handle(cx))
                    .focus_trap("control-center-card-trap", &self.focus_handle)
                    .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                        this.handle_key_down(event, window, cx);
                    }))
                    .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| {
                        cx.stop_propagation();
                    })
                    .w(px(340.))
                    .p_5()
                    .gap_4()
                    .rounded_3xl()
                    .bg(cx.theme().surface_container_high)
                    .border_1()
                    .border_color(cx.theme().outline_variant.opacity(0.4))
                    .shadow_2xl()
                    // Header Info Summary
                    .child(
                        h_flex()
                            .justify_between()
                            .items_center()
                            .child(
                                h_flex()
                                    .gap_2()
                                    .items_center()
                                    .child(Icon::new(IconName::Network).size(px(16.)))
                                    .child(div().text_sm().font_bold().child(
                                        if network.available {
                                            network.ssid.unwrap_or_else(|| "WiFi".into())
                                        } else {
                                            "WiFi unavailable".into()
                                        },
                                    )),
                            )
                            .child(
                                h_flex()
                                    .gap_2()
                                    .items_center()
                                    .child(
                                        div()
                                            .text_sm()
                                            .font_bold()
                                            .child(format!("{}%", battery.percentage)),
                                    )
                                    .child(Icon::new(IconName::Battery).size(px(16.))),
                            ),
                    )
                    // Grid Toggles (WiFi, Bluetooth, DND, Night Light)
                    .child(
                        v_flex()
                            .gap_2_5()
                            .child(
                                h_flex()
                                    .gap_2_5()
                                    // WiFi Toggle
                                    .child(
                                        h_flex()
                                            .id("cc-toggle-wifi")
                                            .w(relative(0.5))
                                            .px_3()
                                            .py_2()
                                            .rounded_xl()
                                            .bg(wifi_bg)
                                            .text_color(wifi_fg)
                                            .gap_2_5()
                                            .items_center()
                                            .child(Icon::new(IconName::Network).size(px(16.)))
                                            .child(div().text_xs().font_bold().child("WiFi")),
                                    )
                                    // DND Toggle
                                    .child(
                                        h_flex()
                                            .id("cc-toggle-dnd")
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.toggle_dnd(cx);
                                            }))
                                            .w(relative(0.5))
                                            .px_3()
                                            .py_2()
                                            .rounded_xl()
                                            .bg(dnd_bg)
                                            .text_color(dnd_fg)
                                            .gap_2_5()
                                            .items_center()
                                            .child(Icon::new(IconName::Copy).size(px(16.)))
                                            .child(div().text_xs().font_bold().child("DND")),
                                    ),
                            )
                            .child(
                                h_flex()
                                    .gap_2_5()
                                    // Bluetooth Toggle
                                    .child(
                                        h_flex()
                                            .id("cc-toggle-bluetooth")
                                            .w(relative(0.5))
                                            .px_3()
                                            .py_2()
                                            .rounded_xl()
                                            .bg(cx.theme().surface_container_highest)
                                            .text_color(cx.theme().on_surface_variant)
                                            .gap_2_5()
                                            .items_center()
                                            .child(
                                                Icon::new(IconName::SquareTerminal).size(px(16.)),
                                            )
                                            .child(div().text_xs().font_bold().child("Bluetooth")),
                                    )
                                    // Night Light Toggle
                                    .child(
                                        h_flex()
                                            .id("cc-toggle-night")
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.toggle_night_light(cx);
                                            }))
                                            .w(relative(0.5))
                                            .px_3()
                                            .py_2()
                                            .rounded_xl()
                                            .bg(night_bg)
                                            .text_color(night_fg)
                                            .gap_2_5()
                                            .items_center()
                                            .child(Icon::new(IconName::Star).size(px(16.)))
                                            .child(
                                                div().text_xs().font_bold().child("Night Light"),
                                            ),
                                    ),
                            ),
                    )
                    // Volume Slider
                    .child(
                        v_flex()
                            .gap_1()
                            .child(
                                h_flex()
                                    .justify_between()
                                    .items_center()
                                    .child(div().text_xs().font_semibold().child("Volume"))
                                    .child(Icon::new(IconName::Star).size(px(14.))),
                            )
                            .child(
                                Slider::new(&self.volume_state)
                                    .disabled(!audio_available)
                                    .horizontal()
                                    .with_size(shilpo_ui::Size::Small),
                            ),
                    )
                    // Brightness Slider
                    .child(
                        v_flex()
                            .gap_1()
                            .child(
                                h_flex()
                                    .justify_between()
                                    .items_center()
                                    .child(div().text_xs().font_semibold().child("Brightness"))
                                    .child(Icon::new(IconName::Search).size(px(14.))),
                            )
                            .child(
                                Slider::new(&self.brightness_state)
                                    .disabled(!brightness_available)
                                    .horizontal()
                                    .with_size(shilpo_ui::Size::Small),
                            ),
                    )
                    // Power Session Action buttons
                    .child(
                        h_flex()
                            .gap_2_5()
                            .justify_between()
                            .child(
                                div()
                                    .id("cc-power-lock")
                                    .px_3()
                                    .py_2()
                                    .rounded_xl()
                                    .bg(cx.theme().surface_container_highest)
                                    .text_color(cx.theme().on_surface)
                                    .cursor_pointer()
                                    .hover(|s| {
                                        s.bg(cx.theme().surface_container_highest.darken(0.1))
                                    })
                                    .child(Icon::new(IconName::SquareTerminal).size(px(16.))),
                            )
                            .child(
                                div()
                                    .id("cc-power-suspend")
                                    .role(Role::Button)
                                    .aria_label("Suspend System")
                                    .px_3()
                                    .py_2()
                                    .rounded_xl()
                                    .bg(cx.theme().surface_container_highest)
                                    .text_color(cx.theme().on_surface)
                                    .cursor_pointer()
                                    .hover(|s| {
                                        s.bg(cx.theme().surface_container_highest.darken(0.1))
                                    })
                                    .on_click(|_, window, cx| {
                                        let _ = std::process::Command::new("systemctl")
                                            .arg("suspend")
                                            .spawn();
                                        ShellRuntime::forget_control_center(cx);
                                        window.remove_window();
                                    })
                                    .child(Icon::new(IconName::Copy).size(px(16.))),
                            )
                            .child(
                                div()
                                    .id("cc-power-reboot")
                                    .role(Role::Button)
                                    .aria_label("Reboot System")
                                    .px_3()
                                    .py_2()
                                    .rounded_xl()
                                    .bg(cx.theme().surface_container_highest)
                                    .text_color(cx.theme().on_surface)
                                    .cursor_pointer()
                                    .hover(|s| {
                                        s.bg(cx.theme().surface_container_highest.darken(0.1))
                                    })
                                    .on_click(|_, window, cx| {
                                        let _ = std::process::Command::new("systemctl")
                                            .arg("reboot")
                                            .spawn();
                                        ShellRuntime::forget_control_center(cx);
                                        window.remove_window();
                                    })
                                    .child(Icon::new(IconName::Network).size(px(16.))),
                            )
                            .child(
                                div()
                                    .id("cc-power-off")
                                    .role(Role::Button)
                                    .aria_label("Power Off System")
                                    .px_3()
                                    .py_2()
                                    .rounded_xl()
                                    .bg(cx.theme().primary_container)
                                    .text_color(cx.theme().on_primary_container)
                                    .cursor_pointer()
                                    .hover(|s| s.bg(cx.theme().primary_container.darken(0.1)))
                                    .on_click(|_, window, cx| {
                                        let _ = std::process::Command::new("systemctl")
                                            .arg("poweroff")
                                            .spawn();
                                        ShellRuntime::forget_control_center(cx);
                                        window.remove_window();
                                    })
                                    .child(Icon::new(IconName::Star).size(px(16.))),
                            ),
                    )
                    // Notifications Drawer Header & History List
                    .child(
                        v_flex()
                            .gap_2()
                            .child(
                                h_flex()
                                    .justify_between()
                                    .items_center()
                                    .child(div().text_xs().font_bold().child("Notifications"))
                                    .child(
                                        div()
                                            .id("cc-clear-notifications")
                                            .role(Role::Button)
                                            .aria_label("Clear All Notifications")
                                            .text_xs()
                                            .font_semibold()
                                            .text_color(cx.theme().primary)
                                            .cursor_pointer()
                                            .on_click(|_, _, cx| {
                                                ShellRuntime::clear_notification_history(cx);
                                            })
                                            .child("Clear All"),
                                    ),
                            )
                            .child({
                                let history = ShellRuntime::notification_history(cx);
                                if history.is_empty() {
                                    div()
                                        .py_2()
                                        .text_xs()
                                        .text_color(cx.theme().on_surface_variant)
                                        .child("No recent notifications")
                                } else {
                                    v_flex()
                                        .gap_1_5()
                                        .children(history.iter().rev().take(4).map(|n| {
                                            h_flex()
                                                .px_2p5()
                                                .py_2()
                                                .rounded_xl()
                                                .bg(cx.theme().surface_container)
                                                .justify_between()
                                                .items_center()
                                                .child(
                                                    v_flex()
                                                        .gap_0p5()
                                                        .child(
                                                            div()
                                                                .text_xs()
                                                                .font_bold()
                                                                .text_color(cx.theme().on_surface)
                                                                .child(n.summary.clone()),
                                                        )
                                                        .child(
                                                            div()
                                                                .text_xs()
                                                                .text_color(
                                                                    cx.theme().on_surface_variant,
                                                                )
                                                                .child(n.body.clone()),
                                                        ),
                                                )
                                                .child(
                                                    div()
                                                        .text_xs()
                                                        .text_color(cx.theme().outline_variant)
                                                        .child(
                                                            n.timestamp.format("%H:%M").to_string(),
                                                        ),
                                                )
                                        }))
                                }
                            }),
                    ),
            )
    }
}
