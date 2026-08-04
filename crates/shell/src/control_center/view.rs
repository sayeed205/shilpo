use crate::battery::BatteryIndicator;
use crate::runtime::ShellRuntime;
use gpui::{
    App, AppContext, Context, Entity, FocusHandle, Focusable, InteractiveElement, IntoElement,
    KeyDownEvent, ParentElement, Render, Role, StatefulInteractiveElement, Styled, Window, div,
    prelude::FluentBuilder, px, relative,
};
use shilpo_services::{
    BluetoothService, NightLightService, PowerProfile, PowerProfileService, ScreenCaptureService,
    ScreenshotMode,
};
use shilpo_ui::{
    ActiveTheme, Colorize, FocusTrapElement, Icon, IconName, Sizable, StyledExt, h_flex,
    slider::{Slider, SliderEvent, SliderState, SliderValue},
    v_flex,
};

/// M3 Expressive Control Center Panel Overlay View.
pub struct ControlCenterView {
    pub night_light_service: Option<NightLightService>,
    pub bluetooth_service: Option<BluetoothService>,
    pub power_profile_service: Option<PowerProfileService>,
    pub screen_capture_service: Option<ScreenCaptureService>,
    volume_state: Entity<SliderState>,
    brightness_state: Entity<SliderState>,
    dnd_active: bool,
    night_light_active: bool,
    bluetooth_active: bool,
    recording_state: shilpo_capture::RecordingState,
    active_power_profile: PowerProfile,
    focus_handle: FocusHandle,
    device_snapshot: crate::bar::service_worker::DeviceSnapshot,
    _device_snapshot_task: gpui::Task<()>,
}

impl ControlCenterView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let snapshot = ShellRuntime::device_snapshot(cx);
        let night_light_service = service_or_warn(NightLightService::new, "night_light");
        let bluetooth_service = service_or_warn(BluetoothService::new, "bluetooth");
        let power_profile_service = service_or_warn(PowerProfileService::new, "power_profile");
        let screen_capture_service = service_or_warn(ScreenCaptureService::new, "screen_capture");

        let initial_recording_state = ShellRuntime::recording_state(cx);

        let initial_bluetooth = bluetooth_service
            .as_ref()
            .map(|s| s.info().powered)
            .unwrap_or(false);
        let initial_power_profile = power_profile_service
            .as_ref()
            .map(|s| s.info().active_profile)
            .unwrap_or(PowerProfile::Balanced);

        let initial_night_light = night_light_service
            .as_ref()
            .map(|service| service.info().is_active)
            .unwrap_or(false);
        let initial_dnd = ShellRuntime::is_dnd_active(cx);

        let volume_state = cx.new(|_| {
            SliderState::new()
                .min(0.0)
                .max(100.0)
                .step(1.0)
                .default_value(snapshot.audio.volume as f32)
        });

        let brightness_state = cx.new(|_| {
            SliderState::new()
                .min(0.0)
                .max(100.0)
                .step(1.0)
                .default_value(snapshot.brightness.percentage as f32)
        });

        let focus_handle = cx.focus_handle();
        window.focus(&focus_handle, cx);

        window.on_window_should_close(cx, |_, cx| {
            ShellRuntime::forget_control_center(cx);
            true
        });

        // Subscribe to volume changes
        cx.subscribe(&volume_state, |_, _, event: &SliderEvent, cx| {
            if let SliderEvent::Change(SliderValue::Single(val)) = event {
                ShellRuntime::dispatch_device_command(
                    cx,
                    crate::bar::service_worker::DeviceCommand::Audio(
                        crate::bar::service_worker::AudioCommand::SetDefaultVolume(
                            val.round() as u8
                        ),
                    ),
                );
            }
        })
        .detach();

        // Subscribe to brightness changes
        cx.subscribe(&brightness_state, |_, _, event: &SliderEvent, cx| {
            if let SliderEvent::Change(SliderValue::Single(val)) = event {
                ShellRuntime::dispatch_device_command(
                    cx,
                    crate::bar::service_worker::DeviceCommand::Brightness(val.round() as u8),
                );
            }
        })
        .detach();

        let snapshot_clone = snapshot.clone();
        let _device_snapshot_task = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(250))
                    .await;
                let res = this.update(cx, |view, cx| {
                    let snapshot = ShellRuntime::device_snapshot(cx);
                    if replace_device_snapshot_if_changed(&mut view.device_snapshot, snapshot) {
                        cx.notify();
                    }
                    let recording_state = ShellRuntime::recording_state(cx);
                    if view.recording_state != recording_state {
                        view.recording_state = recording_state;
                        cx.notify();
                    }
                });
                if res.is_err() {
                    break;
                }
            }
        });

        Self {
            night_light_service,
            bluetooth_service,
            power_profile_service,
            screen_capture_service,
            volume_state,
            brightness_state,
            dnd_active: initial_dnd,
            night_light_active: initial_night_light,
            bluetooth_active: initial_bluetooth,
            recording_state: initial_recording_state,
            active_power_profile: initial_power_profile,
            focus_handle,
            device_snapshot: snapshot_clone,
            _device_snapshot_task,
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
        ShellRuntime::set_dnd_enabled(cx, self.dnd_active);
        cx.notify();
    }

    fn toggle_night_light(&mut self, cx: &mut Context<Self>) {
        if let Some(service) = &self.night_light_service {
            self.night_light_active = service.toggle();
        } else {
            self.night_light_active = !self.night_light_active;
        }
        cx.notify();
    }

    fn toggle_bluetooth(&mut self, cx: &mut Context<Self>) {
        if let Some(service) = &self.bluetooth_service {
            self.bluetooth_active = service.toggle();
        } else {
            self.bluetooth_active = !self.bluetooth_active;
        }
        cx.notify();
    }

    fn set_power_profile(&mut self, profile: PowerProfile, cx: &mut Context<Self>) {
        if let Some(service) = &self.power_profile_service {
            if service.set_profile(profile.clone()) {
                self.active_power_profile = profile;
            }
        } else {
            self.active_power_profile = profile;
        }
        cx.notify();
    }

    fn cycle_power_profile(&mut self, cx: &mut Context<Self>) {
        let next = match self.active_power_profile {
            PowerProfile::PowerSaver => PowerProfile::Balanced,
            PowerProfile::Balanced => PowerProfile::Performance,
            PowerProfile::Performance => PowerProfile::PowerSaver,
        };
        self.set_power_profile(next, cx);
    }

    fn take_screenshot(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(service) = &self.screen_capture_service {
            service.take_screenshot(ScreenshotMode::Region, None);
        } else {
            let config = shilpo_config::CaptureConfig::default();
            cx.background_executor()
                .spawn(async move {
                    let _ = shilpo_capture::open_selector(
                        shilpo_capture::CaptureIntent::Clipboard,
                        &config,
                        None,
                    )
                    .await;
                })
                .detach();
        }
        ShellRuntime::forget_control_center(cx);
        window.remove_window();
    }

    fn toggle_recording(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if ShellRuntime::recording_state(cx).is_stoppable() {
            ShellRuntime::stop_recording(cx);
        } else {
            ShellRuntime::forget_control_center(cx);
            window.remove_window();
            let audio = ShellRuntime::configured_recording_audio(cx);
            ShellRuntime::open_recording_chooser(cx, audio);
        }
        cx.notify();
    }

    fn toggle_recording_pause(&mut self, cx: &mut Context<Self>) {
        if matches!(
            self.recording_state,
            shilpo_capture::RecordingState::Paused { .. }
        ) {
            ShellRuntime::resume_recording(cx);
        } else if matches!(
            self.recording_state,
            shilpo_capture::RecordingState::Recording { .. }
        ) {
            ShellRuntime::pause_recording(cx);
        }
    }

    fn select_audio_device(&mut self, device_id: &str, is_input: bool, cx: &mut Context<Self>) {
        ShellRuntime::dispatch_device_command(
            cx,
            crate::bar::service_worker::DeviceCommand::Audio(
                crate::bar::service_worker::AudioCommand::SetDefaultDevice {
                    device_id: device_id.to_string(),
                    is_input,
                },
            ),
        );
        ShellRuntime::save_audio_preference(cx, Some(device_id.to_string()), None);
        cx.notify();
    }

    fn select_audio_port(&mut self, sink_name: &str, port_name: &str, cx: &mut Context<Self>) {
        ShellRuntime::dispatch_device_command(
            cx,
            crate::bar::service_worker::DeviceCommand::Audio(
                crate::bar::service_worker::AudioCommand::SetSinkPort {
                    sink_name: sink_name.to_string(),
                    port_name: port_name.to_string(),
                },
            ),
        );
        ShellRuntime::save_audio_preference(cx, None, Some(port_name.to_string()));
        cx.notify();
    }

    fn toggle_simultaneous_audio(&mut self, cx: &mut Context<Self>) {
        ShellRuntime::dispatch_device_command(
            cx,
            crate::bar::service_worker::DeviceCommand::Audio(
                crate::bar::service_worker::AudioCommand::ToggleSimultaneousOutput,
            ),
        );
        cx.notify();
    }

    fn toggle_wifi(&mut self, enabled: bool, cx: &mut Context<Self>) {
        ShellRuntime::dispatch_device_command(
            cx,
            crate::bar::service_worker::DeviceCommand::Network(
                crate::bar::service_worker::NetworkCommand::SetWifiEnabled(enabled),
            ),
        );
        cx.notify();
    }

    fn deactivate_vpn(&mut self, active_conn_path: &str, cx: &mut Context<Self>) {
        ShellRuntime::dispatch_device_command(
            cx,
            crate::bar::service_worker::DeviceCommand::Network(
                crate::bar::service_worker::NetworkCommand::DeactivateConnection(
                    active_conn_path.to_string(),
                ),
            ),
        );
        cx.notify();
    }

    fn toggle_airplane_mode(&mut self, enabled: bool, cx: &mut Context<Self>) {
        ShellRuntime::dispatch_device_command(
            cx,
            crate::bar::service_worker::DeviceCommand::Network(
                crate::bar::service_worker::NetworkCommand::SetAirplaneModeEnabled(enabled),
            ),
        );
        cx.notify();
    }

    fn handle_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event.keystroke.key.as_str() {
            "escape" => {
                ShellRuntime::forget_control_center(cx);
                window.remove_window();
            }
            "d" => self.toggle_dnd(cx),
            "n" => self.toggle_night_light(cx),
            "b" => self.toggle_bluetooth(cx),
            "p" => self.cycle_power_profile(cx),
            _ => {}
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

fn format_recording_elapsed(elapsed: std::time::Duration) -> String {
    let total_seconds = elapsed.as_secs();
    let hours = total_seconds / 3_600;
    let minutes = total_seconds % 3_600 / 60;
    let seconds = total_seconds % 60;
    if hours > 0 {
        format!("{hours:02}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes:02}:{seconds:02}")
    }
}

fn replace_device_snapshot_if_changed(
    current: &mut crate::bar::service_worker::DeviceSnapshot,
    new: crate::bar::service_worker::DeviceSnapshot,
) -> bool {
    if *current == new {
        false
    } else {
        *current = new;
        true
    }
}

impl Focusable for ControlCenterView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for ControlCenterView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let extension_entries = ShellRuntime::extension_surface_views(
            cx,
            crate::extensions::ContributionSurface::ControlCenter,
        )
        .into_iter()
        .map(|(id, tree)| {
            crate::bar::ext_view_adapter::render_ext_view_tree(&id, None, &tree, window, cx)
        })
        .collect::<Vec<_>>();
        let side_panel_entries = ShellRuntime::extension_descriptors(
            cx,
            crate::extensions::ContributionSurface::SidePanel,
        )
        .into_iter()
        .map(|descriptor| {
            let contribution = descriptor.id;
            h_flex()
                .id(format!("extension-panel:{contribution}"))
                .role(Role::Button)
                .cursor_pointer()
                .px_3()
                .py_2()
                .rounded_xl()
                .bg(cx.theme().surface_container)
                .child(descriptor.name)
                .on_click(move |_, _, cx| {
                    ShellRuntime::open_extension_panel(cx, contribution.clone());
                })
                .into_any_element()
        })
        .collect::<Vec<_>>();
        let battery = self.device_snapshot.battery.clone();
        let network = self.device_snapshot.network.clone();
        let audio_available = self.device_snapshot.audio.available;
        let brightness_available = self.device_snapshot.brightness.available;

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

        let bt_bg = if self.bluetooth_active {
            cx.theme().primary_container
        } else {
            cx.theme().surface_container_highest
        };
        let bt_fg = if self.bluetooth_active {
            cx.theme().on_primary_container
        } else {
            cx.theme().on_surface_variant
        };

        let is_recording = self.recording_state.is_active();
        let is_paused = matches!(
            self.recording_state,
            shilpo_capture::RecordingState::Paused { .. }
        );
        let record_bg = if is_recording {
            cx.theme().error_container
        } else {
            cx.theme().surface_container_highest
        };
        let record_fg = if is_recording {
            cx.theme().on_error_container
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
                                    .child(Icon::new(IconName::Lan).size(px(16.)))
                                    .child(div().text_sm().font_bold().child(
                                        if network.available {
                                            network.ssid.unwrap_or_else(|| "WiFi".into())
                                        } else {
                                            "WiFi unavailable".into()
                                        },
                                    )),
                            )
                            .when(battery.is_present, |this| {
                                this.child(BatteryIndicator::new("cc-battery", battery))
                            }),
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
                                            .cursor_pointer()
                                            .on_click(cx.listener({
                                                let wifi_enabled = network.wifi_enabled;
                                                move |this, _, _, cx| {
                                                    this.toggle_wifi(!wifi_enabled, cx);
                                                }
                                            }))
                                            .w(relative(0.5))
                                            .px_3()
                                            .py_2()
                                            .rounded_xl()
                                            .bg(wifi_bg)
                                            .text_color(wifi_fg)
                                            .gap_2_5()
                                            .items_center()
                                            .child(Icon::new(IconName::Lan).size(px(16.)))
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
                                            .child(Icon::new(IconName::ContentCopy).size(px(16.)))
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
                                            .role(Role::Button)
                                            .aria_label("Toggle Bluetooth")
                                            .cursor_pointer()
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.toggle_bluetooth(cx);
                                            }))
                                            .w(relative(0.5))
                                            .px_3()
                                            .py_2()
                                            .rounded_xl()
                                            .bg(bt_bg)
                                            .text_color(bt_fg)
                                            .gap_2_5()
                                            .items_center()
                                            .child(Icon::new(IconName::Terminal).size(px(16.)))
                                            .child(div().text_xs().font_bold().child("Bluetooth")),
                                    )
                                    // Night Light Toggle
                                    .child(
                                        h_flex()
                                            .id("cc-toggle-night")
                                            .role(Role::Button)
                                            .aria_label("Toggle Night Light")
                                            .cursor_pointer()
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
                            )
                            .child(
                                h_flex()
                                    .gap_2_5()
                                    // Airplane Mode Toggle
                                    .child(
                                        h_flex()
                                            .id("cc-toggle-airplane")
                                            .role(Role::Button)
                                            .aria_label("Toggle Airplane Mode")
                                            .cursor_pointer()
                                            .on_click(cx.listener({
                                                let airplane_enabled = network.airplane_mode;
                                                move |this, _, _, cx| {
                                                    this.toggle_airplane_mode(
                                                        !airplane_enabled,
                                                        cx,
                                                    );
                                                }
                                            }))
                                            .w(relative(0.5))
                                            .px_3()
                                            .py_2()
                                            .rounded_xl()
                                            .bg(if network.airplane_mode {
                                                cx.theme().primary
                                            } else {
                                                cx.theme().surface_container
                                            })
                                            .text_color(if network.airplane_mode {
                                                cx.theme().on_primary
                                            } else {
                                                cx.theme().on_surface
                                            })
                                            .gap_2_5()
                                            .items_center()
                                            .child(Icon::new(IconName::Sunny).size(px(16.)))
                                            .child(
                                                div().text_xs().font_bold().child("Airplane Mode"),
                                            ),
                                    ),
                            )
                            .child(
                                h_flex()
                                    .gap_2_5()
                                    // Screenshot Button
                                    .child(
                                        h_flex()
                                            .id("cc-capture-screen")
                                            .role(Role::Button)
                                            .aria_label("Take Screenshot")
                                            .cursor_pointer()
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.take_screenshot(window, cx);
                                            }))
                                            .w(relative(0.5))
                                            .px_3()
                                            .py_2()
                                            .rounded_xl()
                                            .bg(cx.theme().surface_container_highest)
                                            .text_color(cx.theme().on_surface_variant)
                                            .gap_2_5()
                                            .items_center()
                                            .child(Icon::new(IconName::ResizeWindow).size(px(16.)))
                                            .child(div().text_xs().font_bold().child("Screenshot")),
                                    )
                                    // Screen Record Button
                                    .child(
                                        h_flex()
                                            .id("cc-toggle-record")
                                            .role(Role::Button)
                                            .aria_label("Record Screen Video")
                                            .cursor_pointer()
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.toggle_recording(window, cx);
                                            }))
                                            .w(relative(0.5))
                                            .px_3()
                                            .py_2()
                                            .rounded_xl()
                                            .bg(record_bg)
                                            .text_color(record_fg)
                                            .gap_2_5()
                                            .items_center()
                                            .child(
                                                Icon::new(if is_recording {
                                                    IconName::Close
                                                } else {
                                                    IconName::PlayArrow
                                                })
                                                .size(px(16.)),
                                            )
                                            .child(div().text_xs().font_bold().child(
                                                if is_recording { "Stop" } else { "Record" },
                                            )),
                                    ),
                            )
                            .when(is_recording, |grid| {
                                grid.child(
                                    h_flex()
                                        .id("cc-recording-status")
                                        .justify_between()
                                        .items_center()
                                        .px_3()
                                        .py_2()
                                        .rounded_xl()
                                        .bg(cx.theme().error_container.opacity(0.7))
                                        .child(div().text_xs().font_bold().child(format!(
                                            "{}  {}",
                                            if is_paused { "Paused" } else { "Recording" },
                                            format_recording_elapsed(
                                                self.recording_state.elapsed()
                                            )
                                        )))
                                        .child(
                                            h_flex()
                                                .id("cc-toggle-recording-pause")
                                                .role(Role::Button)
                                                .aria_label(if is_paused {
                                                    "Resume screen recording"
                                                } else {
                                                    "Pause screen recording"
                                                })
                                                .cursor_pointer()
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.toggle_recording_pause(cx);
                                                }))
                                                .gap_1p5()
                                                .child(
                                                    Icon::new(if is_paused {
                                                        IconName::PlayArrowFill
                                                    } else {
                                                        IconName::PauseFill
                                                    })
                                                    .size(px(15.)),
                                                )
                                                .child(div().text_xs().font_bold().child(
                                                    if is_paused { "Resume" } else { "Pause" },
                                                )),
                                        ),
                                )
                            }),
                    )
                    // Power Profile Selector Pills
                    .child(
                        h_flex().gap_1p5().justify_between().children(
                            [
                                (PowerProfile::PowerSaver, "Saver"),
                                (PowerProfile::Balanced, "Balanced"),
                                (PowerProfile::Performance, "Perf"),
                            ]
                            .into_iter()
                            .enumerate()
                            .map(|(i, (prof, label))| {
                                let is_active = self.active_power_profile == prof;
                                let (bg, fg) = if is_active {
                                    (cx.theme().primary, cx.theme().on_primary)
                                } else {
                                    (
                                        cx.theme().surface_container_highest,
                                        cx.theme().on_surface_variant,
                                    )
                                };
                                let prof_clone = prof.clone();

                                div()
                                    .id(("power-profile-pill", i))
                                    .role(Role::Button)
                                    .aria_label(format!("Select {} Power Profile", label))
                                    .px_3()
                                    .py_1()
                                    .rounded_full()
                                    .bg(bg)
                                    .text_color(fg)
                                    .text_xs()
                                    .font_semibold()
                                    .cursor_pointer()
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.set_power_profile(prof_clone.clone(), cx);
                                    }))
                                    .child(label)
                            }),
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
                    // Audio Devices & Ports Selector
                    .when(audio_available, |this| {
                        let devices = self.device_snapshot.audio_hardware.devices.clone();
                        let ports = self.device_snapshot.audio_hardware.ports.clone();

                        this.child(
                            v_flex()
                                .gap_1p5()
                                .child(
                                    h_flex()
                                        .justify_between()
                                        .items_center()
                                        .child(
                                            div()
                                                .text_xs()
                                                .font_semibold()
                                                .child("Audio Devices & Ports"),
                                        )
                                        .child(
                                            div()
                                                .id("cc-toggle-simultaneous-audio")
                                                .role(Role::Button)
                                                .px_2()
                                                .py_0p5()
                                                .rounded_full()
                                                .bg(cx.theme().primary_container)
                                                .text_color(cx.theme().on_primary_container)
                                                .text_xs()
                                                .font_bold()
                                                .cursor_pointer()
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.toggle_simultaneous_audio(cx);
                                                }))
                                                .child("Simultaneous Audio"),
                                        ),
                                )
                                .child(h_flex().gap_1p5().flex_wrap().children(
                                    devices.into_iter().enumerate().map(|(i, dev)| {
                                        let is_input = dev.is_input;
                                        let dev_id = dev.id.clone();
                                        let label = dev.description;
                                        div()
                                            .id(("audio-dev-pill", i))
                                            .role(Role::Button)
                                            .px_2()
                                            .py_1()
                                            .rounded_md()
                                            .bg(cx.theme().surface_container_highest)
                                            .text_color(cx.theme().on_surface_variant)
                                            .text_xs()
                                            .cursor_pointer()
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.select_audio_device(&dev_id, is_input, cx);
                                            }))
                                            .child(label)
                                    }),
                                ))
                                .when(!ports.is_empty(), |this| {
                                    this.child(h_flex().gap_1p5().flex_wrap().children(
                                        ports.into_iter().enumerate().map(|(i, port)| {
                                            let port_name = port.name.clone();
                                            let label = port.description.clone();
                                            let is_active = port.is_active;
                                            let (bg, fg) = if is_active {
                                                (cx.theme().primary, cx.theme().on_primary)
                                            } else {
                                                (
                                                    cx.theme().surface_container_highest,
                                                    cx.theme().on_surface_variant,
                                                )
                                            };
                                            div()
                                                .id(("audio-port-pill", i))
                                                .role(Role::Button)
                                                .px_2()
                                                .py_1()
                                                .rounded_md()
                                                .bg(bg)
                                                .text_color(fg)
                                                .text_xs()
                                                .cursor_pointer()
                                                .on_click(cx.listener(move |this, _, _, cx| {
                                                    this.select_audio_port(
                                                        "@DEFAULT_SINK@",
                                                        &port_name,
                                                        cx,
                                                    );
                                                }))
                                                .child(label)
                                        }),
                                    ))
                                }),
                        )
                    })
                    // Active VPN Connections Section
                    .when(
                        network.available && !network.active_vpns.is_empty(),
                        |this| {
                            let vpns = network.active_vpns.clone();
                            this.child(
                                v_flex()
                                    .gap_1()
                                    .child(
                                        div()
                                            .text_xs()
                                            .font_semibold()
                                            .child("Active VPN Connections"),
                                    )
                                    .child(h_flex().gap_1p5().flex_wrap().children(
                                        vpns.into_iter().enumerate().map(|(i, vpn)| {
                                            let obj_path = vpn.object_path.clone();
                                            let label = format!("{} ({})", vpn.id, vpn.vpn_type);
                                            div()
                                                .id(("vpn-conn-pill", i))
                                                .role(Role::Button)
                                                .px_2()
                                                .py_1()
                                                .rounded_md()
                                                .bg(cx.theme().primary_container)
                                                .text_color(cx.theme().on_primary_container)
                                                .text_xs()
                                                .font_semibold()
                                                .cursor_pointer()
                                                .on_click(cx.listener(move |this, _, _, cx| {
                                                    this.deactivate_vpn(&obj_path, cx);
                                                }))
                                                .child(label)
                                        }),
                                    )),
                            )
                        },
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
                                    .child(Icon::new(IconName::Terminal).size(px(16.))),
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
                                    .child(Icon::new(IconName::ContentCopy).size(px(16.))),
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
                                    .child(Icon::new(IconName::Lan).size(px(16.))),
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
                    )
                    // Clipboard History Drawer
                    .child(
                        v_flex()
                            .gap_2()
                            .child(
                                h_flex()
                                    .justify_between()
                                    .items_center()
                                    .child(div().text_xs().font_bold().child("Clipboard History")),
                            )
                            .child({
                                let clips = ShellRuntime::clipboard_history(cx);
                                if clips.is_empty() {
                                    div()
                                        .py_2()
                                        .text_xs()
                                        .text_color(cx.theme().on_surface_variant)
                                        .child("No clipboard items copied yet")
                                } else {
                                    v_flex().gap_1_5().children(
                                        clips.into_iter().take(3).enumerate().map(|(i, clip)| {
                                            let text_val = clip.text.clone();
                                            h_flex()
                                                .id(("cc-clip-item", i))
                                                .role(Role::Button)
                                                .aria_label("Copy item")
                                                .cursor_pointer()
                                                .on_click(cx.listener({
                                                    let text_val = text_val.clone();
                                                    move |_, _, _, cx| {
                                                        ShellRuntime::copy_clipboard_text(
                                                            cx, &text_val,
                                                        );
                                                    }
                                                }))
                                                .px_2p5()
                                                .py_1p5()
                                                .rounded_xl()
                                                .bg(cx.theme().surface_container)
                                                .justify_between()
                                                .items_center()
                                                .child(
                                                    div()
                                                        .text_xs()
                                                        .font_medium()
                                                        .text_color(cx.theme().on_surface)
                                                        .child(if text_val.len() > 35 {
                                                            format!("{}...", &text_val[..35])
                                                        } else {
                                                            text_val
                                                        }),
                                                )
                                                .child(
                                                    div()
                                                        .text_xs()
                                                        .text_color(cx.theme().outline_variant)
                                                        .child(clip.timestamp),
                                                )
                                        }),
                                    )
                                }
                            }),
                    )
                    // Workspace Overview Grid
                    .child(
                        v_flex()
                            .gap_2()
                            .child(
                                h_flex()
                                    .justify_between()
                                    .items_center()
                                    .child(div().text_xs().font_bold().child("Workspace Overview")),
                            )
                            .child({
                                let workspaces = ShellRuntime::workspace_overview(cx);
                                if workspaces.is_empty() {
                                    div()
                                        .py_2()
                                        .text_xs()
                                        .text_color(cx.theme().on_surface_variant)
                                        .child("Workspace overview unavailable")
                                } else {
                                    h_flex().gap_2().children(
                                        workspaces.into_iter().enumerate().map(|(i, ws)| {
                                            let ws_id = ws.id;
                                            let is_active = ws.is_active;
                                            let (bg, fg) = if is_active {
                                                (cx.theme().primary, cx.theme().on_primary)
                                            } else {
                                                (
                                                    cx.theme().surface_container,
                                                    cx.theme().on_surface,
                                                )
                                            };
                                            h_flex()
                                                .id(("cc-ws-pill", i))
                                                .role(Role::Button)
                                                .cursor_pointer()
                                                .on_click(cx.listener(move |_, _, _, cx| {
                                                    let _ =
                                                        ShellRuntime::focus_workspace(cx, ws_id);
                                                }))
                                                .px_3()
                                                .py_1p5()
                                                .rounded_xl()
                                                .bg(bg)
                                                .text_color(fg)
                                                .text_xs()
                                                .font_bold()
                                                .child(
                                                    ws.name.unwrap_or_else(|| {
                                                        format!("WS {}", ws.idx)
                                                    }),
                                                )
                                        }),
                                    )
                                }
                            }),
                    )
                    .children(side_panel_entries)
                    .children(extension_entries),
            )
    }
}
