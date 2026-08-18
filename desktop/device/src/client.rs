use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use futures_lite::StreamExt;
use tokio::sync::broadcast;

use crate::protocol::{
    CancellationReason, CommandId, CommandOutcome, DeviceCommand, DeviceDomain, DomainLifecycle,
    DomainPayload, DomainState, DomainVersion, PROTOCOL_VERSION,
};
use shilpo_domain::{DomainSupervisor, MonotonicTimeSource, SupervisorState, TimeSource};

const QUARANTINE_IDLE_POLL_MS: u64 = 2_000;

#[derive(Clone, Debug)]
pub struct DeviceClientUpdate {
    pub domain: DeviceDomain,
    pub state: DomainState,
}

#[derive(Clone)]
pub struct DeviceClient {
    inner: Arc<DeviceClientInner>,
}

struct DeviceClientInner {
    domains: RwLock<HashMap<DeviceDomain, DomainState>>,
    update_tx: broadcast::Sender<DeviceClientUpdate>,
    outcome_tx: broadcast::Sender<CommandOutcome>,
    connection: Mutex<Option<zbus::Connection>>,
    connection_identity: Mutex<Option<String>>,
    debounced: Mutex<DebouncedCommands>,
    installed_owner_generation: AtomicU64,
    debounce_depth: AtomicU64,
    restarts: AtomicU64,
    last_error: Mutex<Option<String>>,
    stale_updates: AtomicU64,
    overloads: AtomicU64,
    supersessions: AtomicU64,
    supervisor: Mutex<DomainSupervisor>,
    time_source: Arc<dyn TimeSource>,
    is_shutdown: AtomicBool,
    next_command_seq: AtomicU64,
}

impl DeviceClientInner {
    fn shutdown(&self) {
        if self.is_shutdown.swap(true, Ordering::SeqCst) {
            return;
        }

        // 1. Enter the ADR-0006 `Stopping` transitional state while the drain
        // below is still in flight; only the final step below moves to `Stopped`.
        self.supervisor.lock().unwrap().enter_stopping();

        // 2. Set domain lifecycles to Unavailable and broadcast terminal snapshot
        {
            let mut domains = self.domains.write().unwrap();
            for state in domains.values_mut() {
                state.lifecycle = DomainLifecycle::Unavailable;
                let _ = self.update_tx.send(DeviceClientUpdate {
                    domain: state.domain,
                    state: state.clone(),
                });
            }
        }

        // 3. Drain all debounced commands and emit Cancelled { Shutdown }. Each
        // drained command is given a fresh id/sequence number: the debounce
        // queue never assigned either while pending, so reusing the
        // coalescing key here would collide across distinct non-coalescible
        // commands sharing no key at all.
        let drained = {
            let mut debounced = self.debounced.lock().unwrap();
            debounced.drain(&self.debounce_depth)
        };
        for cmd in drained {
            let seq = self.next_command_seq.fetch_add(1, Ordering::SeqCst);
            let key = cmd.coalescing_key().unwrap_or_else(|| "cmd".to_string());
            let _ = self.outcome_tx.send(CommandOutcome::Cancelled {
                command_id: CommandId(format!("{key}-shutdown-{seq}")),
                arrival_sequence: seq,
                domain: cmd.domain(),
                reason: CancellationReason::Shutdown,
            });
        }

        // 4. Drop D-Bus connection
        *self.connection.lock().unwrap() = None;

        // 5. Fully quiesced: move to the terminal `Stopped` state.
        self.supervisor.lock().unwrap().enter_stopped();
    }
}

impl Drop for DeviceClientInner {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[derive(Default)]
struct DebouncedCommands {
    pending: HashMap<String, (DeviceCommand, tokio::time::Instant)>,
    non_coalescible: Vec<(DeviceCommand, tokio::time::Instant)>,
}

impl DebouncedCommands {
    fn replace(
        &mut self,
        command: DeviceCommand,
        deadline: tokio::time::Instant,
        supersessions: &AtomicU64,
        depth: &AtomicU64,
    ) -> bool {
        if let Some(key) = command.coalescing_key() {
            if self.pending.insert(key, (command, deadline)).is_some() {
                supersessions.fetch_add(1, Ordering::SeqCst);
            } else {
                depth.fetch_add(1, Ordering::SeqCst);
            }
        } else {
            if self.pending.len() + self.non_coalescible.len() >= 32 {
                return false;
            }
            self.non_coalescible.push((command, deadline));
            depth.fetch_add(1, Ordering::SeqCst);
        }
        true
    }

    fn take_due(&mut self, now: tokio::time::Instant, depth: &AtomicU64) -> Vec<DeviceCommand> {
        let mut due = Vec::new();
        let due_keys: Vec<String> = self
            .pending
            .iter()
            .filter_map(|(k, (_, deadline))| (*deadline <= now).then_some(k.clone()))
            .collect();
        for key in due_keys {
            if let Some((command, _)) = self.pending.remove(&key) {
                due.push(command);
            }
        }
        let (due_nc, remaining_nc) = std::mem::take(&mut self.non_coalescible)
            .into_iter()
            .partition(|(_, deadline)| *deadline <= now);
        self.non_coalescible = remaining_nc;
        due.extend(due_nc.into_iter().map(|(cmd, _)| cmd));
        depth.fetch_sub(due.len() as u64, Ordering::SeqCst);
        due
    }

    fn drain(&mut self, depth: &AtomicU64) -> Vec<DeviceCommand> {
        let mut all = Vec::new();
        for (_, (cmd, _)) in self.pending.drain() {
            all.push(cmd);
        }
        for (cmd, _) in self.non_coalescible.drain(..) {
            all.push(cmd);
        }
        depth.fetch_sub(all.len() as u64, Ordering::SeqCst);
        all
    }

    #[cfg(test)]
    fn is_empty(&self) -> bool {
        self.pending.is_empty() && self.non_coalescible.is_empty()
    }
}

impl DeviceClient {
    pub fn spawn_command(client: Self, command: DeviceCommand) {
        match tokio::runtime::Handle::try_current() {
            Ok(handle) => {
                handle.spawn(async move {
                    let _ = client.send_command(command).await;
                });
            }
            Err(_) => {
                tracing::warn!("dropping device command: no Tokio runtime available to spawn on")
            }
        }
    }

    pub fn new() -> Self {
        Self::new_with_time_source(Arc::new(MonotonicTimeSource::new()))
    }

    pub fn new_with_time_source(time_source: Arc<dyn TimeSource>) -> Self {
        let (update_tx, _) = broadcast::channel(128);
        let (outcome_tx, _) = broadcast::channel(128);
        let domains = RwLock::new(unavailable_domains());
        let connection = Mutex::new(None);
        let connection_identity = Mutex::new(None);
        let debounced = Mutex::new(DebouncedCommands::default());
        let installed_owner_generation = AtomicU64::new(0);
        let debounce_depth = AtomicU64::new(0);
        let restarts = AtomicU64::new(0);
        let last_error = Mutex::new(None);
        let stale_updates = AtomicU64::new(0);
        let overloads = AtomicU64::new(0);
        let supersessions = AtomicU64::new(0);
        let supervisor = Mutex::new(DomainSupervisor::new());

        let inner = Arc::new(DeviceClientInner {
            domains,
            update_tx,
            outcome_tx,
            connection,
            connection_identity,
            debounced,
            installed_owner_generation,
            debounce_depth,
            restarts,
            last_error,
            stale_updates,
            overloads,
            supersessions,
            supervisor,
            time_source,
            is_shutdown: AtomicBool::new(false),
            next_command_seq: AtomicU64::new(0),
        });

        let client = Self {
            inner: inner.clone(),
        };
        // Debounced submission now inserts directly into a shared mutex
        // (`send_command_debounced` below) instead of routing through an
        // mpsc channel, so shutdown can drain everything synchronously under
        // one lock with no risk of a submission racing in behind the drain.
        // This worker only needs to poll for due deadlines and exit once
        // `is_shutdown` is observed.
        let weak_inner = Arc::downgrade(&inner);
        match tokio::runtime::Handle::try_current() {
            Ok(handle) => {
                handle.spawn(async move {
                    loop {
                        tokio::time::sleep(Duration::from_millis(20)).await;
                        let Some(inner) = weak_inner.upgrade() else {
                            break;
                        };
                        if inner.is_shutdown.load(Ordering::SeqCst) {
                            break;
                        }
                        let due = {
                            let mut pending = inner.debounced.lock().unwrap();
                            pending.take_due(tokio::time::Instant::now(), &inner.debounce_depth)
                        };
                        for command in due {
                            let client = DeviceClient {
                                inner: inner.clone(),
                            };
                            let _ = client.send_command(command).await;
                        }
                    }
                });
            }
            Err(_) => tracing::warn!(
                "device command debounce loop not started: no Tokio runtime available to spawn on"
            ),
        }
        client
    }

    pub fn shutdown(&self) {
        self.inner.shutdown();
    }

    pub fn installed_owner_generation(&self) -> u64 {
        self.inner.installed_owner_generation.load(Ordering::SeqCst)
    }

    pub fn stale_updates(&self) -> u64 {
        self.inner.stale_updates.load(Ordering::SeqCst)
    }

    pub fn overloads(&self) -> u64 {
        self.inner.overloads.load(Ordering::SeqCst)
    }

    pub fn supersessions(&self) -> u64 {
        self.inner.supersessions.load(Ordering::SeqCst)
    }

    pub fn telemetry(&self) -> crate::protocol::DomainPortTelemetry {
        crate::protocol::DomainPortTelemetry {
            owner_generation: self.inner.installed_owner_generation.load(Ordering::SeqCst),
            current_queue_depth: self.inner.debounce_depth.load(Ordering::SeqCst) as usize,
            queue_capacity: 32,
            overloads: self.inner.overloads.load(Ordering::SeqCst),
            supersessions: self.inner.supersessions.load(Ordering::SeqCst),
            restarts: self.inner.restarts.load(Ordering::SeqCst),
            stale_updates: self.inner.stale_updates.load(Ordering::SeqCst),
            last_error: self.inner.last_error.lock().unwrap().clone(),
        }
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

        // Increment installed owner generation on establishing a new owner connection
        let identity = connection.unique_name().map(ToString::to_string);
        let owner_changed = {
            let mut installed = self.inner.connection_identity.lock().unwrap();
            let changed = *installed != identity;
            if changed {
                *installed = identity;
            }
            changed
        };
        if owner_changed {
            self.inner
                .installed_owner_generation
                .fetch_add(1, Ordering::SeqCst);
        }
        let installed_generation = self.installed_owner_generation();
        *self.inner.last_error.lock().unwrap() = None;

        let mut listener_tasks = Vec::new();
        let mut listener_readiness = Vec::new();
        let outcome_listener = self.clone();
        let outcome_connection = connection.clone();
        let (outcome_ready_tx, outcome_ready_rx) = tokio::sync::oneshot::channel();
        listener_readiness.push(outcome_ready_rx);
        listener_tasks.push(tokio::spawn(async move {
            let Ok(proxy) = DeviceDbusProxy::builder(&outcome_connection).build().await else {
                let _ = outcome_ready_tx.send(Err(
                    "device daemon outcome signal proxy unavailable".to_string(),
                ));
                return;
            };
            let Ok(mut signals) = proxy.receive_command_reconciled().await else {
                let _ = outcome_ready_tx.send(Err(
                    "device daemon outcome signal subscription failed".to_string(),
                ));
                return;
            };
            let _ = outcome_ready_tx.send(Ok(()));
            while let Some(signal) = signals.next().await {
                if let Ok(args) = signal.args()
                    && let Ok(outcome) = CommandOutcome::try_from(args.outcome.clone())
                {
                    outcome_listener.notify_command_outcome(outcome);
                }
            }
        }));
        macro_rules! spawn_state_listener {
            ($receive:ident, $domain:expr, $variant:ident) => {{
                let listener = self.clone();
                let connection = connection.clone();
                let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
                listener_readiness.push(ready_rx);
                listener_tasks.push(tokio::spawn(async move {
                    let Ok(proxy) = DeviceDbusProxy::builder(&connection).build().await else {
                        let _ = ready_tx.send(Err(format!(
                            "device daemon {:?} signal proxy unavailable",
                            $domain
                        )));
                        return;
                    };
                    let Ok(mut signals) = proxy.$receive().await else {
                        let _ = ready_tx.send(Err(format!(
                            "device daemon {:?} signal subscription failed",
                            $domain
                        )));
                        return;
                    };
                    let _ = ready_tx.send(Ok(()));
                    while let Some(signal) = signals.next().await {
                        if let Ok(args) = signal.args() {
                            listener.update_local_domain_state(state_from_wire(
                                $domain,
                                installed_generation,
                                args.revision,
                                args.lifecycle,
                                DomainPayload::$variant(args.payload.clone()),
                                args.error.to_string(),
                            ));
                        }
                    }
                }));
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

        for readiness in listener_readiness {
            let readiness = readiness.await.unwrap_or_else(|_| {
                Err("device daemon signal listener stopped during setup".to_string())
            });
            if let Err(error) = readiness {
                for task in &listener_tasks {
                    task.abort();
                }
                return Err(error);
            }
        }

        // Subscribe before loading the initial projection so no state change can
        // fall into the connection/setup gap. Revision checks deduplicate a
        // signal racing with the matching snapshot response.
        macro_rules! load_state {
            ($method:ident, $domain:expr, $variant:ident) => {
                if let Ok((_owner_generation, revision, lifecycle, payload, error)) =
                    proxy.$method().await
                {
                    self.update_local_domain_state(state_from_wire(
                        $domain,
                        installed_generation,
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

        // Publish only after every listener and the initial projection are
        // ready. A failed setup therefore cannot leak listeners into a retry.
        *self.inner.connection.lock().unwrap() = Some(connection.clone());
        let ready_now_ms = self.inner.time_source.now_ms();
        self.mark_ready(ready_now_ms);
        let closed_client = self.clone();
        tokio::spawn(async move {
            connection.closed().await;
            *closed_client.inner.connection.lock().unwrap() = None;
            let now_ms = closed_client.inner.time_source.now_ms();
            closed_client
                .report_owner_failure("device daemon connection closed".to_string(), now_ms);
        });
        Ok(())
    }

    /// Keeps the client connected for the lifetime of its consumer. Failed
    /// attempts drive the persistent supervisor state machine.
    pub async fn maintain_connection(&self) {
        loop {
            let now_ms = self.time_source().now_ms();
            self.tick(now_ms);

            match self.supervisor_state() {
                SupervisorState::Running => {
                    if self.is_connected() {
                        tokio::time::sleep(Duration::from_millis(250)).await;
                    } else {
                        self.report_owner_failure(
                            "device daemon connection closed".to_string(),
                            now_ms,
                        );
                    }
                }
                SupervisorState::Backoff { retry_at_ms, .. } => {
                    let now_ms = self.time_source().now_ms();
                    if now_ms < retry_at_ms {
                        tokio::time::sleep(Duration::from_millis(retry_at_ms - now_ms)).await;
                    } else {
                        self.tick(now_ms);
                    }
                }
                SupervisorState::Quarantined => {
                    tokio::time::sleep(Duration::from_millis(QUARANTINE_IDLE_POLL_MS)).await;
                }
                SupervisorState::Starting => match self.connect().await {
                    Ok(()) => {
                        let ready_now_ms = self.time_source().now_ms();
                        self.mark_ready(ready_now_ms);
                    }
                    Err(error) => {
                        let fail_now_ms = self.time_source().now_ms();
                        self.report_owner_failure(error, fail_now_ms);
                    }
                },
                SupervisorState::Stopping | SupervisorState::Stopped => break,
            }
        }
    }

    pub fn is_connected(&self) -> bool {
        self.inner.connection.lock().unwrap().is_some()
    }

    pub fn get_domain_state(&self, domain: DeviceDomain) -> DomainState {
        self.inner
            .domains
            .read()
            .unwrap()
            .get(&domain)
            .cloned()
            .unwrap_or_else(|| unavailable_state(domain))
    }

    pub fn subscribe_updates(&self) -> broadcast::Receiver<DeviceClientUpdate> {
        self.inner.update_tx.subscribe()
    }

    pub fn subscribe_outcomes(&self) -> broadcast::Receiver<CommandOutcome> {
        self.inner.outcome_tx.subscribe()
    }

    pub fn update_local_domain_state(&self, state: DomainState) {
        let installed_gen = self.inner.installed_owner_generation.load(Ordering::SeqCst);
        let domain = state.domain;
        let mut domains = self.inner.domains.write().unwrap();
        let current = domains
            .get(&domain)
            .cloned()
            .unwrap_or_else(|| unavailable_state(domain));

        // Reject future/uninstalled generation
        if state.version.owner_generation > installed_gen {
            self.inner.stale_updates.fetch_add(1, Ordering::SeqCst);
            return;
        }

        // Reject stale version (older generation or older revision in same generation)
        if state.version < current.version {
            self.inner.stale_updates.fetch_add(1, Ordering::SeqCst);
            return;
        }

        // Handle equal version: identical is idempotent; differing is a conflict rejection
        if state.version == current.version {
            if current.lifecycle == state.lifecycle
                && current.payload == state.payload
                && current.error == state.error
            {
                return;
            }
            self.inner.stale_updates.fetch_add(1, Ordering::SeqCst);
            return;
        }

        // Strictly newer version: accept and update
        domains.insert(domain, state.clone());
        drop(domains);
        let _ = self
            .inner
            .update_tx
            .send(DeviceClientUpdate { domain, state });
    }

    pub fn notify_command_outcome(&self, outcome: CommandOutcome) {
        let _ = self.inner.outcome_tx.send(outcome);
    }

    pub async fn send_command(&self, command: DeviceCommand) -> Result<CommandOutcome, String> {
        let connection = self
            .inner
            .connection
            .lock()
            .unwrap()
            .clone()
            .ok_or_else(|| "device daemon unavailable (degraded state)".to_string())?;
        let proxy = DeviceDbusProxy::builder(&connection)
            .build()
            .await
            .map_err(|error| format!("device daemon proxy unavailable: {error}"))?;
        use crate::protocol::{
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
            DeviceCommand::Brightness(BrightnessAction::SetDisplay { id, percentage }) => {
                proxy
                    .set_display_brightness(&id, percentage, PROTOCOL_VERSION)
                    .await
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
            | CommandOutcome::ReconciledApplied { command_id, .. }
            | CommandOutcome::Rejected { command_id, .. }
            | CommandOutcome::TimedOut { command_id, .. }
            | CommandOutcome::Cancelled { command_id, .. } => command_id.clone(),
        };
        self.notify_command_outcome(outcome.clone());
        Ok(outcome)
    }

    pub fn supervisor_state(&self) -> SupervisorState {
        self.inner.supervisor.lock().unwrap().state()
    }

    pub fn time_source(&self) -> &Arc<dyn TimeSource> {
        &self.inner.time_source
    }

    pub fn mark_ready(&self, now_ms: u64) {
        self.inner.supervisor.lock().unwrap().mark_running(now_ms);
        *self.inner.last_error.lock().unwrap() = None;
    }

    pub fn report_owner_failure(&self, error: String, now_ms: u64) {
        self.inner.restarts.fetch_add(1, Ordering::SeqCst);
        *self.inner.last_error.lock().unwrap() = Some(error.clone());

        let new_state = self.inner.supervisor.lock().unwrap().record_failure(now_ms);
        let target_lifecycle = match new_state {
            SupervisorState::Quarantined => DomainLifecycle::Unavailable,
            _ => DomainLifecycle::Reconnecting,
        };

        let mut domains = self.inner.domains.write().unwrap();
        for state in domains.values_mut() {
            state.lifecycle = target_lifecycle;
            state.error = Some(error.clone());
            let _ = self.inner.update_tx.send(DeviceClientUpdate {
                domain: state.domain,
                state: state.clone(),
            });
        }
    }

    pub fn tick(&self, now_ms: u64) {
        self.inner.supervisor.lock().unwrap().tick(now_ms);
    }

    pub fn reset_quarantine(&self) {
        if self.inner.supervisor.lock().unwrap().reset_quarantine() {
            let mut domains = self.inner.domains.write().unwrap();
            for state in domains.values_mut() {
                state.lifecycle = DomainLifecycle::Reconnecting;
                let _ = self.inner.update_tx.send(DeviceClientUpdate {
                    domain: state.domain,
                    state: state.clone(),
                });
            }
        }
    }

    pub fn send_command_debounced(
        &self,
        command: DeviceCommand,
        delay: Duration,
    ) -> Result<(), crate::protocol::RejectionReason> {
        if self.inner.is_shutdown.load(Ordering::SeqCst) {
            return Err(crate::protocol::RejectionReason::Unavailable);
        }
        if matches!(
            self.inner.supervisor.lock().unwrap().state(),
            SupervisorState::Quarantined | SupervisorState::Stopped
        ) {
            return Err(crate::protocol::RejectionReason::Unavailable);
        }
        let mut pending = self.inner.debounced.lock().unwrap();
        if self.inner.is_shutdown.load(Ordering::SeqCst) {
            return Err(crate::protocol::RejectionReason::Unavailable);
        }
        if !pending.replace(
            command,
            tokio::time::Instant::now() + delay,
            &self.inner.supersessions,
            &self.inner.debounce_depth,
        ) {
            self.inner.overloads.fetch_add(1, Ordering::SeqCst);
            return Err(crate::protocol::RejectionReason::Overloaded);
        }
        Ok(())
    }
}

fn state_from_wire(
    domain: DeviceDomain,
    owner_generation: u64,
    revision: u64,
    lifecycle: u8,
    payload: DomainPayload,
    error: String,
) -> DomainState {
    DomainState {
        domain,
        version: DomainVersion::new(owner_generation, revision),
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
        version: DomainVersion::ZERO,
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
    ) -> zbus::Result<(u64, u64, u8, crate::protocol::AudioPayload, String)>;
    fn get_bluetooth_state(
        &self,
    ) -> zbus::Result<(u64, u64, u8, crate::protocol::BluetoothPayload, String)>;
    fn get_brightness_state(
        &self,
    ) -> zbus::Result<(u64, u64, u8, crate::protocol::BrightnessPayload, String)>;
    fn get_network_state(
        &self,
    ) -> zbus::Result<(u64, u64, u8, crate::protocol::NetworkPayload, String)>;
    fn get_night_light_state(
        &self,
    ) -> zbus::Result<(u64, u64, u8, crate::protocol::NightLightPayload, String)>;
    fn get_power_profile_state(
        &self,
    ) -> zbus::Result<(u64, u64, u8, crate::protocol::PowerProfilePayload, String)>;
    fn get_media_state(
        &self,
    ) -> zbus::Result<(u64, u64, u8, crate::protocol::MediaPayload, String)>;
    fn get_battery_state(
        &self,
    ) -> zbus::Result<(u64, u64, u8, crate::protocol::BatteryPayload, String)>;
    fn get_caffeine_state(
        &self,
    ) -> zbus::Result<(u64, u64, u8, crate::protocol::CaffeinePayload, String)>;
    fn set_audio_volume(
        &self,
        value: u8,
        version: u32,
    ) -> zbus::Result<crate::protocol::CommandOutcomeRecord>;
    fn set_audio_muted(
        &self,
        value: bool,
        version: u32,
    ) -> zbus::Result<crate::protocol::CommandOutcomeRecord>;
    fn toggle_audio_mute(
        &self,
        version: u32,
    ) -> zbus::Result<crate::protocol::CommandOutcomeRecord>;
    fn set_bluetooth_powered(
        &self,
        value: bool,
        version: u32,
    ) -> zbus::Result<crate::protocol::CommandOutcomeRecord>;
    fn toggle_bluetooth_powered(
        &self,
        version: u32,
    ) -> zbus::Result<crate::protocol::CommandOutcomeRecord>;
    fn connect_bluetooth(
        &self,
        address: &str,
        version: u32,
    ) -> zbus::Result<crate::protocol::CommandOutcomeRecord>;
    fn disconnect_bluetooth(
        &self,
        address: &str,
        version: u32,
    ) -> zbus::Result<crate::protocol::CommandOutcomeRecord>;
    fn set_brightness(
        &self,
        value: u8,
        version: u32,
    ) -> zbus::Result<crate::protocol::CommandOutcomeRecord>;
    fn set_display_brightness(
        &self,
        id: &str,
        value: u8,
        version: u32,
    ) -> zbus::Result<crate::protocol::CommandOutcomeRecord>;
    fn step_brightness_up(
        &self,
        version: u32,
    ) -> zbus::Result<crate::protocol::CommandOutcomeRecord>;
    fn step_brightness_down(
        &self,
        version: u32,
    ) -> zbus::Result<crate::protocol::CommandOutcomeRecord>;
    fn set_wifi_enabled(
        &self,
        value: bool,
        version: u32,
    ) -> zbus::Result<crate::protocol::CommandOutcomeRecord>;
    fn toggle_wifi(&self, version: u32) -> zbus::Result<crate::protocol::CommandOutcomeRecord>;
    fn connect_wifi(
        &self,
        ssid: &str,
        version: u32,
    ) -> zbus::Result<crate::protocol::CommandOutcomeRecord>;
    fn connect_vpn(
        &self,
        name: &str,
        version: u32,
    ) -> zbus::Result<crate::protocol::CommandOutcomeRecord>;
    fn disconnect_vpn(
        &self,
        name: &str,
        version: u32,
    ) -> zbus::Result<crate::protocol::CommandOutcomeRecord>;
    fn set_night_light_enabled(
        &self,
        value: bool,
        version: u32,
    ) -> zbus::Result<crate::protocol::CommandOutcomeRecord>;
    fn toggle_night_light(
        &self,
        version: u32,
    ) -> zbus::Result<crate::protocol::CommandOutcomeRecord>;
    fn set_night_light_temperature(
        &self,
        value: u32,
        version: u32,
    ) -> zbus::Result<crate::protocol::CommandOutcomeRecord>;
    fn set_power_profile(
        &self,
        profile: &str,
        version: u32,
    ) -> zbus::Result<crate::protocol::CommandOutcomeRecord>;
    fn media_play(&self, version: u32) -> zbus::Result<crate::protocol::CommandOutcomeRecord>;
    fn media_pause(&self, version: u32) -> zbus::Result<crate::protocol::CommandOutcomeRecord>;
    fn media_play_pause(&self, version: u32)
    -> zbus::Result<crate::protocol::CommandOutcomeRecord>;
    fn media_next(&self, version: u32) -> zbus::Result<crate::protocol::CommandOutcomeRecord>;
    fn media_previous(&self, version: u32) -> zbus::Result<crate::protocol::CommandOutcomeRecord>;
    fn set_caffeine_enabled(
        &self,
        value: bool,
        version: u32,
    ) -> zbus::Result<crate::protocol::CommandOutcomeRecord>;
    fn toggle_caffeine(&self, version: u32) -> zbus::Result<crate::protocol::CommandOutcomeRecord>;

    #[zbus(signal)]
    fn audio_state_changed(
        &self,
        owner_generation: u64,
        revision: u64,
        lifecycle: u8,
        payload: crate::protocol::AudioPayload,
        error: &str,
    ) -> zbus::Result<()>;
    #[zbus(signal)]
    fn bluetooth_state_changed(
        &self,
        owner_generation: u64,
        revision: u64,
        lifecycle: u8,
        payload: crate::protocol::BluetoothPayload,
        error: &str,
    ) -> zbus::Result<()>;
    #[zbus(signal)]
    fn brightness_state_changed(
        &self,
        owner_generation: u64,
        revision: u64,
        lifecycle: u8,
        payload: crate::protocol::BrightnessPayload,
        error: &str,
    ) -> zbus::Result<()>;
    #[zbus(signal)]
    fn network_state_changed(
        &self,
        owner_generation: u64,
        revision: u64,
        lifecycle: u8,
        payload: crate::protocol::NetworkPayload,
        error: &str,
    ) -> zbus::Result<()>;
    #[zbus(signal)]
    fn night_light_state_changed(
        &self,
        owner_generation: u64,
        revision: u64,
        lifecycle: u8,
        payload: crate::protocol::NightLightPayload,
        error: &str,
    ) -> zbus::Result<()>;
    #[zbus(signal)]
    fn power_profile_state_changed(
        &self,
        owner_generation: u64,
        revision: u64,
        lifecycle: u8,
        payload: crate::protocol::PowerProfilePayload,
        error: &str,
    ) -> zbus::Result<()>;
    #[zbus(signal)]
    fn media_state_changed(
        &self,
        owner_generation: u64,
        revision: u64,
        lifecycle: u8,
        payload: crate::protocol::MediaPayload,
        error: &str,
    ) -> zbus::Result<()>;
    #[zbus(signal)]
    fn battery_state_changed(
        &self,
        owner_generation: u64,
        revision: u64,
        lifecycle: u8,
        payload: crate::protocol::BatteryPayload,
        error: &str,
    ) -> zbus::Result<()>;
    #[zbus(signal)]
    fn caffeine_state_changed(
        &self,
        owner_generation: u64,
        revision: u64,
        lifecycle: u8,
        payload: crate::protocol::CaffeinePayload,
        error: &str,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    fn command_reconciled(
        &self,
        outcome: crate::protocol::CommandOutcomeRecord,
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
        client
            .inner
            .installed_owner_generation
            .store(1, Ordering::SeqCst);
        let mut newer = unavailable_state(DeviceDomain::Audio);
        newer.version = DomainVersion::new(1, 3);
        newer.lifecycle = DomainLifecycle::Ready;
        let mut stale = newer.clone();
        stale.version = DomainVersion::new(1, 2);
        stale.lifecycle = DomainLifecycle::Degraded;

        client.update_local_domain_state(newer.clone());
        client.update_local_domain_state(stale);

        assert_eq!(client.get_domain_state(DeviceDomain::Audio), newer);
        assert_eq!(client.stale_updates(), 1);
    }

    #[tokio::test]
    async fn freshness_rejects_uninstalled_generation_and_equal_version_conflicts() {
        let client = DeviceClient::new();
        client
            .inner
            .installed_owner_generation
            .store(1, Ordering::SeqCst);

        // 1. Uninstalled generation rejection
        let uninstalled = DomainState {
            domain: DeviceDomain::Audio,
            version: DomainVersion::new(2, 0),
            lifecycle: DomainLifecycle::Ready,
            payload: DomainPayload::empty(DeviceDomain::Audio),
            error: None,
        };
        client.update_local_domain_state(uninstalled);
        assert_eq!(
            client.get_domain_state(DeviceDomain::Audio).version,
            DomainVersion::ZERO
        );
        assert_eq!(client.stale_updates(), 1);

        // Install generation 1 state
        let state_v1 = DomainState {
            domain: DeviceDomain::Audio,
            version: DomainVersion::new(1, 1),
            lifecycle: DomainLifecycle::Ready,
            payload: DomainPayload::empty(DeviceDomain::Audio),
            error: None,
        };
        client.update_local_domain_state(state_v1.clone());
        assert_eq!(
            client.get_domain_state(DeviceDomain::Audio).version,
            DomainVersion::new(1, 1)
        );

        // 2. Equal version identical is idempotent
        client.update_local_domain_state(state_v1.clone());
        assert_eq!(client.stale_updates(), 1); // Not incremented

        // 3. Equal version with different error/payload is rejected as conflict
        let mut conflict = state_v1.clone();
        conflict.error = Some("conflict".to_string());
        client.update_local_domain_state(conflict);
        assert_eq!(client.stale_updates(), 2); // Incremented
    }

    #[tokio::test]
    async fn debounce_keeps_latest_absolute_intent_per_domain() {
        use crate::protocol::{AudioAction, BrightnessAction};
        let now = tokio::time::Instant::now();
        let supersessions = AtomicU64::new(0);
        let depth = AtomicU64::new(0);
        let mut pending = DebouncedCommands::default();
        pending.replace(
            DeviceCommand::Audio(AudioAction::SetVolume(20)),
            now,
            &supersessions,
            &depth,
        );
        pending.replace(
            DeviceCommand::Audio(AudioAction::SetVolume(80)),
            now,
            &supersessions,
            &depth,
        );
        pending.replace(
            DeviceCommand::Brightness(BrightnessAction::SetBrightness(60)),
            now,
            &supersessions,
            &depth,
        );
        let due = pending.take_due(now, &depth);
        assert_eq!(due.len(), 2);
        assert!(due.contains(&DeviceCommand::Audio(AudioAction::SetVolume(80))));
        assert!(!due.contains(&DeviceCommand::Audio(AudioAction::SetVolume(20))));
        assert!(pending.is_empty());
        assert_eq!(supersessions.load(Ordering::SeqCst), 1);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn two_clients_share_typed_production_dbus_projection_and_commands() {
        use std::os::unix::net::UnixStream;
        use std::sync::Arc;

        use shilpo_services::device_daemon::{
            DeviceDaemonService, DeviceDbusService, InMemoryDeviceAdapter,
        };

        use crate::protocol::{AudioAction, AudioPayload};

        let (server_socket, client_socket) = UnixStream::pair().unwrap();
        let daemon = Arc::new(DeviceDaemonService::new(Arc::new(
            InMemoryDeviceAdapter::new(),
        )));
        let server_builder = zbus::connection::Builder::async_io_unix_stream(server_socket)
            .server(zbus::Guid::generate())
            .unwrap()
            .p2p()
            .serve_at("/org/shilpo/Device", DeviceDbusService::new(daemon))
            .unwrap();
        let client_builder = zbus::connection::Builder::async_io_unix_stream(client_socket).p2p();
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
        let (shell_state, settings_state) = {
            let mut converged = None;
            for _ in 0..50 {
                let shell_state = shell.get_domain_state(DeviceDomain::Audio);
                let settings_state = settings.get_domain_state(DeviceDomain::Audio);
                if shell_state.version == settings_state.version
                    && matches!(
                        settings_state.payload,
                        DomainPayload::Audio(AudioPayload { volume: 72, .. })
                    )
                {
                    converged = Some((shell_state, settings_state));
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            converged.expect("both clients should converge on the applied command")
        };
        assert_eq!(shell_state.version, settings_state.version);
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

    #[cfg(unix)]
    #[tokio::test]
    async fn battery_projection_retains_last_known_payload_during_reconnect() {
        use std::os::unix::net::UnixStream;
        use std::sync::Arc;

        use shilpo_services::device_daemon::{
            DeviceDaemonService, DeviceDbusService, InMemoryDeviceAdapter,
        };

        use crate::protocol::{BatteryChargeState, BatteryPayload};

        let (server_socket, client_socket) = UnixStream::pair().unwrap();
        let adapter = Arc::new(InMemoryDeviceAdapter::new());
        let daemon = Arc::new(DeviceDaemonService::new(adapter.clone()));
        let server_builder = zbus::connection::Builder::async_io_unix_stream(server_socket)
            .server(zbus::Guid::generate())
            .unwrap()
            .p2p()
            .serve_at("/org/shilpo/Device", DeviceDbusService::new(daemon))
            .unwrap();
        let client_builder = zbus::connection::Builder::async_io_unix_stream(client_socket).p2p();
        let (server, connection) =
            futures_lite::future::zip(server_builder.build(), client_builder.build()).await;
        let server_connection = server.unwrap();
        let connection = connection.unwrap();

        let client = DeviceClient::new();
        client.connect_on(connection).await.unwrap();

        let initial_battery = client.get_domain_state(DeviceDomain::Battery);
        assert_eq!(initial_battery.lifecycle, DomainLifecycle::Ready);

        // Update local domain state with a known battery snapshot
        let known_payload = BatteryPayload {
            available: true,
            is_present: true,
            percentage: 75,
            state: BatteryChargeState::Discharging,
            ..Default::default()
        };
        let mut ready_state = initial_battery;
        ready_state.version.revision += 1;
        ready_state.payload = DomainPayload::Battery(known_payload.clone());
        client.update_local_domain_state(ready_state);

        let active_state = client.get_domain_state(DeviceDomain::Battery);
        assert_eq!(active_state.lifecycle, DomainLifecycle::Ready);
        assert_eq!(
            active_state.payload,
            DomainPayload::Battery(known_payload.clone())
        );

        // Drop server connection to simulate daemon loss
        drop(server_connection);
        for _ in 0..20 {
            if !client.is_connected() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }

        let reconnecting_state = client.get_domain_state(DeviceDomain::Battery);
        assert_eq!(reconnecting_state.lifecycle, DomainLifecycle::Reconnecting);
        // Payload must be preserved during reconnect!
        assert_eq!(
            reconnecting_state.payload,
            DomainPayload::Battery(known_payload)
        );
    }

    #[test]
    fn reconnect_backoff_progression_and_30s_cap() {
        assert_eq!(shilpo_domain::reconnect_backoff_ms(0), 250);
        assert_eq!(shilpo_domain::reconnect_backoff_ms(1), 250);
        assert_eq!(shilpo_domain::reconnect_backoff_ms(2), 500);
        assert_eq!(shilpo_domain::reconnect_backoff_ms(3), 1000);
        assert_eq!(shilpo_domain::reconnect_backoff_ms(4), 2000);
        assert_eq!(shilpo_domain::reconnect_backoff_ms(5), 4000);
        assert_eq!(shilpo_domain::reconnect_backoff_ms(6), 8000);
        assert_eq!(shilpo_domain::reconnect_backoff_ms(7), 16000);
        assert_eq!(shilpo_domain::reconnect_backoff_ms(8), 30000);
        assert_eq!(shilpo_domain::reconnect_backoff_ms(9), 30000);
        assert_eq!(shilpo_domain::reconnect_backoff_ms(32), 30000);
    }

    #[tokio::test]
    async fn test_device_shutdown_cancels_queued_commands() {
        use crate::protocol::{AudioAction, BluetoothAction, BrightnessAction};
        let client = DeviceClient::new();
        let mut rx = client.subscribe_outcomes();

        client
            .send_command_debounced(
                DeviceCommand::Audio(AudioAction::SetVolume(30)),
                Duration::from_secs(10),
            )
            .unwrap();
        client
            .send_command_debounced(
                DeviceCommand::Brightness(BrightnessAction::SetBrightness(50)),
                Duration::from_secs(10),
            )
            .unwrap();
        client
            .send_command_debounced(
                DeviceCommand::Bluetooth(BluetoothAction::TogglePowered),
                Duration::from_secs(10),
            )
            .unwrap();

        assert_eq!(client.telemetry().current_queue_depth, 3);

        client.shutdown();

        assert_eq!(client.telemetry().current_queue_depth, 0);

        let mut cancelled_domains = Vec::new();
        for _ in 0..3 {
            let outcome = rx.try_recv().expect("must receive cancelled outcome");
            match outcome {
                CommandOutcome::Cancelled {
                    domain,
                    reason: CancellationReason::Shutdown,
                    ..
                } => {
                    cancelled_domains.push(domain);
                }
                other => panic!("expected Cancelled Shutdown, got {:?}", other),
            }
        }
        assert!(cancelled_domains.contains(&DeviceDomain::Audio));
        assert!(cancelled_domains.contains(&DeviceDomain::Brightness));
        assert!(cancelled_domains.contains(&DeviceDomain::Bluetooth));
        assert!(rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn test_device_shutdown_exactly_once_idempotent() {
        use crate::protocol::AudioAction;
        let client = DeviceClient::new();
        let mut rx = client.subscribe_outcomes();

        client
            .send_command_debounced(
                DeviceCommand::Audio(AudioAction::SetVolume(30)),
                Duration::from_secs(10),
            )
            .unwrap();

        client.shutdown();

        let outcome = rx.try_recv().expect("must receive cancelled outcome");
        assert!(matches!(
            outcome,
            CommandOutcome::Cancelled {
                domain: DeviceDomain::Audio,
                reason: CancellationReason::Shutdown,
                ..
            }
        ));

        // Second explicit call is a no-op
        client.shutdown();
        assert!(rx.try_recv().is_err());

        // Drop after shutdown is also a no-op
        drop(client);
        assert!(rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn test_device_drop_alone_cancels_queued_commands() {
        use crate::protocol::{AudioAction, BrightnessAction};
        let mut rx = {
            let client = DeviceClient::new();
            let rx = client.subscribe_outcomes();
            client
                .send_command_debounced(
                    DeviceCommand::Audio(AudioAction::SetVolume(30)),
                    Duration::from_secs(10),
                )
                .unwrap();
            client
                .send_command_debounced(
                    DeviceCommand::Brightness(BrightnessAction::SetBrightness(50)),
                    Duration::from_secs(10),
                )
                .unwrap();
            rx
            // client dropped here without calling shutdown()
        };

        let mut cancelled_domains = Vec::new();
        for _ in 0..2 {
            let outcome = rx
                .try_recv()
                .expect("must receive cancelled outcome on drop");
            match outcome {
                CommandOutcome::Cancelled {
                    domain,
                    reason: CancellationReason::Shutdown,
                    ..
                } => {
                    cancelled_domains.push(domain);
                }
                other => panic!("expected Cancelled Shutdown on drop, got {:?}", other),
            }
        }
        assert!(cancelled_domains.contains(&DeviceDomain::Audio));
        assert!(cancelled_domains.contains(&DeviceDomain::Brightness));
        assert!(rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn test_device_post_shutdown_submission_rejected() {
        use crate::protocol::AudioAction;
        let client = DeviceClient::new();
        let mut rx = client.subscribe_outcomes();

        client.shutdown();

        let res = client.send_command_debounced(
            DeviceCommand::Audio(AudioAction::SetVolume(30)),
            Duration::from_secs(10),
        );
        assert_eq!(res, Err(crate::protocol::RejectionReason::Unavailable));
        assert!(rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn test_device_terminal_snapshot_published_on_shutdown() {
        let client = DeviceClient::new();
        let mut update_rx = client.subscribe_updates();

        client.update_local_domain_state(DomainState {
            domain: DeviceDomain::Audio,
            version: DomainVersion::new(0, 1),
            lifecycle: DomainLifecycle::Ready,
            payload: DomainPayload::empty(DeviceDomain::Audio),
            error: None,
        });
        assert_eq!(
            client.get_domain_state(DeviceDomain::Audio).lifecycle,
            DomainLifecycle::Ready
        );

        client.shutdown();

        assert_eq!(client.supervisor_state(), SupervisorState::Stopped);
        assert_eq!(
            client.get_domain_state(DeviceDomain::Audio).lifecycle,
            DomainLifecycle::Unavailable
        );

        // Terminal snapshot observed on update stream
        let mut observed_audio_unavailable = false;
        while let Ok(update) = update_rx.try_recv() {
            if update.domain == DeviceDomain::Audio
                && update.state.lifecycle == DomainLifecycle::Unavailable
            {
                observed_audio_unavailable = true;
            }
        }
        assert!(observed_audio_unavailable);
    }
}
