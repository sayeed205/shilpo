use futures_lite::StreamExt;
use shilpo_device_protocol::{
    CommandOutcome, DeviceCommand, DeviceDomain, DomainLifecycle, DomainPayload, DomainState,
    PROTOCOL_VERSION,
};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;
use tokio::sync::broadcast;

#[derive(Clone, Debug)]
pub struct DeviceClientUpdate {
    pub domain: DeviceDomain,
    pub state: DomainState,
}

#[derive(Clone)]
pub struct DeviceClient {
    domains: Arc<RwLock<HashMap<DeviceDomain, DomainState>>>,
    update_tx: broadcast::Sender<DeviceClientUpdate>,
    outcome_tx: broadcast::Sender<CommandOutcome>,
    connection: Arc<Mutex<Option<zbus::Connection>>>,
    debounce_tx: tokio::sync::mpsc::UnboundedSender<(DeviceCommand, Duration)>,
}

#[derive(Default)]
struct DebouncedCommands {
    pending: HashMap<DeviceDomain, (DeviceCommand, tokio::time::Instant)>,
}

impl DebouncedCommands {
    fn replace(&mut self, command: DeviceCommand, deadline: tokio::time::Instant) {
        self.pending.insert(command.domain(), (command, deadline));
    }

    fn take_due(&mut self, now: tokio::time::Instant) -> Vec<DeviceCommand> {
        let due = self
            .pending
            .iter()
            .filter_map(|(domain, (_, deadline))| (*deadline <= now).then_some(*domain))
            .collect::<Vec<_>>();
        due.into_iter()
            .filter_map(|domain| self.pending.remove(&domain).map(|(command, _)| command))
            .collect()
    }

    fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }
}

impl DeviceClient {
    pub fn spawn_command(client: Self, command: DeviceCommand) {
        tokio::spawn(async move {
            let _ = client.send_command(command).await;
        });
    }

    pub fn new() -> Self {
        let (update_tx, _) = broadcast::channel(128);
        let (outcome_tx, _) = broadcast::channel(128);
        let domains = Arc::new(RwLock::new(unavailable_domains()));
        let connection = Arc::new(Mutex::new(None));
        let (debounce_tx, mut debounce_rx) =
            tokio::sync::mpsc::unbounded_channel::<(DeviceCommand, Duration)>();

        let client = Self {
            domains,
            update_tx,
            outcome_tx,
            connection,
            debounce_tx,
        };
        let debounce_client = client.clone();
        tokio::spawn(async move {
            let mut pending = DebouncedCommands::default();
            loop {
                tokio::select! {
                    Some((command, delay)) = debounce_rx.recv() => {
                        pending.replace(command, tokio::time::Instant::now() + delay);
                    }
                    _ = tokio::time::sleep(Duration::from_millis(20)), if !pending.is_empty() => {
                        for command in pending.take_due(tokio::time::Instant::now()) {
                            let _ = debounce_client.send_command(command).await;
                        }
                    }
                    else => break,
                }
            }
        });
        client
    }

    pub async fn connect(&self) -> Result<(), String> {
        let connection = zbus::Connection::session()
            .await
            .map_err(|error| format!("device daemon unavailable: {error}"))?;
        self.connect_on(connection).await
    }

    /// Connects through an already-established transport. This is primarily
    /// useful for deterministic peer-to-peer integration tests.
    pub async fn connect_on(&self, connection: zbus::Connection) -> Result<(), String> {
        let proxy = DeviceDbusProxy::builder(&connection)
            .build()
            .await
            .map_err(|error| format!("device daemon proxy unavailable: {error}"))?;
        let version = proxy
            .get_protocol_version()
            .await
            .map_err(|error| format!("device protocol negotiation failed: {error}"))?;
        if version != PROTOCOL_VERSION {
            return Err(format!(
                "device protocol mismatch: client {PROTOCOL_VERSION}, daemon {version}; restart or update both components"
            ));
        }
        macro_rules! load_state {
            ($method:ident, $domain:expr, $variant:ident) => {
                if let Ok((revision, lifecycle, payload, error)) = proxy.$method().await {
                    self.update_local_domain_state(state_from_wire(
                        $domain,
                        revision,
                        lifecycle,
                        DomainPayload::$variant(payload),
                        error,
                    ));
                }
            };
        }
        load_state!(get_audio_state, DeviceDomain::Audio, Audio);
        load_state!(get_bluetooth_state, DeviceDomain::Bluetooth, Bluetooth);
        load_state!(get_brightness_state, DeviceDomain::Brightness, Brightness);
        load_state!(get_network_state, DeviceDomain::Network, Network);
        load_state!(get_night_light_state, DeviceDomain::NightLight, NightLight);
        load_state!(
            get_power_profile_state,
            DeviceDomain::PowerProfile,
            PowerProfile
        );
        load_state!(get_media_state, DeviceDomain::Media, Media);
        load_state!(get_battery_state, DeviceDomain::Battery, Battery);
        load_state!(get_caffeine_state, DeviceDomain::Caffeine, Caffeine);
        *self.connection.lock().unwrap() = Some(connection.clone());
        let closed_client = self.clone();
        let closed_connection = connection.clone();
        tokio::spawn(async move {
            closed_connection.closed().await;
            *closed_client.connection.lock().unwrap() = None;
            closed_client.mark_connection_lost("device daemon connection closed");
        });
        let outcome_listener = self.clone();
        let outcome_connection = connection.clone();
        tokio::spawn(async move {
            let Ok(proxy) = DeviceDbusProxy::builder(&outcome_connection).build().await else {
                return;
            };
            let Ok(mut signals) = proxy.receive_command_reconciled().await else {
                return;
            };
            while let Some(signal) = signals.next().await {
                if let Ok(args) = signal.args()
                    && let Ok(outcome) = CommandOutcome::try_from(args.outcome.clone())
                {
                    outcome_listener.notify_command_outcome(outcome);
                }
            }
        });
        macro_rules! spawn_state_listener {
            ($receive:ident, $domain:expr, $variant:ident) => {{
                let listener = self.clone();
                let connection = connection.clone();
                tokio::spawn(async move {
                    let Ok(proxy) = DeviceDbusProxy::builder(&connection).build().await else {
                        return;
                    };
                    let Ok(mut signals) = proxy.$receive().await else {
                        return;
                    };
                    while let Some(signal) = signals.next().await {
                        if let Ok(args) = signal.args() {
                            listener.update_local_domain_state(state_from_wire(
                                $domain,
                                args.revision,
                                args.lifecycle,
                                DomainPayload::$variant(args.payload.clone()),
                                args.error.to_string(),
                            ));
                        }
                    }
                })
            }};
        }
        spawn_state_listener!(receive_audio_state_changed, DeviceDomain::Audio, Audio);
        spawn_state_listener!(
            receive_bluetooth_state_changed,
            DeviceDomain::Bluetooth,
            Bluetooth
        );
        spawn_state_listener!(
            receive_brightness_state_changed,
            DeviceDomain::Brightness,
            Brightness
        );
        spawn_state_listener!(
            receive_network_state_changed,
            DeviceDomain::Network,
            Network
        );
        spawn_state_listener!(
            receive_night_light_state_changed,
            DeviceDomain::NightLight,
            NightLight
        );
        spawn_state_listener!(
            receive_power_profile_state_changed,
            DeviceDomain::PowerProfile,
            PowerProfile
        );
        spawn_state_listener!(receive_media_state_changed, DeviceDomain::Media, Media);
        spawn_state_listener!(
            receive_battery_state_changed,
            DeviceDomain::Battery,
            Battery
        );
        spawn_state_listener!(
            receive_caffeine_state_changed,
            DeviceDomain::Caffeine,
            Caffeine
        );
        Ok(())
    }

    /// Keeps the client connected for the lifetime of its consumer. Failed
    /// attempts leave the cached projection explicitly reconnecting instead of
    /// silently retaining stale ready data.
    pub async fn maintain_connection(&self) {
        let mut attempt = 0u32;
        loop {
            if self.is_connected() {
                tokio::time::sleep(Duration::from_millis(250)).await;
                continue;
            }
            self.mark_connection_lost("device daemon unavailable");
            match self.connect().await {
                Ok(()) => attempt = 0,
                Err(error) => {
                    attempt = attempt.saturating_add(1);
                    let delay = Duration::from_secs(2u64.saturating_pow(attempt.min(5)));
                    self.mark_connection_lost(&error);
                    tokio::time::sleep(delay.min(Duration::from_secs(30))).await;
                }
            }
        }
    }

    pub fn is_connected(&self) -> bool {
        self.connection.lock().unwrap().is_some()
    }

    pub fn get_domain_state(&self, domain: DeviceDomain) -> DomainState {
        self.domains
            .read()
            .unwrap()
            .get(&domain)
            .cloned()
            .unwrap_or_else(|| unavailable_state(domain))
    }

    pub fn subscribe_updates(&self) -> broadcast::Receiver<DeviceClientUpdate> {
        self.update_tx.subscribe()
    }

    pub fn subscribe_outcomes(&self) -> broadcast::Receiver<CommandOutcome> {
        self.outcome_tx.subscribe()
    }

    pub fn update_local_domain_state(&self, state: DomainState) {
        let domain = state.domain;
        let mut domains = self.domains.write().unwrap();
        if domains
            .get(&domain)
            .is_some_and(|current| current.revision > state.revision)
        {
            return;
        }
        domains.insert(domain, state.clone());
        drop(domains);
        let _ = self.update_tx.send(DeviceClientUpdate { domain, state });
    }

    fn mark_connection_lost(&self, error: &str) {
        let mut domains = self.domains.write().unwrap();
        for state in domains.values_mut() {
            state.lifecycle = DomainLifecycle::Reconnecting;
            state.error = Some(error.to_string());
            let _ = self.update_tx.send(DeviceClientUpdate {
                domain: state.domain,
                state: state.clone(),
            });
        }
    }

    pub fn notify_command_outcome(&self, outcome: CommandOutcome) {
        let _ = self.outcome_tx.send(outcome);
    }

    pub async fn send_command(&self, command: DeviceCommand) -> Result<CommandOutcome, String> {
        let connection = self
            .connection
            .lock()
            .unwrap()
            .clone()
            .ok_or_else(|| "device daemon unavailable (degraded state)".to_string())?;
        let proxy = DeviceDbusProxy::builder(&connection)
            .build()
            .await
            .map_err(|error| format!("device daemon proxy unavailable: {error}"))?;
        use shilpo_device_protocol::{
            AudioAction, BluetoothAction, BrightnessAction, CaffeineAction, MediaAction,
            NetworkAction, NightLightAction, PowerProfileAction,
        };
        let record = match command {
            DeviceCommand::Audio(AudioAction::SetVolume(v)) => {
                proxy.set_audio_volume(v, PROTOCOL_VERSION).await
            }
            DeviceCommand::Audio(AudioAction::SetMuted(v)) => {
                proxy.set_audio_muted(v, PROTOCOL_VERSION).await
            }
            DeviceCommand::Audio(AudioAction::ToggleMute) => {
                proxy.toggle_audio_mute(PROTOCOL_VERSION).await
            }
            DeviceCommand::Bluetooth(BluetoothAction::SetPowered(v)) => {
                proxy.set_bluetooth_powered(v, PROTOCOL_VERSION).await
            }
            DeviceCommand::Bluetooth(BluetoothAction::TogglePowered) => {
                proxy.toggle_bluetooth_powered(PROTOCOL_VERSION).await
            }
            DeviceCommand::Bluetooth(BluetoothAction::Connect(v)) => {
                proxy.connect_bluetooth(&v, PROTOCOL_VERSION).await
            }
            DeviceCommand::Bluetooth(BluetoothAction::Disconnect(v)) => {
                proxy.disconnect_bluetooth(&v, PROTOCOL_VERSION).await
            }
            DeviceCommand::Brightness(BrightnessAction::SetBrightness(v)) => {
                proxy.set_brightness(v, PROTOCOL_VERSION).await
            }
            DeviceCommand::Brightness(BrightnessAction::StepUp) => {
                proxy.step_brightness_up(PROTOCOL_VERSION).await
            }
            DeviceCommand::Brightness(BrightnessAction::StepDown) => {
                proxy.step_brightness_down(PROTOCOL_VERSION).await
            }
            DeviceCommand::Network(NetworkAction::SetWifiEnabled(v)) => {
                proxy.set_wifi_enabled(v, PROTOCOL_VERSION).await
            }
            DeviceCommand::Network(NetworkAction::ToggleWifi) => {
                proxy.toggle_wifi(PROTOCOL_VERSION).await
            }
            DeviceCommand::Network(NetworkAction::ConnectWifi(v)) => {
                proxy.connect_wifi(&v, PROTOCOL_VERSION).await
            }
            DeviceCommand::Network(NetworkAction::ConnectVpn(v)) => {
                proxy.connect_vpn(&v, PROTOCOL_VERSION).await
            }
            DeviceCommand::Network(NetworkAction::DisconnectVpn(v)) => {
                proxy.disconnect_vpn(&v, PROTOCOL_VERSION).await
            }
            DeviceCommand::NightLight(NightLightAction::SetEnabled(v)) => {
                proxy.set_night_light_enabled(v, PROTOCOL_VERSION).await
            }
            DeviceCommand::NightLight(NightLightAction::ToggleEnabled) => {
                proxy.toggle_night_light(PROTOCOL_VERSION).await
            }
            DeviceCommand::NightLight(NightLightAction::SetTemperature(v)) => {
                proxy.set_night_light_temperature(v, PROTOCOL_VERSION).await
            }
            DeviceCommand::PowerProfile(PowerProfileAction::SetProfile(v)) => {
                proxy.set_power_profile(&v, PROTOCOL_VERSION).await
            }
            DeviceCommand::Media(MediaAction::Play) => proxy.media_play(PROTOCOL_VERSION).await,
            DeviceCommand::Media(MediaAction::Pause) => proxy.media_pause(PROTOCOL_VERSION).await,
            DeviceCommand::Media(MediaAction::PlayPause) => {
                proxy.media_play_pause(PROTOCOL_VERSION).await
            }
            DeviceCommand::Media(MediaAction::Next) => proxy.media_next(PROTOCOL_VERSION).await,
            DeviceCommand::Media(MediaAction::Previous) => {
                proxy.media_previous(PROTOCOL_VERSION).await
            }
            DeviceCommand::Caffeine(CaffeineAction::SetEnabled(v)) => {
                proxy.set_caffeine_enabled(v, PROTOCOL_VERSION).await
            }
            DeviceCommand::Caffeine(CaffeineAction::Toggle) => {
                proxy.toggle_caffeine(PROTOCOL_VERSION).await
            }
        }
        .map_err(|error| format!("device command failed: {error}"))?;
        let outcome = CommandOutcome::try_from(record)
            .map_err(|error| format!("invalid device command outcome: {error}"))?;
        let _command_id = match &outcome {
            CommandOutcome::Applied { command_id, .. }
            | CommandOutcome::Rejected { command_id, .. }
            | CommandOutcome::Timeout { command_id, .. }
            | CommandOutcome::ReconciledApplied { command_id, .. } => command_id.clone(),
        };
        self.notify_command_outcome(outcome.clone());
        Ok(outcome)
    }

    pub fn send_command_debounced(&self, command: DeviceCommand, delay: Duration) {
        let _ = self.debounce_tx.send((command, delay));
    }
}

fn state_from_wire(
    domain: DeviceDomain,
    revision: u64,
    lifecycle: u8,
    payload: DomainPayload,
    error: String,
) -> DomainState {
    DomainState {
        domain,
        revision,
        lifecycle: match lifecycle {
            1 => DomainLifecycle::Connecting,
            2 => DomainLifecycle::Ready,
            3 => DomainLifecycle::Reconnecting,
            4 => DomainLifecycle::Degraded,
            _ => DomainLifecycle::Unavailable,
        },
        payload,
        error: (!error.is_empty()).then_some(error),
    }
}

impl Default for DeviceClient {
    fn default() -> Self {
        Self::new()
    }
}

fn unavailable_state(domain: DeviceDomain) -> DomainState {
    DomainState {
        domain,
        revision: 0,
        lifecycle: DomainLifecycle::Unavailable,
        payload: DomainPayload::empty(domain),
        error: None,
    }
}

fn unavailable_domains() -> HashMap<DeviceDomain, DomainState> {
    DeviceDomain::ALL
        .into_iter()
        .map(|domain| (domain, unavailable_state(domain)))
        .collect()
}

#[zbus::proxy(
    interface = "org.shilpo.Device",
    default_service = "org.shilpo.Device",
    default_path = "/org/shilpo/Device"
)]
trait DeviceDbus {
    fn get_protocol_version(&self) -> zbus::Result<u32>;
    fn get_audio_state(
        &self,
    ) -> zbus::Result<(u64, u8, shilpo_device_protocol::AudioPayload, String)>;
    fn get_bluetooth_state(
        &self,
    ) -> zbus::Result<(u64, u8, shilpo_device_protocol::BluetoothPayload, String)>;
    fn get_brightness_state(
        &self,
    ) -> zbus::Result<(u64, u8, shilpo_device_protocol::BrightnessPayload, String)>;
    fn get_network_state(
        &self,
    ) -> zbus::Result<(u64, u8, shilpo_device_protocol::NetworkPayload, String)>;
    fn get_night_light_state(
        &self,
    ) -> zbus::Result<(u64, u8, shilpo_device_protocol::NightLightPayload, String)>;
    fn get_power_profile_state(
        &self,
    ) -> zbus::Result<(u64, u8, shilpo_device_protocol::PowerProfilePayload, String)>;
    fn get_media_state(
        &self,
    ) -> zbus::Result<(u64, u8, shilpo_device_protocol::MediaPayload, String)>;
    fn get_battery_state(
        &self,
    ) -> zbus::Result<(u64, u8, shilpo_device_protocol::BatteryPayload, String)>;
    fn get_caffeine_state(
        &self,
    ) -> zbus::Result<(u64, u8, shilpo_device_protocol::CaffeinePayload, String)>;
    fn set_audio_volume(
        &self,
        value: u8,
        version: u32,
    ) -> zbus::Result<shilpo_device_protocol::CommandOutcomeRecord>;
    fn set_audio_muted(
        &self,
        value: bool,
        version: u32,
    ) -> zbus::Result<shilpo_device_protocol::CommandOutcomeRecord>;
    fn toggle_audio_mute(
        &self,
        version: u32,
    ) -> zbus::Result<shilpo_device_protocol::CommandOutcomeRecord>;
    fn set_bluetooth_powered(
        &self,
        value: bool,
        version: u32,
    ) -> zbus::Result<shilpo_device_protocol::CommandOutcomeRecord>;
    fn toggle_bluetooth_powered(
        &self,
        version: u32,
    ) -> zbus::Result<shilpo_device_protocol::CommandOutcomeRecord>;
    fn connect_bluetooth(
        &self,
        address: &str,
        version: u32,
    ) -> zbus::Result<shilpo_device_protocol::CommandOutcomeRecord>;
    fn disconnect_bluetooth(
        &self,
        address: &str,
        version: u32,
    ) -> zbus::Result<shilpo_device_protocol::CommandOutcomeRecord>;
    fn set_brightness(
        &self,
        value: u8,
        version: u32,
    ) -> zbus::Result<shilpo_device_protocol::CommandOutcomeRecord>;
    fn step_brightness_up(
        &self,
        version: u32,
    ) -> zbus::Result<shilpo_device_protocol::CommandOutcomeRecord>;
    fn step_brightness_down(
        &self,
        version: u32,
    ) -> zbus::Result<shilpo_device_protocol::CommandOutcomeRecord>;
    fn set_wifi_enabled(
        &self,
        value: bool,
        version: u32,
    ) -> zbus::Result<shilpo_device_protocol::CommandOutcomeRecord>;
    fn toggle_wifi(
        &self,
        version: u32,
    ) -> zbus::Result<shilpo_device_protocol::CommandOutcomeRecord>;
    fn connect_wifi(
        &self,
        ssid: &str,
        version: u32,
    ) -> zbus::Result<shilpo_device_protocol::CommandOutcomeRecord>;
    fn connect_vpn(
        &self,
        name: &str,
        version: u32,
    ) -> zbus::Result<shilpo_device_protocol::CommandOutcomeRecord>;
    fn disconnect_vpn(
        &self,
        name: &str,
        version: u32,
    ) -> zbus::Result<shilpo_device_protocol::CommandOutcomeRecord>;
    fn set_night_light_enabled(
        &self,
        value: bool,
        version: u32,
    ) -> zbus::Result<shilpo_device_protocol::CommandOutcomeRecord>;
    fn toggle_night_light(
        &self,
        version: u32,
    ) -> zbus::Result<shilpo_device_protocol::CommandOutcomeRecord>;
    fn set_night_light_temperature(
        &self,
        value: u32,
        version: u32,
    ) -> zbus::Result<shilpo_device_protocol::CommandOutcomeRecord>;
    fn set_power_profile(
        &self,
        profile: &str,
        version: u32,
    ) -> zbus::Result<shilpo_device_protocol::CommandOutcomeRecord>;
    fn media_play(
        &self,
        version: u32,
    ) -> zbus::Result<shilpo_device_protocol::CommandOutcomeRecord>;
    fn media_pause(
        &self,
        version: u32,
    ) -> zbus::Result<shilpo_device_protocol::CommandOutcomeRecord>;
    fn media_play_pause(
        &self,
        version: u32,
    ) -> zbus::Result<shilpo_device_protocol::CommandOutcomeRecord>;
    fn media_next(
        &self,
        version: u32,
    ) -> zbus::Result<shilpo_device_protocol::CommandOutcomeRecord>;
    fn media_previous(
        &self,
        version: u32,
    ) -> zbus::Result<shilpo_device_protocol::CommandOutcomeRecord>;
    fn set_caffeine_enabled(
        &self,
        value: bool,
        version: u32,
    ) -> zbus::Result<shilpo_device_protocol::CommandOutcomeRecord>;
    fn toggle_caffeine(
        &self,
        version: u32,
    ) -> zbus::Result<shilpo_device_protocol::CommandOutcomeRecord>;

    #[zbus(signal)]
    fn audio_state_changed(
        &self,
        revision: u64,
        lifecycle: u8,
        payload: shilpo_device_protocol::AudioPayload,
        error: &str,
    ) -> zbus::Result<()>;
    #[zbus(signal)]
    fn bluetooth_state_changed(
        &self,
        revision: u64,
        lifecycle: u8,
        payload: shilpo_device_protocol::BluetoothPayload,
        error: &str,
    ) -> zbus::Result<()>;
    #[zbus(signal)]
    fn brightness_state_changed(
        &self,
        revision: u64,
        lifecycle: u8,
        payload: shilpo_device_protocol::BrightnessPayload,
        error: &str,
    ) -> zbus::Result<()>;
    #[zbus(signal)]
    fn network_state_changed(
        &self,
        revision: u64,
        lifecycle: u8,
        payload: shilpo_device_protocol::NetworkPayload,
        error: &str,
    ) -> zbus::Result<()>;
    #[zbus(signal)]
    fn night_light_state_changed(
        &self,
        revision: u64,
        lifecycle: u8,
        payload: shilpo_device_protocol::NightLightPayload,
        error: &str,
    ) -> zbus::Result<()>;
    #[zbus(signal)]
    fn power_profile_state_changed(
        &self,
        revision: u64,
        lifecycle: u8,
        payload: shilpo_device_protocol::PowerProfilePayload,
        error: &str,
    ) -> zbus::Result<()>;
    #[zbus(signal)]
    fn media_state_changed(
        &self,
        revision: u64,
        lifecycle: u8,
        payload: shilpo_device_protocol::MediaPayload,
        error: &str,
    ) -> zbus::Result<()>;
    #[zbus(signal)]
    fn battery_state_changed(
        &self,
        revision: u64,
        lifecycle: u8,
        payload: shilpo_device_protocol::BatteryPayload,
        error: &str,
    ) -> zbus::Result<()>;
    #[zbus(signal)]
    fn caffeine_state_changed(
        &self,
        revision: u64,
        lifecycle: u8,
        payload: shilpo_device_protocol::CaffeinePayload,
        error: &str,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    fn command_reconciled(
        &self,
        outcome: shilpo_device_protocol::CommandOutcomeRecord,
    ) -> zbus::Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn starts_degraded_without_daemon() {
        let client = DeviceClient::new();
        assert!(!client.is_connected());
        assert_eq!(
            client.get_domain_state(DeviceDomain::Audio).lifecycle,
            DomainLifecycle::Unavailable
        );
    }

    #[tokio::test]
    async fn stale_domain_signal_cannot_overwrite_newer_projection() {
        let client = DeviceClient::new();
        let mut newer = unavailable_state(DeviceDomain::Audio);
        newer.revision = 3;
        newer.lifecycle = DomainLifecycle::Ready;
        let mut stale = newer.clone();
        stale.revision = 2;
        stale.lifecycle = DomainLifecycle::Degraded;

        client.update_local_domain_state(newer.clone());
        client.update_local_domain_state(stale);

        assert_eq!(client.get_domain_state(DeviceDomain::Audio), newer);
    }

    #[tokio::test]
    async fn debounce_keeps_latest_absolute_intent_per_domain() {
        use shilpo_device_protocol::{AudioAction, BrightnessAction};
        let now = tokio::time::Instant::now();
        let mut pending = DebouncedCommands::default();
        pending.replace(DeviceCommand::Audio(AudioAction::SetVolume(20)), now);
        pending.replace(DeviceCommand::Audio(AudioAction::SetVolume(80)), now);
        pending.replace(
            DeviceCommand::Brightness(BrightnessAction::SetBrightness(60)),
            now,
        );
        let due = pending.take_due(now);
        assert_eq!(due.len(), 2);
        assert!(due.contains(&DeviceCommand::Audio(AudioAction::SetVolume(80))));
        assert!(!due.contains(&DeviceCommand::Audio(AudioAction::SetVolume(20))));
        assert!(pending.is_empty());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn two_clients_share_typed_production_dbus_projection_and_commands() {
        use shilpo_device_protocol::{AudioAction, AudioPayload};
        use shilpo_services::{DeviceDaemonService, DeviceDbusService, InMemoryDeviceAdapter};
        use std::os::unix::net::UnixStream;
        use std::sync::Arc;

        let (server_socket, client_socket) = UnixStream::pair().unwrap();
        let daemon = Arc::new(DeviceDaemonService::new(Arc::new(
            InMemoryDeviceAdapter::new(),
        )));
        let server_builder = zbus::connection::Builder::unix_stream(server_socket)
            .server(zbus::Guid::generate())
            .unwrap()
            .p2p()
            .serve_at("/org/shilpo/Device", DeviceDbusService::new(daemon))
            .unwrap();
        let client_builder = zbus::connection::Builder::unix_stream(client_socket).p2p();
        let (server, connection) =
            futures_lite::future::zip(server_builder.build(), client_builder.build()).await;
        let server_connection = server.unwrap();
        let connection = connection.unwrap();
        let shell = DeviceClient::new();
        let settings = DeviceClient::new();
        shell.connect_on(connection.clone()).await.unwrap();
        settings.connect_on(connection).await.unwrap();
        assert_eq!(
            shell.get_domain_state(DeviceDomain::Audio),
            settings.get_domain_state(DeviceDomain::Audio)
        );

        let outcome = shell
            .send_command(DeviceCommand::Audio(AudioAction::SetVolume(72)))
            .await
            .unwrap();
        assert!(matches!(outcome, CommandOutcome::Applied { .. }));
        tokio::time::sleep(Duration::from_millis(20)).await;
        let shell_state = shell.get_domain_state(DeviceDomain::Audio);
        let settings_state = settings.get_domain_state(DeviceDomain::Audio);
        assert_eq!(shell_state.revision, settings_state.revision);
        assert!(matches!(
            shell_state.payload,
            DomainPayload::Audio(AudioPayload { volume: 72, .. })
        ));
        drop(server_connection);
        for _ in 0..20 {
            if !shell.is_connected() && !settings.is_connected() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        let shell_degraded = shell.get_domain_state(DeviceDomain::Audio);
        let settings_degraded = settings.get_domain_state(DeviceDomain::Audio);
        assert_eq!(shell_degraded.lifecycle, DomainLifecycle::Reconnecting);
        assert_eq!(shell_degraded, settings_degraded);
    }
}
