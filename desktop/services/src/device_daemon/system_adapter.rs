use std::sync::{Arc, Mutex};

use super::DeviceAdapter;
use crate::device_protocol::{
    AudioAction, BrightnessAction, CaffeineAction, DeviceCommand, DeviceDomain, DomainLifecycle,
    DomainPayload, DomainState, NightLightAction,
};

/// Adapter that projects the existing Linux integration services through the
/// daemon's typed device protocol.  Services that cannot connect at startup
/// are kept in their offline/degraded mode and can recover independently.
pub struct SystemDeviceAdapter {
    audio: Arc<crate::AudioService>,
    brightness: Arc<crate::BrightnessService>,
    bluetooth: Arc<crate::BluetoothService>,
    network: Arc<crate::NetworkService>,
    night_light: Arc<crate::NightLightService>,
    power_profile: Arc<crate::PowerProfileService>,
    media: Arc<crate::MediaService>,
    battery: Arc<crate::BatteryService>,
    caffeine: Arc<crate::CaffeineService>,
    revisions: Mutex<std::collections::HashMap<DeviceDomain, u64>>,
    last_payloads: Mutex<std::collections::HashMap<DeviceDomain, serde_json::Value>>,
}

impl SystemDeviceAdapter {
    pub fn new() -> Self {
        let audio =
            crate::AudioService::new().unwrap_or_else(|_| crate::AudioService::new_offline());
        let brightness = crate::BrightnessService::new()
            .unwrap_or_else(|_| crate::BrightnessService::new_offline());
        let bluetooth = crate::BluetoothService::new()
            .unwrap_or_else(|_| crate::BluetoothService::new_offline());
        let network =
            crate::NetworkService::new().unwrap_or_else(|_| crate::NetworkService::new_offline());
        let night_light = crate::NightLightService::new()
            .unwrap_or_else(|_| crate::NightLightService::new_offline());
        let media =
            crate::MediaService::new().unwrap_or_else(|_| crate::MediaService::new_offline());
        let battery =
            crate::BatteryService::new().unwrap_or_else(|_| crate::BatteryService::new_offline());
        Self {
            audio: Arc::new(audio),
            brightness: Arc::new(brightness),
            bluetooth: Arc::new(bluetooth),
            network: Arc::new(network),
            night_light: Arc::new(night_light),
            power_profile: Arc::new(crate::PowerProfileService::new()),
            media: Arc::new(media),
            battery: Arc::new(battery),
            caffeine: Arc::new(crate::CaffeineService::new()),
            revisions: Mutex::new(std::collections::HashMap::new()),
            last_payloads: Mutex::new(std::collections::HashMap::new()),
        }
    }

    fn state(&self, domain: DeviceDomain, payload: serde_json::Value) -> DomainState {
        let mut revisions = self.revisions.lock().unwrap();
        let mut payloads = self.last_payloads.lock().unwrap();
        let revision = revisions.entry(domain).or_insert(0);
        if payloads.get(&domain) != Some(&payload) {
            *revision = revision.saturating_add(1);
            payloads.insert(domain, payload.clone());
        }
        let lifecycle = match payload
            .get("available")
            .and_then(serde_json::Value::as_bool)
        {
            Some(true) => DomainLifecycle::Ready,
            Some(false) => DomainLifecycle::Unavailable,
            None => DomainLifecycle::Ready,
        };
        DomainState {
            domain,
            version: crate::device_protocol::DomainVersion::new(1, *revision),
            lifecycle,
            payload: payload_for(domain, payload),
            error: None,
        }
    }
}

fn payload_for(domain: DeviceDomain, payload: serde_json::Value) -> DomainPayload {
    let payload = normalize_for_wire(domain, payload);
    match domain {
        DeviceDomain::Audio => {
            DomainPayload::Audio(serde_json::from_value(payload).unwrap_or_default())
        }
        DeviceDomain::Bluetooth => {
            DomainPayload::Bluetooth(serde_json::from_value(payload).unwrap_or_default())
        }
        DeviceDomain::Brightness => {
            DomainPayload::Brightness(serde_json::from_value(payload).unwrap_or_default())
        }
        DeviceDomain::Network => {
            DomainPayload::Network(serde_json::from_value(payload).unwrap_or_default())
        }
        DeviceDomain::NightLight => {
            DomainPayload::NightLight(crate::device_protocol::NightLightPayload {
                available: payload["available"].as_bool().unwrap_or(false),
                enabled: payload["enabled"].as_bool().unwrap_or(false),
                temperature: payload["temperature"].as_u64().unwrap_or(6500) as u32,
                backend_name: payload["backend_name"]
                    .as_str()
                    .unwrap_or("none")
                    .to_string(),
            })
        }
        DeviceDomain::PowerProfile => {
            DomainPayload::PowerProfile(serde_json::from_value(payload).unwrap_or_default())
        }
        DeviceDomain::Media => {
            let mut payload = payload;
            payload["available"] = serde_json::json!(
                payload["player_id"]
                    .as_str()
                    .is_some_and(|id| !id.is_empty())
            );
            DomainPayload::Media(serde_json::from_value(payload).unwrap_or_default())
        }
        DeviceDomain::Battery => {
            DomainPayload::Battery(serde_json::from_value(payload).unwrap_or_default())
        }
        DeviceDomain::Caffeine => {
            DomainPayload::Caffeine(serde_json::from_value(payload).unwrap_or_default())
        }
    }
}

fn take_optional_string(object: &mut serde_json::Map<String, serde_json::Value>, key: &str) {
    if object.get(key).is_none_or(serde_json::Value::is_null) {
        object.insert(key.to_string(), serde_json::Value::String(String::new()));
    }
}

fn normalize_for_wire(domain: DeviceDomain, mut payload: serde_json::Value) -> serde_json::Value {
    let Some(root) = payload.as_object_mut() else {
        return payload;
    };
    match domain {
        DeviceDomain::Audio => {
            for key in ["sinks", "sources"] {
                if let Some(devices) = root.get_mut(key).and_then(serde_json::Value::as_array_mut) {
                    for device in devices
                        .iter_mut()
                        .filter_map(serde_json::Value::as_object_mut)
                    {
                        take_optional_string(device, "active_port");
                    }
                }
            }
        }
        DeviceDomain::Bluetooth => {
            if let Some(devices) = root
                .get_mut("devices")
                .and_then(serde_json::Value::as_array_mut)
            {
                for device in devices
                    .iter_mut()
                    .filter_map(serde_json::Value::as_object_mut)
                {
                    take_optional_string(device, "icon");
                    for (key, presence) in [
                        ("rssi", "has_rssi"),
                        ("battery_percentage", "has_battery_percentage"),
                    ] {
                        let present = device.get(key).is_some_and(|value| !value.is_null());
                        device.insert(presence.into(), serde_json::Value::Bool(present));
                        if !present {
                            device.insert(key.into(), serde_json::json!(0));
                        }
                    }
                }
            }
            if let Some(count) = root.get("connected_devices_count").and_then(|v| v.as_u64()) {
                root.insert(
                    "connected_devices_count".into(),
                    serde_json::json!(count as u32),
                );
            }
        }
        DeviceDomain::Brightness => {
            take_optional_string(root, "device_name");
            take_optional_string(root, "primary_display_id");
            if let Some(displays) = root
                .get_mut("displays")
                .and_then(serde_json::Value::as_array_mut)
            {
                for display in displays
                    .iter_mut()
                    .filter_map(serde_json::Value::as_object_mut)
                {
                    take_optional_string(display, "connector");
                    let backend = display
                        .get("backend")
                        .map(|value| {
                            value
                                .as_str()
                                .map(str::to_owned)
                                .unwrap_or_else(|| value.to_string())
                        })
                        .unwrap_or_default();
                    display.insert("backend".into(), serde_json::Value::String(backend));
                }
            }
        }
        DeviceDomain::Network => {
            take_optional_string(root, "ssid");
            let has_ip_config = root.get("ip_config").is_some_and(|value| !value.is_null());
            root.insert(
                "has_ip_config".into(),
                serde_json::Value::Bool(has_ip_config),
            );
            if !has_ip_config {
                root.insert(
                    "ip_config".into(),
                    serde_json::json!({
                        "ipv4_address": "", "ipv4_gateway": "",
                        "ipv6_address": "", "ipv6_gateway": "", "dns_servers": []
                    }),
                );
            } else if let Some(ip) = root
                .get_mut("ip_config")
                .and_then(serde_json::Value::as_object_mut)
            {
                for key in [
                    "ipv4_address",
                    "ipv4_gateway",
                    "ipv6_address",
                    "ipv6_gateway",
                ] {
                    take_optional_string(ip, key);
                }
            }
        }
        _ => {}
    }
    payload
}

impl Default for SystemDeviceAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl DeviceAdapter for SystemDeviceAdapter {
    fn name(&self) -> &'static str {
        "linux-services"
    }

    fn get_domain_state(&self, domain: DeviceDomain) -> DomainState {
        let payload = match domain {
            DeviceDomain::Audio => {
                serde_json::to_value(self.audio.audio_info()).unwrap_or_default()
            }
            DeviceDomain::Brightness => {
                serde_json::to_value(self.brightness.brightness_info()).unwrap_or_default()
            }
            DeviceDomain::Bluetooth => {
                serde_json::to_value(self.bluetooth.info()).unwrap_or_default()
            }
            DeviceDomain::Network => {
                serde_json::to_value(self.network.network_info()).unwrap_or_default()
            }
            DeviceDomain::NightLight => {
                serde_json::json!({"available": self.night_light.info().available, "enabled": self.night_light.info().is_active, "temperature": self.night_light.info().temperature_kelvin})
            }
            DeviceDomain::PowerProfile => {
                serde_json::json!({"available": self.power_profile.info().available, "profile": self.power_profile.info().active_profile.as_str()})
            }
            DeviceDomain::Media => {
                serde_json::to_value(self.media.media_info()).unwrap_or_default()
            }
            DeviceDomain::Battery => {
                serde_json::to_value(self.battery.battery_info()).unwrap_or_default()
            }
            DeviceDomain::Caffeine => {
                serde_json::json!({"available": true, "enabled": self.caffeine.info().active})
            }
        };
        self.state(domain, payload)
    }

    fn execute_command(
        &self,
        command: DeviceCommand,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<DomainState, String>> + Send + 'static>,
    > {
        let domain = command.domain();
        let result = match command {
            DeviceCommand::Audio(action) => {
                let result: Result<(), String> = match action {
                    AudioAction::SetVolume(v) => {
                        self.audio.set_volume(v);
                        Ok(())
                    }
                    AudioAction::SetMuted(m) => {
                        if m != self.audio.audio_info().is_muted {
                            self.audio.toggle_mute();
                            Ok(())
                        } else {
                            Ok(())
                        }
                    }
                    AudioAction::ToggleMute => {
                        self.audio.toggle_mute();
                        Ok(())
                    }
                };
                result
            }
            DeviceCommand::Brightness(action) => {
                let result: Result<(), String> = match action {
                    BrightnessAction::SetBrightness(v) => {
                        self.brightness.set_brightness(v);
                        Ok(())
                    }
                    BrightnessAction::SetDisplay { id, percentage } => {
                        self.brightness.set_display_brightness(&id, percentage);
                        Ok(())
                    }
                    BrightnessAction::StepUp => {
                        self.brightness.set_brightness(
                            (self.brightness.brightness_info().percentage + 10).min(100),
                        );
                        Ok(())
                    }
                    BrightnessAction::StepDown => {
                        self.brightness.set_brightness(
                            self.brightness
                                .brightness_info()
                                .percentage
                                .saturating_sub(10),
                        );
                        Ok(())
                    }
                };
                result
            }
            DeviceCommand::NightLight(action) => {
                let ok = match action {
                    NightLightAction::SetEnabled(v) => self.night_light.set_active(v),
                    NightLightAction::ToggleEnabled => self.night_light.toggle(),
                    NightLightAction::SetTemperature(v) => self.night_light.set_temperature(v),
                };
                if ok {
                    Ok(())
                } else {
                    Err("night light unavailable".into())
                }
            }
            DeviceCommand::Caffeine(action) => {
                match action {
                    CaffeineAction::SetEnabled(v) => {
                        self.caffeine.set_active(v);
                    }
                    CaffeineAction::Toggle => {
                        self.caffeine.toggle();
                    }
                };
                Ok(())
            }
            DeviceCommand::Bluetooth(action) => {
                use crate::device_protocol::BluetoothAction;
                match action {
                    BluetoothAction::SetPowered(v) => {
                        self.bluetooth.set_powered(v).map_err(|e| e.to_string())
                    }
                    BluetoothAction::TogglePowered => self
                        .bluetooth
                        .toggle()
                        .map(|_| ())
                        .map_err(|e| e.to_string()),
                    BluetoothAction::Connect(a) => {
                        self.bluetooth.connect_device(a).map_err(|e| e.to_string())
                    }
                    BluetoothAction::Disconnect(a) => self
                        .bluetooth
                        .disconnect_device(a)
                        .map_err(|e| e.to_string()),
                }
            }
            DeviceCommand::Network(action) => {
                use crate::device_protocol::NetworkAction;
                match action {
                    NetworkAction::SetWifiEnabled(v) => {
                        self.network.set_wifi_enabled(v).map_err(|e| e.to_string())
                    }
                    NetworkAction::ToggleWifi => self
                        .network
                        .set_wifi_enabled(!self.network.network_info().wifi_enabled)
                        .map_err(|e| e.to_string()),
                    NetworkAction::ConnectWifi(ssid) => self
                        .network
                        .connect_wifi(&ssid, None)
                        .map_err(|e| e.to_string()),
                    NetworkAction::ConnectVpn(name) => {
                        self.network.connect_vpn(&name).map_err(|e| e.to_string())
                    }
                    NetworkAction::DisconnectVpn(name) => self
                        .network
                        .disconnect_vpn(&name)
                        .map_err(|e| e.to_string()),
                }
            }
            DeviceCommand::PowerProfile(action) => {
                let crate::device_protocol::PowerProfileAction::SetProfile(profile) = action;
                self.power_profile
                    .set_profile(crate::PowerProfile::parse(&profile))
                    .map_err(|e| e.to_string())
            }
            DeviceCommand::Media(action) => {
                use crate::device_protocol::MediaAction;
                let command = match action {
                    MediaAction::PlayPause => crate::MediaCommand::PlayPause,
                    MediaAction::Next => crate::MediaCommand::Next,
                    MediaAction::Play | MediaAction::Pause | MediaAction::Previous => {
                        return Box::pin(async {
                            Err("requested media command is unsupported by MediaService".into())
                        });
                    }
                };
                self.media.send_command(command).map_err(|e| e.to_string())
            }
        };
        let state = result.map(|_| self.get_domain_state(domain));
        Box::pin(async move { state })
    }
}
