use gpui::{
    App, AppContext, Context, Entity, FocusHandle, Focusable, IntoElement, ParentElement, Render,
    Styled, Window,
};
use shilpo_ui::{
    BluetoothWidget, NetworkWidget, button::Button, dock::PanelControl, h_flex, v_flex,
};

use crate::section;

pub struct NetworkBluetoothStory {
    focus_handle: FocusHandle,
    wifi_enabled: bool,
    wifi_connected: bool,
    bluetooth_powered: bool,
}

impl NetworkBluetoothStory {
    fn new(_: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            wifi_enabled: true,
            wifi_connected: true,
            bluetooth_powered: true,
        }
    }

    pub fn view(window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self::new(window, cx))
    }
}

impl super::Story for NetworkBluetoothStory {
    fn title() -> &'static str {
        "Wi-Fi & Bluetooth Icons"
    }

    fn description() -> &'static str {
        "Status bar connectivity icons for Wi-Fi signal level and Bluetooth status."
    }

    fn new_view(window: &mut Window, cx: &mut App) -> Entity<impl Render> {
        Self::view(window, cx)
    }

    fn zoomable() -> Option<PanelControl> {
        None
    }
}

impl Focusable for NetworkBluetoothStory {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for NetworkBluetoothStory {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let entity = cx.entity().clone();

        v_flex()
            .gap_6()
            .child(
                section("Status Bar Icons Preview").max_w_xl().child(
                    h_flex()
                        .gap_4()
                        .items_center()
                        .child(NetworkWidget::new(
                            "wifi-preview",
                            self.wifi_enabled,
                            self.wifi_connected,
                        ))
                        .child(BluetoothWidget::new(
                            "bt-preview",
                            self.bluetooth_powered,
                            false,
                        )),
                ),
            )
            .child(
                section("Connectivity State Controls").max_w_xl().child(
                    v_flex().gap_4().child(
                        h_flex()
                            .gap_2()
                            .child(
                                Button::new("toggle-wifi-on")
                                    .label(if self.wifi_enabled {
                                        "Wi-Fi: ON"
                                    } else {
                                        "Wi-Fi: OFF"
                                    })
                                    .on_click({
                                        let entity = entity.clone();
                                        move |_, _, cx| {
                                            entity.update(cx, |this, cx| {
                                                this.wifi_enabled = !this.wifi_enabled;
                                                cx.notify();
                                            });
                                        }
                                    }),
                            )
                            .child(
                                Button::new("toggle-wifi-connect")
                                    .label(if self.wifi_connected {
                                        "Connected"
                                    } else {
                                        "Disconnected"
                                    })
                                    .on_click({
                                        let entity = entity.clone();
                                        move |_, _, cx| {
                                            entity.update(cx, |this, cx| {
                                                this.wifi_connected = !this.wifi_connected;
                                                cx.notify();
                                            });
                                        }
                                    }),
                            )
                            .child(
                                Button::new("toggle-bt")
                                    .label(if self.bluetooth_powered {
                                        "Bluetooth: ON"
                                    } else {
                                        "Bluetooth: OFF"
                                    })
                                    .on_click({
                                        let entity = entity.clone();
                                        move |_, _, cx| {
                                            entity.update(cx, |this, cx| {
                                                this.bluetooth_powered = !this.bluetooth_powered;
                                                cx.notify();
                                            });
                                        }
                                    }),
                            ),
                    ),
                ),
            )
    }
}
