use super::DeviceDaemonService;
use shilpo_device_protocol::{
    AudioPayload, BatteryPayload, BluetoothPayload, BrightnessPayload, CaffeinePayload,
    DeviceCommand, DeviceDomain, DomainLifecycle, DomainPayload, DomainState, MediaPayload,
    NetworkPayload, NightLightPayload, PROTOCOL_VERSION, PowerProfilePayload,
};
use std::sync::Arc;
use zbus::object_server::SignalEmitter;

fn lifecycle_code(lifecycle: DomainLifecycle) -> u8 {
    match lifecycle {
        DomainLifecycle::Unavailable => 0,
        DomainLifecycle::Connecting => 1,
        DomainLifecycle::Ready => 2,
        DomainLifecycle::Reconnecting => 3,
        DomainLifecycle::Degraded => 4,
    }
}

fn error_text(state: &DomainState) -> String {
    state.error.clone().unwrap_or_default()
}

#[derive(Clone)]
pub struct DeviceDbusService {
    daemon: Arc<DeviceDaemonService>,
}

impl DeviceDbusService {
    pub fn new(daemon: Arc<DeviceDaemonService>) -> Self {
        Self { daemon }
    }

    pub async fn emit_outcome(
        &self,
        emitter: &SignalEmitter<'_>,
        outcome: &shilpo_device_protocol::CommandOutcome,
    ) -> zbus::Result<()> {
        Self::command_reconciled(emitter, outcome.clone().into()).await
    }

    pub async fn emit_updates(&self, emitter: &SignalEmitter<'_>) -> zbus::Result<()> {
        for state in self.daemon.refresh_domain_states() {
            Self::emit_state(emitter, state).await?;
        }
        Ok(())
    }

    async fn emit_state(emitter: &SignalEmitter<'_>, state: DomainState) -> zbus::Result<()> {
        let revision = state.revision;
        let lifecycle = lifecycle_code(state.lifecycle);
        let error = error_text(&state);
        match state.payload {
            DomainPayload::Audio(payload) => {
                Self::audio_state_changed(emitter, revision, lifecycle, payload, &error).await?
            }
            DomainPayload::Bluetooth(payload) => {
                Self::bluetooth_state_changed(emitter, revision, lifecycle, payload, &error).await?
            }
            DomainPayload::Brightness(payload) => {
                Self::brightness_state_changed(emitter, revision, lifecycle, payload, &error)
                    .await?
            }
            DomainPayload::Network(payload) => {
                Self::network_state_changed(emitter, revision, lifecycle, payload, &error).await?
            }
            DomainPayload::NightLight(payload) => {
                Self::night_light_state_changed(emitter, revision, lifecycle, payload, &error)
                    .await?
            }
            DomainPayload::PowerProfile(payload) => {
                Self::power_profile_state_changed(emitter, revision, lifecycle, payload, &error)
                    .await?
            }
            DomainPayload::Media(payload) => {
                Self::media_state_changed(emitter, revision, lifecycle, payload, &error).await?
            }
            DomainPayload::Battery(payload) => {
                Self::battery_state_changed(emitter, revision, lifecycle, payload, &error).await?
            }
            DomainPayload::Caffeine(payload) => {
                Self::caffeine_state_changed(emitter, revision, lifecycle, payload, &error).await?
            }
        }
        Ok(())
    }

    async fn submit_typed(
        &self,
        command: DeviceCommand,
        client_protocol_version: u32,
        emitter: &SignalEmitter<'_>,
    ) -> zbus::fdo::Result<shilpo_device_protocol::CommandOutcomeRecord> {
        let domain = command.domain();
        let (_, outcome_rx) = self
            .daemon
            .submit_command(command, client_protocol_version)
            .map_err(zbus::fdo::Error::Failed)?;
        let outcome = outcome_rx
            .await
            .map_err(|_| zbus::fdo::Error::Failed("command worker dropped".into()))?;
        if let Some(state) = self.daemon.domain_state_if_revision(domain, &outcome) {
            Self::emit_state(emitter, state)
                .await
                .map_err(|error| zbus::fdo::Error::Failed(error.to_string()))?;
        }
        Ok(outcome.into())
    }
}

#[zbus::interface(name = "org.shilpo.Device")]
impl DeviceDbusService {
    async fn get_protocol_version(&self) -> u32 {
        PROTOCOL_VERSION
    }

    async fn get_audio_state(&self) -> zbus::fdo::Result<(u64, u8, AudioPayload, String)> {
        typed_state(self.daemon.get_domain_state(DeviceDomain::Audio), |p| {
            if let DomainPayload::Audio(v) = p {
                Some(v)
            } else {
                None
            }
        })
    }
    async fn get_bluetooth_state(&self) -> zbus::fdo::Result<(u64, u8, BluetoothPayload, String)> {
        typed_state(self.daemon.get_domain_state(DeviceDomain::Bluetooth), |p| {
            if let DomainPayload::Bluetooth(v) = p {
                Some(v)
            } else {
                None
            }
        })
    }
    async fn get_brightness_state(
        &self,
    ) -> zbus::fdo::Result<(u64, u8, BrightnessPayload, String)> {
        typed_state(
            self.daemon.get_domain_state(DeviceDomain::Brightness),
            |p| {
                if let DomainPayload::Brightness(v) = p {
                    Some(v)
                } else {
                    None
                }
            },
        )
    }
    async fn get_network_state(&self) -> zbus::fdo::Result<(u64, u8, NetworkPayload, String)> {
        typed_state(self.daemon.get_domain_state(DeviceDomain::Network), |p| {
            if let DomainPayload::Network(v) = p {
                Some(v)
            } else {
                None
            }
        })
    }
    async fn get_night_light_state(
        &self,
    ) -> zbus::fdo::Result<(u64, u8, NightLightPayload, String)> {
        typed_state(
            self.daemon.get_domain_state(DeviceDomain::NightLight),
            |p| {
                if let DomainPayload::NightLight(v) = p {
                    Some(v)
                } else {
                    None
                }
            },
        )
    }
    async fn get_power_profile_state(
        &self,
    ) -> zbus::fdo::Result<(u64, u8, PowerProfilePayload, String)> {
        typed_state(
            self.daemon.get_domain_state(DeviceDomain::PowerProfile),
            |p| {
                if let DomainPayload::PowerProfile(v) = p {
                    Some(v)
                } else {
                    None
                }
            },
        )
    }
    async fn get_media_state(&self) -> zbus::fdo::Result<(u64, u8, MediaPayload, String)> {
        typed_state(self.daemon.get_domain_state(DeviceDomain::Media), |p| {
            if let DomainPayload::Media(v) = p {
                Some(v)
            } else {
                None
            }
        })
    }
    async fn get_battery_state(&self) -> zbus::fdo::Result<(u64, u8, BatteryPayload, String)> {
        typed_state(self.daemon.get_domain_state(DeviceDomain::Battery), |p| {
            if let DomainPayload::Battery(v) = p {
                Some(v)
            } else {
                None
            }
        })
    }
    async fn get_caffeine_state(&self) -> zbus::fdo::Result<(u64, u8, CaffeinePayload, String)> {
        typed_state(self.daemon.get_domain_state(DeviceDomain::Caffeine), |p| {
            if let DomainPayload::Caffeine(v) = p {
                Some(v)
            } else {
                None
            }
        })
    }

    async fn set_audio_volume(
        &self,
        value: u8,
        version: u32,
        #[zbus(signal_emitter)] e: SignalEmitter<'_>,
    ) -> zbus::fdo::Result<shilpo_device_protocol::CommandOutcomeRecord> {
        self.submit_typed(
            DeviceCommand::Audio(shilpo_device_protocol::AudioAction::SetVolume(value)),
            version,
            &e,
        )
        .await
    }
    async fn set_audio_muted(
        &self,
        value: bool,
        version: u32,
        #[zbus(signal_emitter)] e: SignalEmitter<'_>,
    ) -> zbus::fdo::Result<shilpo_device_protocol::CommandOutcomeRecord> {
        self.submit_typed(
            DeviceCommand::Audio(shilpo_device_protocol::AudioAction::SetMuted(value)),
            version,
            &e,
        )
        .await
    }
    async fn toggle_audio_mute(
        &self,
        version: u32,
        #[zbus(signal_emitter)] e: SignalEmitter<'_>,
    ) -> zbus::fdo::Result<shilpo_device_protocol::CommandOutcomeRecord> {
        self.submit_typed(
            DeviceCommand::Audio(shilpo_device_protocol::AudioAction::ToggleMute),
            version,
            &e,
        )
        .await
    }
    async fn set_bluetooth_powered(
        &self,
        value: bool,
        version: u32,
        #[zbus(signal_emitter)] e: SignalEmitter<'_>,
    ) -> zbus::fdo::Result<shilpo_device_protocol::CommandOutcomeRecord> {
        self.submit_typed(
            DeviceCommand::Bluetooth(shilpo_device_protocol::BluetoothAction::SetPowered(value)),
            version,
            &e,
        )
        .await
    }
    async fn toggle_bluetooth_powered(
        &self,
        version: u32,
        #[zbus(signal_emitter)] e: SignalEmitter<'_>,
    ) -> zbus::fdo::Result<shilpo_device_protocol::CommandOutcomeRecord> {
        self.submit_typed(
            DeviceCommand::Bluetooth(shilpo_device_protocol::BluetoothAction::TogglePowered),
            version,
            &e,
        )
        .await
    }
    async fn connect_bluetooth(
        &self,
        address: String,
        version: u32,
        #[zbus(signal_emitter)] e: SignalEmitter<'_>,
    ) -> zbus::fdo::Result<shilpo_device_protocol::CommandOutcomeRecord> {
        self.submit_typed(
            DeviceCommand::Bluetooth(shilpo_device_protocol::BluetoothAction::Connect(address)),
            version,
            &e,
        )
        .await
    }
    async fn disconnect_bluetooth(
        &self,
        address: String,
        version: u32,
        #[zbus(signal_emitter)] e: SignalEmitter<'_>,
    ) -> zbus::fdo::Result<shilpo_device_protocol::CommandOutcomeRecord> {
        self.submit_typed(
            DeviceCommand::Bluetooth(shilpo_device_protocol::BluetoothAction::Disconnect(address)),
            version,
            &e,
        )
        .await
    }
    async fn set_brightness(
        &self,
        value: u8,
        version: u32,
        #[zbus(signal_emitter)] e: SignalEmitter<'_>,
    ) -> zbus::fdo::Result<shilpo_device_protocol::CommandOutcomeRecord> {
        self.submit_typed(
            DeviceCommand::Brightness(shilpo_device_protocol::BrightnessAction::SetBrightness(
                value,
            )),
            version,
            &e,
        )
        .await
    }
    async fn step_brightness_up(
        &self,
        version: u32,
        #[zbus(signal_emitter)] e: SignalEmitter<'_>,
    ) -> zbus::fdo::Result<shilpo_device_protocol::CommandOutcomeRecord> {
        self.submit_typed(
            DeviceCommand::Brightness(shilpo_device_protocol::BrightnessAction::StepUp),
            version,
            &e,
        )
        .await
    }
    async fn step_brightness_down(
        &self,
        version: u32,
        #[zbus(signal_emitter)] e: SignalEmitter<'_>,
    ) -> zbus::fdo::Result<shilpo_device_protocol::CommandOutcomeRecord> {
        self.submit_typed(
            DeviceCommand::Brightness(shilpo_device_protocol::BrightnessAction::StepDown),
            version,
            &e,
        )
        .await
    }
    async fn set_wifi_enabled(
        &self,
        value: bool,
        version: u32,
        #[zbus(signal_emitter)] e: SignalEmitter<'_>,
    ) -> zbus::fdo::Result<shilpo_device_protocol::CommandOutcomeRecord> {
        self.submit_typed(
            DeviceCommand::Network(shilpo_device_protocol::NetworkAction::SetWifiEnabled(value)),
            version,
            &e,
        )
        .await
    }
    async fn toggle_wifi(
        &self,
        version: u32,
        #[zbus(signal_emitter)] e: SignalEmitter<'_>,
    ) -> zbus::fdo::Result<shilpo_device_protocol::CommandOutcomeRecord> {
        self.submit_typed(
            DeviceCommand::Network(shilpo_device_protocol::NetworkAction::ToggleWifi),
            version,
            &e,
        )
        .await
    }
    async fn connect_wifi(
        &self,
        ssid: String,
        version: u32,
        #[zbus(signal_emitter)] e: SignalEmitter<'_>,
    ) -> zbus::fdo::Result<shilpo_device_protocol::CommandOutcomeRecord> {
        self.submit_typed(
            DeviceCommand::Network(shilpo_device_protocol::NetworkAction::ConnectWifi(ssid)),
            version,
            &e,
        )
        .await
    }
    async fn connect_vpn(
        &self,
        name: String,
        version: u32,
        #[zbus(signal_emitter)] e: SignalEmitter<'_>,
    ) -> zbus::fdo::Result<shilpo_device_protocol::CommandOutcomeRecord> {
        self.submit_typed(
            DeviceCommand::Network(shilpo_device_protocol::NetworkAction::ConnectVpn(name)),
            version,
            &e,
        )
        .await
    }
    async fn disconnect_vpn(
        &self,
        name: String,
        version: u32,
        #[zbus(signal_emitter)] e: SignalEmitter<'_>,
    ) -> zbus::fdo::Result<shilpo_device_protocol::CommandOutcomeRecord> {
        self.submit_typed(
            DeviceCommand::Network(shilpo_device_protocol::NetworkAction::DisconnectVpn(name)),
            version,
            &e,
        )
        .await
    }
    async fn set_night_light_enabled(
        &self,
        value: bool,
        version: u32,
        #[zbus(signal_emitter)] e: SignalEmitter<'_>,
    ) -> zbus::fdo::Result<shilpo_device_protocol::CommandOutcomeRecord> {
        self.submit_typed(
            DeviceCommand::NightLight(shilpo_device_protocol::NightLightAction::SetEnabled(value)),
            version,
            &e,
        )
        .await
    }
    async fn toggle_night_light(
        &self,
        version: u32,
        #[zbus(signal_emitter)] e: SignalEmitter<'_>,
    ) -> zbus::fdo::Result<shilpo_device_protocol::CommandOutcomeRecord> {
        self.submit_typed(
            DeviceCommand::NightLight(shilpo_device_protocol::NightLightAction::ToggleEnabled),
            version,
            &e,
        )
        .await
    }
    async fn set_night_light_temperature(
        &self,
        value: u32,
        version: u32,
        #[zbus(signal_emitter)] e: SignalEmitter<'_>,
    ) -> zbus::fdo::Result<shilpo_device_protocol::CommandOutcomeRecord> {
        self.submit_typed(
            DeviceCommand::NightLight(shilpo_device_protocol::NightLightAction::SetTemperature(
                value,
            )),
            version,
            &e,
        )
        .await
    }
    async fn set_power_profile(
        &self,
        profile: String,
        version: u32,
        #[zbus(signal_emitter)] e: SignalEmitter<'_>,
    ) -> zbus::fdo::Result<shilpo_device_protocol::CommandOutcomeRecord> {
        self.submit_typed(
            DeviceCommand::PowerProfile(shilpo_device_protocol::PowerProfileAction::SetProfile(
                profile,
            )),
            version,
            &e,
        )
        .await
    }
    async fn media_play(
        &self,
        version: u32,
        #[zbus(signal_emitter)] e: SignalEmitter<'_>,
    ) -> zbus::fdo::Result<shilpo_device_protocol::CommandOutcomeRecord> {
        self.submit_typed(
            DeviceCommand::Media(shilpo_device_protocol::MediaAction::Play),
            version,
            &e,
        )
        .await
    }
    async fn media_pause(
        &self,
        version: u32,
        #[zbus(signal_emitter)] e: SignalEmitter<'_>,
    ) -> zbus::fdo::Result<shilpo_device_protocol::CommandOutcomeRecord> {
        self.submit_typed(
            DeviceCommand::Media(shilpo_device_protocol::MediaAction::Pause),
            version,
            &e,
        )
        .await
    }
    async fn media_play_pause(
        &self,
        version: u32,
        #[zbus(signal_emitter)] e: SignalEmitter<'_>,
    ) -> zbus::fdo::Result<shilpo_device_protocol::CommandOutcomeRecord> {
        self.submit_typed(
            DeviceCommand::Media(shilpo_device_protocol::MediaAction::PlayPause),
            version,
            &e,
        )
        .await
    }
    async fn media_next(
        &self,
        version: u32,
        #[zbus(signal_emitter)] e: SignalEmitter<'_>,
    ) -> zbus::fdo::Result<shilpo_device_protocol::CommandOutcomeRecord> {
        self.submit_typed(
            DeviceCommand::Media(shilpo_device_protocol::MediaAction::Next),
            version,
            &e,
        )
        .await
    }
    async fn media_previous(
        &self,
        version: u32,
        #[zbus(signal_emitter)] e: SignalEmitter<'_>,
    ) -> zbus::fdo::Result<shilpo_device_protocol::CommandOutcomeRecord> {
        self.submit_typed(
            DeviceCommand::Media(shilpo_device_protocol::MediaAction::Previous),
            version,
            &e,
        )
        .await
    }
    async fn set_caffeine_enabled(
        &self,
        value: bool,
        version: u32,
        #[zbus(signal_emitter)] e: SignalEmitter<'_>,
    ) -> zbus::fdo::Result<shilpo_device_protocol::CommandOutcomeRecord> {
        self.submit_typed(
            DeviceCommand::Caffeine(shilpo_device_protocol::CaffeineAction::SetEnabled(value)),
            version,
            &e,
        )
        .await
    }
    async fn toggle_caffeine(
        &self,
        version: u32,
        #[zbus(signal_emitter)] e: SignalEmitter<'_>,
    ) -> zbus::fdo::Result<shilpo_device_protocol::CommandOutcomeRecord> {
        self.submit_typed(
            DeviceCommand::Caffeine(shilpo_device_protocol::CaffeineAction::Toggle),
            version,
            &e,
        )
        .await
    }

    #[zbus(signal)]
    async fn audio_state_changed(
        signal_emitter: &SignalEmitter<'_>,
        revision: u64,
        lifecycle: u8,
        payload: AudioPayload,
        error: &str,
    ) -> zbus::Result<()>;
    #[zbus(signal)]
    async fn bluetooth_state_changed(
        signal_emitter: &SignalEmitter<'_>,
        revision: u64,
        lifecycle: u8,
        payload: BluetoothPayload,
        error: &str,
    ) -> zbus::Result<()>;
    #[zbus(signal)]
    async fn brightness_state_changed(
        signal_emitter: &SignalEmitter<'_>,
        revision: u64,
        lifecycle: u8,
        payload: BrightnessPayload,
        error: &str,
    ) -> zbus::Result<()>;
    #[zbus(signal)]
    async fn network_state_changed(
        signal_emitter: &SignalEmitter<'_>,
        revision: u64,
        lifecycle: u8,
        payload: NetworkPayload,
        error: &str,
    ) -> zbus::Result<()>;
    #[zbus(signal)]
    async fn night_light_state_changed(
        signal_emitter: &SignalEmitter<'_>,
        revision: u64,
        lifecycle: u8,
        payload: NightLightPayload,
        error: &str,
    ) -> zbus::Result<()>;
    #[zbus(signal)]
    async fn power_profile_state_changed(
        signal_emitter: &SignalEmitter<'_>,
        revision: u64,
        lifecycle: u8,
        payload: PowerProfilePayload,
        error: &str,
    ) -> zbus::Result<()>;
    #[zbus(signal)]
    async fn media_state_changed(
        signal_emitter: &SignalEmitter<'_>,
        revision: u64,
        lifecycle: u8,
        payload: MediaPayload,
        error: &str,
    ) -> zbus::Result<()>;
    #[zbus(signal)]
    async fn battery_state_changed(
        signal_emitter: &SignalEmitter<'_>,
        revision: u64,
        lifecycle: u8,
        payload: BatteryPayload,
        error: &str,
    ) -> zbus::Result<()>;
    #[zbus(signal)]
    async fn caffeine_state_changed(
        signal_emitter: &SignalEmitter<'_>,
        revision: u64,
        lifecycle: u8,
        payload: CaffeinePayload,
        error: &str,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn command_reconciled(
        signal_emitter: &SignalEmitter<'_>,
        outcome: shilpo_device_protocol::CommandOutcomeRecord,
    ) -> zbus::Result<()>;
}

fn typed_state<T>(
    state: DomainState,
    take: impl FnOnce(DomainPayload) -> Option<T>,
) -> zbus::fdo::Result<(u64, u8, T, String)> {
    let revision = state.revision;
    let lifecycle = lifecycle_code(state.lifecycle);
    let error = error_text(&state);
    let payload = take(state.payload)
        .ok_or_else(|| zbus::fdo::Error::Failed("device domain payload mismatch".into()))?;
    Ok((revision, lifecycle, payload, error))
}
