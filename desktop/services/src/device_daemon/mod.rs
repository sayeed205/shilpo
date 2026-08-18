pub mod dbus;
pub mod in_memory_adapter;
pub mod system_adapter;

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

pub use dbus::DeviceDbusService;
pub use in_memory_adapter::{DeviceAdapter, InMemoryDeviceAdapter};
pub use system_adapter::SystemDeviceAdapter;
use tokio::sync::{Notify, broadcast, oneshot};

use crate::device_protocol::{
    CancellationReason, CommandId, CommandOutcome, DeviceCommand, DeviceDomain, DomainLifecycle,
    DomainPortTelemetry, DomainState, DomainVersion, RejectionReason, check_protocol_version,
};

const DOMAIN_QUEUE_CAPACITY: usize = 16;

#[derive(Clone)]
struct TimedOutCommand {
    id: CommandId,
    arrival_sequence: u64,
    command: DeviceCommand,
    before: DomainState,
}

pub struct PendingDeviceCommand {
    pub id: CommandId,
    pub arrival_sequence: u64,
    pub generation: u64,
    pub command: DeviceCommand,
    pub reply: oneshot::Sender<CommandOutcome>,
}

#[derive(Clone)]
struct DomainQueueHandle {
    pending: Arc<std::sync::Mutex<VecDeque<PendingDeviceCommand>>>,
    notify: Arc<tokio::sync::Notify>,
    in_flight: Arc<std::sync::atomic::AtomicBool>,
}

impl DomainQueueHandle {
    fn new() -> Self {
        Self {
            pending: Arc::new(std::sync::Mutex::new(VecDeque::new())),
            notify: Arc::new(tokio::sync::Notify::new()),
            in_flight: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    fn queue_depth(&self) -> usize {
        self.pending.lock().unwrap().len()
            + if self.in_flight.load(Ordering::SeqCst) {
                1
            } else {
                0
            }
    }
}

struct ExecContext<'a> {
    adapter: &'a Arc<dyn DeviceAdapter>,
    states: &'a Arc<std::sync::Mutex<HashMap<DeviceDomain, DomainState>>>,
    timed_out: &'a Arc<std::sync::Mutex<HashMap<DeviceDomain, Vec<TimedOutCommand>>>>,
    outcome_tx: &'a broadcast::Sender<CommandOutcome>,
    owner_generation: u64,
    owner_generation_source: &'a Arc<AtomicU64>,
    owner_generation_notify: &'a Arc<Notify>,
}

pub struct DeviceDaemonService {
    adapter: Arc<dyn DeviceAdapter>,
    next_arrival_sequence: Arc<AtomicU64>,
    owner_generation: Arc<AtomicU64>,
    owner_generation_notify: Arc<Notify>,
    domain_queues: HashMap<DeviceDomain, DomainQueueHandle>,
    domain_states: Arc<std::sync::Mutex<HashMap<DeviceDomain, DomainState>>>,
    timed_out: Arc<std::sync::Mutex<HashMap<DeviceDomain, Vec<TimedOutCommand>>>>,
    outcome_tx: broadcast::Sender<CommandOutcome>,
    overloads: Arc<AtomicU64>,
    supersessions: Arc<AtomicU64>,
    stale_updates: Arc<AtomicU64>,
    restarts: Arc<AtomicU64>,
    capacity: usize,
}

impl DeviceDaemonService {
    pub fn new(adapter: Arc<dyn DeviceAdapter>) -> Self {
        Self::new_with_capacity(DOMAIN_QUEUE_CAPACITY, adapter)
    }

    pub fn new_with_capacity(capacity: usize, adapter: Arc<dyn DeviceAdapter>) -> Self {
        let next_arrival_sequence = Arc::new(AtomicU64::new(1));
        let owner_generation = Arc::new(AtomicU64::new(1));
        let owner_generation_notify = Arc::new(Notify::new());
        let domain_states = Arc::new(std::sync::Mutex::new(HashMap::new()));
        let timed_out = Arc::new(std::sync::Mutex::new(HashMap::new()));
        let (outcome_tx, _) = broadcast::channel(128);
        let overloads = Arc::new(AtomicU64::new(0));
        let supersessions = Arc::new(AtomicU64::new(0));
        let stale_updates = Arc::new(AtomicU64::new(0));
        let restarts = Arc::new(AtomicU64::new(0));
        let mut domain_queues = HashMap::new();

        for domain in DeviceDomain::ALL {
            domain_states
                .lock()
                .unwrap()
                .insert(domain, adapter.get_domain_state(domain));

            let queue_handle = DomainQueueHandle::new();
            let adapter_clone = adapter.clone();
            let states_clone = domain_states.clone();
            let timed_out_clone = timed_out.clone();
            let outcome_tx_clone = outcome_tx.clone();
            let pending_queue = queue_handle.pending.clone();
            let notify = queue_handle.notify.clone();
            let owner_gen_clone = owner_generation.clone();
            let owner_gen_notify_clone = owner_generation_notify.clone();

            let in_flight_clone = queue_handle.in_flight.clone();
            tokio::spawn(async move {
                loop {
                    let next_item = {
                        let mut guard = pending_queue.lock().unwrap();
                        guard.pop_front()
                    };

                    if let Some(first) = next_item {
                        in_flight_clone.store(true, Ordering::SeqCst);
                        let current_gen = owner_gen_clone.load(Ordering::SeqCst);
                        if first.generation != current_gen {
                            in_flight_clone.store(false, Ordering::SeqCst);
                            let outcome = CommandOutcome::Cancelled {
                                command_id: first.id,
                                arrival_sequence: first.arrival_sequence,
                                domain,
                                reason: CancellationReason::OwnerReplaced,
                            };
                            let _ = first.reply.send(outcome);
                            continue;
                        }

                        let current_cmd = first.command;
                        let current_id = first.id;
                        let current_sequence = first.arrival_sequence;

                        let outcome = Self::execute_single(
                            ExecContext {
                                adapter: &adapter_clone,
                                states: &states_clone,
                                timed_out: &timed_out_clone,
                                outcome_tx: &outcome_tx_clone,
                                owner_generation: current_gen,
                                owner_generation_source: &owner_gen_clone,
                                owner_generation_notify: &owner_gen_notify_clone,
                            },
                            current_id,
                            current_sequence,
                            current_cmd,
                        )
                        .await;

                        in_flight_clone.store(false, Ordering::SeqCst);
                        let _ = first.reply.send(outcome.clone());
                    } else {
                        notify.notified().await;
                    }
                }
            });

            domain_queues.insert(domain, queue_handle);
        }

        Self {
            adapter,
            next_arrival_sequence,
            owner_generation,
            owner_generation_notify,
            domain_queues,
            domain_states,
            timed_out,
            outcome_tx,
            overloads,
            supersessions,
            stale_updates,
            restarts,
            capacity,
        }
    }

    pub fn owner_generation(&self) -> u64 {
        self.owner_generation.load(Ordering::SeqCst)
    }

    pub fn increment_owner_generation(&self) -> u64 {
        let new_gen = self.owner_generation.fetch_add(1, Ordering::SeqCst) + 1;
        self.restarts.fetch_add(1, Ordering::SeqCst);
        for (domain, queue) in &self.domain_queues {
            let mut guard = queue.pending.lock().unwrap();
            for item in std::mem::take(&mut *guard) {
                let outcome = CommandOutcome::Cancelled {
                    command_id: item.id,
                    arrival_sequence: item.arrival_sequence,
                    domain: *domain,
                    reason: CancellationReason::OwnerReplaced,
                };
                let _ = item.reply.send(outcome);
            }
        }
        {
            let mut states = self.domain_states.lock().unwrap();
            for state in states.values_mut() {
                state.version = DomainVersion::new(new_gen, 0);
                state.lifecycle = DomainLifecycle::Reconnecting;
                state.error = Some("device owner replaced".into());
            }
        }
        self.owner_generation_notify.notify_waiters();
        new_gen
    }

    pub fn telemetry(&self) -> DomainPortTelemetry {
        let current_queue_depth: usize = self.domain_queues.values().map(|q| q.queue_depth()).sum();
        let last_error = self
            .domain_states
            .lock()
            .unwrap()
            .values()
            .find_map(|state| state.error.clone());
        DomainPortTelemetry {
            owner_generation: self.owner_generation.load(Ordering::SeqCst),
            current_queue_depth,
            queue_capacity: self.capacity,
            overloads: self.overloads.load(Ordering::SeqCst),

            supersessions: self.supersessions.load(Ordering::SeqCst),
            restarts: self.restarts.load(Ordering::SeqCst),
            stale_updates: self.stale_updates.load(Ordering::SeqCst),
            last_error,
        }
    }

    async fn execute_single(
        cx: ExecContext<'_>,
        id: CommandId,
        arrival_sequence: u64,
        command: DeviceCommand,
    ) -> CommandOutcome {
        let domain = command.domain();
        let _span = tracing::info_span!(
            target: "shilpo_profile",
            "service_command",
            domain = ?domain,
            operation = "execute",
            command_id = ?id,
            owner_generation = cx.owner_generation,
            outcome = "started",
        );
        let _enter = _span.enter();
        let before = cx
            .states
            .lock()
            .unwrap()
            .get(&domain)
            .cloned()
            .unwrap_or_else(|| cx.adapter.get_domain_state(domain));
        let timeout_duration = confirmation_timeout(domain);

        let exec_fut = cx.adapter.execute_command(command.clone());
        let execution = tokio::select! {
            result = tokio::time::timeout(timeout_duration, exec_fut) => result,
            _ = cx.owner_generation_notify.notified() => {
                return CommandOutcome::Cancelled {
                    command_id: id,
                    arrival_sequence,
                    domain,
                    reason: CancellationReason::OwnerReplaced,
                };
            }
        };
        if cx.owner_generation_source.load(Ordering::SeqCst) != cx.owner_generation {
            return CommandOutcome::Cancelled {
                command_id: id,
                arrival_sequence,
                domain,
                reason: CancellationReason::OwnerReplaced,
            };
        }
        match execution {
            Ok(Ok(_)) => {
                let started = tokio::time::Instant::now();
                loop {
                    let mut observed = cx.adapter.get_domain_state(domain);
                    if cx.owner_generation_source.load(Ordering::SeqCst) != cx.owner_generation {
                        return CommandOutcome::Cancelled {
                            command_id: id,
                            arrival_sequence,
                            domain,
                            reason: CancellationReason::OwnerReplaced,
                        };
                    }
                    if command_confirmed(&command, &before, &observed) {
                        observed.version =
                            DomainVersion::new(cx.owner_generation, observed.version.revision);
                        cx.states.lock().unwrap().insert(domain, observed.clone());
                        break CommandOutcome::Applied {
                            command_id: id.clone(),
                            arrival_sequence,
                            domain,
                            version: observed.version,
                        };
                    }
                    if started.elapsed() >= timeout_duration {
                        cx.timed_out
                            .lock()
                            .unwrap()
                            .entry(domain)
                            .or_default()
                            .push(TimedOutCommand {
                                id: id.clone(),
                                arrival_sequence,
                                command: command.clone(),
                                before: before.clone(),
                            });
                        let outcome = CommandOutcome::TimedOut {
                            command_id: id,
                            arrival_sequence,
                            domain,
                            last_observed_version: DomainVersion::new(
                                cx.owner_generation,
                                observed.version.revision,
                            ),
                        };
                        let _ = cx.outcome_tx.send(outcome.clone());
                        break outcome;
                    }
                    tokio::select! {
                        _ = tokio::time::sleep(Duration::from_millis(100)) => {}
                        _ = cx.owner_generation_notify.notified() => {
                            return CommandOutcome::Cancelled {
                                command_id: id,
                                arrival_sequence,
                                domain,
                                reason: CancellationReason::OwnerReplaced,
                            };
                        }
                    }
                }
            }
            Ok(Err(_err)) => {
                let mut guard = cx.states.lock().unwrap();
                if let Some(state) = guard.get_mut(&domain) {
                    state.lifecycle = DomainLifecycle::Degraded;
                    state.error = Some("Device command rejected".to_string());
                }
                CommandOutcome::Rejected {
                    command_id: id,
                    arrival_sequence,
                    domain,
                    reason: RejectionReason::Unavailable,
                }
            }
            Err(_) => {
                let observed = cx.adapter.get_domain_state(domain);
                let mut guard = cx.states.lock().unwrap();
                if let Some(state) = guard.get_mut(&domain) {
                    state.lifecycle = DomainLifecycle::Degraded;
                    state.error = Some("Command timed out".into());
                }
                cx.timed_out
                    .lock()
                    .unwrap()
                    .entry(domain)
                    .or_default()
                    .push(TimedOutCommand {
                        id: id.clone(),
                        arrival_sequence,
                        command,
                        before,
                    });
                CommandOutcome::TimedOut {
                    command_id: id,
                    arrival_sequence,
                    domain,
                    last_observed_version: DomainVersion::new(
                        cx.owner_generation,
                        observed.version.revision,
                    ),
                }
            }
        }
    }

    pub fn get_domain_state(&self, domain: DeviceDomain) -> DomainState {
        self.domain_states
            .lock()
            .unwrap()
            .get(&domain)
            .cloned()
            .unwrap_or_else(|| self.adapter.get_domain_state(domain))
    }

    /// Reconciles adapter observations into the daemon-owned projection and
    /// returns only domains whose payload or lifecycle changed.
    pub fn refresh_domain_states(&self) -> Vec<DomainState> {
        let current_gen = self.owner_generation.load(Ordering::SeqCst);
        let mut changed = Vec::new();
        let mut states = self.domain_states.lock().unwrap();
        for domain in DeviceDomain::ALL {
            let observed = self.adapter.get_domain_state(domain);
            let previous = states.get(&domain);
            if previous.is_none_or(|state| {
                state.payload != observed.payload
                    || state.lifecycle != observed.lifecycle
                    || state.error != observed.error
            }) {
                let revision = previous.map_or(observed.version.revision, |state| {
                    state.version.revision.saturating_add(1)
                });
                let state = DomainState {
                    version: DomainVersion::new(current_gen, revision),
                    ..observed
                };
                states.insert(domain, state.clone());
                let mut pending = self.timed_out.lock().unwrap();
                if let Some(commands) = pending.get_mut(&domain) {
                    commands.retain(|command| {
                        if command_confirmed(&command.command, &command.before, &state) {
                            let _ = self.outcome_tx.send(CommandOutcome::ReconciledApplied {
                                command_id: command.id.clone(),
                                arrival_sequence: command.arrival_sequence,
                                domain,
                                version: state.version,
                            });
                            false
                        } else {
                            true
                        }
                    });
                }
                changed.push(state);
            }
        }
        changed
    }

    pub fn subscribe_outcomes(&self) -> broadcast::Receiver<CommandOutcome> {
        self.outcome_tx.subscribe()
    }

    pub fn domain_state_if_revision(
        &self,
        domain: DeviceDomain,
        outcome: &crate::device_protocol::CommandOutcome,
    ) -> Option<DomainState> {
        let version = match outcome {
            crate::device_protocol::CommandOutcome::Applied { version, .. }
            | crate::device_protocol::CommandOutcome::ReconciledApplied { version, .. } => *version,
            _ => return None,
        };
        let state = self.get_domain_state(domain);
        (state.version == version).then_some(state)
    }

    pub fn submit_command(
        &self,
        command: DeviceCommand,
        client_protocol_version: u32,
    ) -> Result<(CommandId, oneshot::Receiver<CommandOutcome>), String> {
        check_protocol_version(client_protocol_version)
            .map_err(|err| format!("Protocol version error: {err}"))?;

        let domain = command.domain();
        let queue_handle = self
            .domain_queues
            .get(&domain)
            .ok_or_else(|| format!("No queue for domain {domain:?}"))?;

        let arrival_sequence = self.next_arrival_sequence.fetch_add(1, Ordering::SeqCst);
        let generation = self.owner_generation.load(Ordering::SeqCst);
        let id = CommandId(uuid::Uuid::new_v4().to_string());
        let (reply_tx, reply_rx) = oneshot::channel();

        let mut queue = queue_handle.pending.lock().unwrap();

        // Check coalescing key / replace-latest policy
        if let Some(key) = command.coalescing_key() {
            let mut replaced_idx = None;
            for (idx, item) in queue.iter().enumerate() {
                if let Some(existing_key) = item.command.coalescing_key()
                    && existing_key == key
                {
                    replaced_idx = Some(idx);
                    break;
                }
            }
            if let Some(idx) = replaced_idx {
                let old_item = queue.remove(idx).unwrap();
                let outcome = CommandOutcome::Cancelled {
                    command_id: old_item.id,
                    arrival_sequence: old_item.arrival_sequence,
                    domain,
                    reason: CancellationReason::Superseded,
                };
                let _ = old_item.reply.send(outcome);
                self.supersessions.fetch_add(1, Ordering::SeqCst);
            }
        }

        // Bounded capacity overflow check
        let in_flight_count = if queue_handle.in_flight.load(Ordering::SeqCst) {
            1
        } else {
            0
        };
        if queue.len() + in_flight_count >= self.capacity {
            self.overloads.fetch_add(1, Ordering::SeqCst);

            let outcome = CommandOutcome::Rejected {
                command_id: id.clone(),
                arrival_sequence,
                domain,
                reason: RejectionReason::Overloaded,
            };
            let _ = reply_tx.send(outcome);
            return Ok((id, reply_rx));
        }

        queue.push_back(PendingDeviceCommand {
            id: id.clone(),
            arrival_sequence,
            generation,
            command,
            reply: reply_tx,
        });

        queue_handle.notify.notify_one();
        Ok((id, reply_rx))
    }
}

fn confirmation_timeout(domain: DeviceDomain) -> Duration {
    #[cfg(test)]
    {
        let _ = domain;
        Duration::from_millis(100)
    }
    #[cfg(not(test))]
    match domain {
        DeviceDomain::Audio | DeviceDomain::Brightness => Duration::from_secs(3),
        _ => Duration::from_secs(10),
    }
}

fn command_confirmed(command: &DeviceCommand, before: &DomainState, after: &DomainState) -> bool {
    use crate::device_protocol::DomainPayload as P;
    use crate::device_protocol::{
        AudioAction, BluetoothAction, BrightnessAction, CaffeineAction, MediaAction, NetworkAction,
        NightLightAction, PowerProfileAction,
    };
    match (command, &before.payload, &after.payload) {
        (DeviceCommand::Audio(AudioAction::SetVolume(v)), _, P::Audio(new)) => new.volume == *v,
        (DeviceCommand::Audio(AudioAction::SetMuted(v)), _, P::Audio(new)) => new.is_muted == *v,
        (DeviceCommand::Audio(AudioAction::ToggleMute), P::Audio(old), P::Audio(new)) => {
            new.is_muted != old.is_muted
        }
        (DeviceCommand::Brightness(BrightnessAction::SetBrightness(v)), _, P::Brightness(new)) => {
            new.percentage == *v
        }
        (
            DeviceCommand::Brightness(BrightnessAction::SetDisplay { percentage, .. }),
            _,
            P::Brightness(new),
        ) => new.percentage == *percentage,
        (
            DeviceCommand::Brightness(BrightnessAction::StepUp),
            P::Brightness(old),
            P::Brightness(new),
        ) => new.percentage > old.percentage,
        (
            DeviceCommand::Brightness(BrightnessAction::StepDown),
            P::Brightness(old),
            P::Brightness(new),
        ) => new.percentage < old.percentage,
        (DeviceCommand::Bluetooth(BluetoothAction::SetPowered(v)), _, P::Bluetooth(new)) => {
            new.powered == *v
        }
        (
            DeviceCommand::Bluetooth(BluetoothAction::TogglePowered),
            P::Bluetooth(old),
            P::Bluetooth(new),
        ) => new.powered != old.powered,
        (DeviceCommand::Bluetooth(BluetoothAction::Connect(address)), _, P::Bluetooth(new)) => {
            device_connected(new, address)
        }
        (DeviceCommand::Bluetooth(BluetoothAction::Disconnect(address)), _, P::Bluetooth(new)) => {
            !device_connected(new, address)
        }
        (DeviceCommand::Network(NetworkAction::SetWifiEnabled(v)), _, P::Network(new)) => {
            new.wifi_enabled == *v
        }
        (DeviceCommand::Network(NetworkAction::ToggleWifi), P::Network(old), P::Network(new)) => {
            new.wifi_enabled != old.wifi_enabled
        }
        (DeviceCommand::Network(NetworkAction::ConnectWifi(ssid)), _, P::Network(new)) => {
            new.ssid == *ssid
        }
        (DeviceCommand::Network(NetworkAction::ConnectVpn(name)), _, P::Network(new)) => {
            vpn_active(new, name)
        }
        (DeviceCommand::Network(NetworkAction::DisconnectVpn(name)), _, P::Network(new)) => {
            !vpn_active(new, name)
        }
        (DeviceCommand::NightLight(NightLightAction::SetEnabled(v)), _, P::NightLight(new)) => {
            new.enabled == *v
        }
        (
            DeviceCommand::NightLight(NightLightAction::ToggleEnabled),
            P::NightLight(old),
            P::NightLight(new),
        ) => new.enabled != old.enabled,
        (DeviceCommand::NightLight(NightLightAction::SetTemperature(v)), _, P::NightLight(new)) => {
            new.temperature == *v
        }
        (
            DeviceCommand::PowerProfile(PowerProfileAction::SetProfile(profile)),
            _,
            P::PowerProfile(new),
        ) => new.profile == *profile,
        (DeviceCommand::Media(MediaAction::Play), _, P::Media(new)) => {
            new.playback_state == "playing"
        }
        (DeviceCommand::Media(MediaAction::Pause), _, P::Media(new)) => {
            new.playback_state == "paused"
        }
        (DeviceCommand::Media(MediaAction::PlayPause), P::Media(old), P::Media(new)) => {
            new.playback_state != old.playback_state
        }
        (
            DeviceCommand::Media(MediaAction::Next | MediaAction::Previous),
            P::Media(old),
            P::Media(new),
        ) => new.title != old.title || new.player_id != old.player_id,
        (DeviceCommand::Caffeine(CaffeineAction::SetEnabled(v)), _, P::Caffeine(new)) => {
            new.enabled == *v
        }
        (DeviceCommand::Caffeine(CaffeineAction::Toggle), P::Caffeine(old), P::Caffeine(new)) => {
            new.enabled != old.enabled
        }
        _ => false,
    }
}

fn device_connected(payload: &crate::device_protocol::BluetoothPayload, address: &str) -> bool {
    payload
        .devices
        .iter()
        .any(|device| device.address == address && device.connected)
}

fn vpn_active(payload: &crate::device_protocol::NetworkPayload, name: &str) -> bool {
    payload
        .active_vpns
        .iter()
        .any(|vpn| (vpn.id == name || vpn.uuid == name) && vpn.is_active)
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;
    use crate::device_protocol::{AudioAction, BrightnessAction, DomainPayload, PROTOCOL_VERSION};

    #[derive(Clone)]
    struct DeferredAdapter {
        state: Arc<Mutex<DomainState>>,
        apply_after: Option<Duration>,
    }

    impl DeferredAdapter {
        fn new(apply_after: Option<Duration>) -> Self {
            Self {
                state: Arc::new(Mutex::new(DomainState {
                    domain: DeviceDomain::Audio,
                    version: DomainVersion::new(1, 1),
                    lifecycle: DomainLifecycle::Ready,
                    payload: DomainPayload::Audio(crate::device_protocol::AudioPayload {
                        volume: 50,
                        ..Default::default()
                    }),
                    error: None,
                })),
                apply_after,
            }
        }
    }

    impl DeviceAdapter for DeferredAdapter {
        fn name(&self) -> &'static str {
            "deferred-test"
        }

        fn get_domain_state(&self, domain: DeviceDomain) -> DomainState {
            if domain == DeviceDomain::Audio {
                self.state.lock().unwrap().clone()
            } else {
                DomainState {
                    domain,
                    version: DomainVersion::new(1, 1),
                    lifecycle: DomainLifecycle::Ready,
                    payload: DomainPayload::empty(domain),
                    error: None,
                }
            }
        }

        fn execute_command(
            &self,
            command: DeviceCommand,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<DomainState, String>> + Send + 'static>,
        > {
            let state = self.state.clone();
            let apply_after = self.apply_after;
            let current = state.lock().unwrap().clone();
            if let (Some(delay), DeviceCommand::Audio(AudioAction::SetVolume(volume))) =
                (apply_after, command)
            {
                tokio::spawn(async move {
                    tokio::time::sleep(delay).await;
                    let mut state = state.lock().unwrap();
                    state.version.revision += 1;
                    if let DomainPayload::Audio(payload) = &mut state.payload {
                        payload.volume = volume;
                    }
                });
            }
            Box::pin(async move { Ok(current) })
        }
    }

    fn outcome_identity(outcome: &CommandOutcome) -> (&CommandId, u64) {
        match outcome {
            CommandOutcome::Applied {
                command_id,
                arrival_sequence,
                ..
            }
            | CommandOutcome::Rejected {
                command_id,
                arrival_sequence,
                ..
            }
            | CommandOutcome::TimedOut {
                command_id,
                arrival_sequence,
                ..
            }
            | CommandOutcome::Cancelled {
                command_id,
                arrival_sequence,
                ..
            }
            | CommandOutcome::ReconciledApplied {
                command_id,
                arrival_sequence,
                ..
            } => (command_id, *arrival_sequence),
        }
    }

    #[tokio::test]
    async fn test_protocol_version_negotiation() {
        let adapter = Arc::new(InMemoryDeviceAdapter::new());
        let service = DeviceDaemonService::new(adapter);

        let cmd = DeviceCommand::Audio(AudioAction::SetVolume(80));
        assert!(
            service
                .submit_command(cmd.clone(), PROTOCOL_VERSION)
                .is_ok()
        );

        let err = service.submit_command(cmd, 99).unwrap_err();
        assert!(err.contains("version mismatch"));
    }

    #[tokio::test]
    async fn test_per_domain_independent_execution_and_coalescing() {
        let adapter = Arc::new(InMemoryDeviceAdapter::new());
        let service = DeviceDaemonService::new(adapter.clone());

        let cmd1 = DeviceCommand::Audio(AudioAction::SetVolume(20));
        let cmd2 = DeviceCommand::Audio(AudioAction::SetVolume(40));

        let (_id1, rx1) = service.submit_command(cmd1, PROTOCOL_VERSION).unwrap();
        let (_id2, rx2) = service.submit_command(cmd2, PROTOCOL_VERSION).unwrap();

        let outcome1 = rx1.await.unwrap();
        let outcome2 = rx2.await.unwrap();

        assert!(matches!(
            outcome1,
            CommandOutcome::Applied { .. }
                | CommandOutcome::Rejected { .. }
                | CommandOutcome::Cancelled { .. }
        ));
        assert!(matches!(outcome2, CommandOutcome::Applied { .. }));
        let (id1, sequence1) = outcome_identity(&outcome1);
        let (id2, sequence2) = outcome_identity(&outcome2);
        assert_ne!(id1, id2);
        assert!(sequence1 < sequence2);
        assert!(uuid::Uuid::parse_str(&id1.0).is_ok());
        assert!(uuid::Uuid::parse_str(&id2.0).is_ok());

        let audio_state = service.get_domain_state(DeviceDomain::Audio);
        assert!(matches!(
            audio_state.payload,
            crate::device_protocol::DomainPayload::Audio(payload) if payload.volume == 40
        ));
    }

    #[tokio::test]
    async fn test_command_timeout_and_domain_degradation() {
        let adapter = Arc::new(InMemoryDeviceAdapter::new());
        adapter.set_forced_delay(Some(Duration::from_secs(5)));

        let service = DeviceDaemonService::new(adapter);
        let cmd = DeviceCommand::Audio(AudioAction::SetVolume(90));

        let (_id, rx) = service.submit_command(cmd, PROTOCOL_VERSION).unwrap();
        let outcome = rx.await.unwrap();

        assert!(matches!(outcome, CommandOutcome::TimedOut { .. }));
        let audio_state = service.get_domain_state(DeviceDomain::Audio);
        assert_eq!(audio_state.lifecycle, DomainLifecycle::Degraded);
    }

    #[tokio::test]
    async fn applied_requires_observed_state_confirmation() {
        let service = DeviceDaemonService::new(Arc::new(DeferredAdapter::new(None)));
        let (_, reply) = service
            .submit_command(
                DeviceCommand::Audio(AudioAction::SetVolume(90)),
                PROTOCOL_VERSION,
            )
            .unwrap();

        assert!(matches!(
            reply.await.unwrap(),
            CommandOutcome::TimedOut { .. }
        ));
    }

    #[tokio::test]
    async fn timed_out_command_is_reconciled_when_state_arrives_late() {
        let service = DeviceDaemonService::new(Arc::new(DeferredAdapter::new(Some(
            Duration::from_millis(150),
        ))));
        let mut outcomes = service.subscribe_outcomes();
        let (_, reply) = service
            .submit_command(
                DeviceCommand::Audio(AudioAction::SetVolume(90)),
                PROTOCOL_VERSION,
            )
            .unwrap();
        let timeout = reply.await.unwrap();
        let (timed_out_id, timed_out_sequence) = outcome_identity(&timeout);
        tokio::time::sleep(Duration::from_millis(75)).await;
        let states = service.refresh_domain_states();
        assert_eq!(states.len(), 1);

        let reconciled = loop {
            let outcome = outcomes.recv().await.unwrap();
            if matches!(outcome, CommandOutcome::ReconciledApplied { .. }) {
                break outcome;
            }
        };
        assert!(matches!(
            reconciled,
            CommandOutcome::ReconciledApplied {
                command_id,
                arrival_sequence,
                version,
                ..
            } if &command_id == timed_out_id && arrival_sequence == timed_out_sequence && version.revision == 2
        ));
    }

    #[tokio::test]
    async fn different_domains_execute_concurrently() {
        let adapter = Arc::new(InMemoryDeviceAdapter::new());
        adapter.set_forced_delay(Some(Duration::from_millis(50)));
        let service = DeviceDaemonService::new(adapter);
        let started = tokio::time::Instant::now();
        let (_, audio) = service
            .submit_command(
                DeviceCommand::Audio(AudioAction::SetVolume(60)),
                PROTOCOL_VERSION,
            )
            .unwrap();
        let (_, brightness) = service
            .submit_command(
                DeviceCommand::Brightness(BrightnessAction::SetBrightness(60)),
                PROTOCOL_VERSION,
            )
            .unwrap();
        let (audio, brightness) = tokio::join!(audio, brightness);
        assert!(matches!(audio.unwrap(), CommandOutcome::Applied { .. }));
        assert!(matches!(
            brightness.unwrap(),
            CommandOutcome::Applied { .. }
        ));
        assert!(started.elapsed() < Duration::from_millis(90));
    }

    #[tokio::test]
    async fn owner_generation_increment_cancels_pending_commands() {
        let adapter = Arc::new(InMemoryDeviceAdapter::new());
        adapter.set_forced_delay(Some(Duration::from_millis(200)));
        let service = DeviceDaemonService::new(adapter);

        let (_id1, rx1) = service
            .submit_command(
                DeviceCommand::Audio(AudioAction::ToggleMute),
                PROTOCOL_VERSION,
            )
            .unwrap();
        let (_id2, rx2) = service
            .submit_command(
                DeviceCommand::Audio(AudioAction::ToggleMute),
                PROTOCOL_VERSION,
            )
            .unwrap();

        let new_gen = service.increment_owner_generation();
        assert_eq!(new_gen, 2);
        let reset_state = service.get_domain_state(DeviceDomain::Audio);
        assert_eq!(reset_state.version, DomainVersion::new(2, 0));
        assert_eq!(reset_state.lifecycle, DomainLifecycle::Reconnecting);

        let outcome2 = rx2.await.unwrap();
        let outcome1 = rx1.await.unwrap();
        assert!(matches!(
            outcome1,
            CommandOutcome::Cancelled {
                reason: CancellationReason::OwnerReplaced,
                ..
            }
        ));
        assert!(matches!(
            outcome2,
            CommandOutcome::Cancelled {
                reason: CancellationReason::OwnerReplaced,
                ..
            }
        ));
    }

    #[tokio::test]
    async fn bounded_lossless_overflow_rejects_with_overloaded() {
        let adapter = Arc::new(InMemoryDeviceAdapter::new());
        adapter.set_forced_delay(Some(Duration::from_secs(10)));
        let service = DeviceDaemonService::new(adapter);

        // Submit 1 in-flight + 16 queued non-coalescible commands
        let mut rxs = Vec::new();
        for _ in 0..17 {
            let (_, rx) = service
                .submit_command(
                    DeviceCommand::Audio(AudioAction::ToggleMute),
                    PROTOCOL_VERSION,
                )
                .unwrap();
            rxs.push(rx);
        }

        // The 18th command should be rejected as Overloaded
        let (_, rx_overflow) = service
            .submit_command(
                DeviceCommand::Audio(AudioAction::ToggleMute),
                PROTOCOL_VERSION,
            )
            .unwrap();

        let overflow_outcome = rx_overflow.await.unwrap();
        assert!(matches!(
            overflow_outcome,
            CommandOutcome::Rejected {
                reason: RejectionReason::Overloaded,
                ..
            }
        ));

        let telemetry = service.telemetry();
        assert!(telemetry.overloads >= 1);
    }
}
