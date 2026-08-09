pub mod dbus;
pub mod in_memory_adapter;
pub mod system_adapter;

pub use dbus::DeviceDbusService;
pub use in_memory_adapter::{DeviceAdapter, InMemoryDeviceAdapter};
use shilpo_device_protocol::{
    CommandId, CommandOutcome, DeviceCommand, DeviceDomain, DomainLifecycle, DomainState,
    check_protocol_version,
};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
pub use system_adapter::SystemDeviceAdapter;
use tokio::sync::{broadcast, mpsc, oneshot};

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
    pub command: DeviceCommand,
    pub reply: oneshot::Sender<CommandOutcome>,
}

pub struct DeviceDaemonService {
    adapter: Arc<dyn DeviceAdapter>,
    next_arrival_sequence: Arc<AtomicU64>,
    domain_queues: HashMap<DeviceDomain, mpsc::UnboundedSender<PendingDeviceCommand>>,
    domain_states: Arc<std::sync::Mutex<HashMap<DeviceDomain, DomainState>>>,
    timed_out: Arc<std::sync::Mutex<HashMap<DeviceDomain, Vec<TimedOutCommand>>>>,
    outcome_tx: broadcast::Sender<CommandOutcome>,
}

impl DeviceDaemonService {
    pub fn new(adapter: Arc<dyn DeviceAdapter>) -> Self {
        let next_arrival_sequence = Arc::new(AtomicU64::new(1));
        let domain_states = Arc::new(std::sync::Mutex::new(HashMap::new()));
        let timed_out = Arc::new(std::sync::Mutex::new(HashMap::new()));
        let (outcome_tx, _) = broadcast::channel(128);
        let mut domain_queues = HashMap::new();

        for domain in DeviceDomain::ALL {
            domain_states
                .lock()
                .unwrap()
                .insert(domain, adapter.get_domain_state(domain));

            let (tx, mut rx) = mpsc::unbounded_channel::<PendingDeviceCommand>();
            let adapter_clone = adapter.clone();
            let states_clone = domain_states.clone();
            let timed_out_clone = timed_out.clone();
            let outcome_tx_clone = outcome_tx.clone();

            tokio::spawn(async move {
                while let Some(first) = rx.recv().await {
                    let mut current_cmd = first.command;
                    let mut current_id = first.id;
                    let mut current_sequence = first.arrival_sequence;
                    let mut current_replies =
                        vec![(current_id.clone(), first.arrival_sequence, first.reply)];

                    // Coalesce set-value commands if available in queue
                    if current_cmd.is_coalescable() {
                        while let Ok(next) = rx.try_recv() {
                            if next.command.is_coalescable()
                                && next.command.domain() == current_cmd.domain()
                            {
                                for (command_id, sequence, reply) in current_replies.drain(..) {
                                    let _ = reply.send(CommandOutcome::Rejected {
                                        command_id,
                                        arrival_sequence: sequence,
                                        domain,
                                        reason: "superseded by a newer set-value command".into(),
                                    });
                                }
                                current_cmd = next.command;
                                current_id = next.id.clone();
                                current_sequence = next.arrival_sequence;
                                current_replies =
                                    vec![(next.id, next.arrival_sequence, next.reply)];
                            } else {
                                let outcome = Self::execute_single(
                                    &adapter_clone,
                                    &states_clone,
                                    &timed_out_clone,
                                    &outcome_tx_clone,
                                    current_id.clone(),
                                    current_sequence,
                                    current_cmd,
                                )
                                .await;

                                for (command_id, sequence, reply) in current_replies {
                                    let _ = reply
                                        .send(outcome.clone().with_command(command_id, sequence));
                                }

                                current_cmd = next.command;
                                current_id = next.id;
                                current_sequence = next.arrival_sequence;
                                current_replies =
                                    vec![(current_id.clone(), current_sequence, next.reply)];
                                break;
                            }
                        }
                    }

                    let outcome = Self::execute_single(
                        &adapter_clone,
                        &states_clone,
                        &timed_out_clone,
                        &outcome_tx_clone,
                        current_id.clone(),
                        current_sequence,
                        current_cmd,
                    )
                    .await;

                    for (command_id, sequence, reply) in current_replies {
                        let _ = reply.send(outcome.clone().with_command(command_id, sequence));
                    }
                }
            });

            domain_queues.insert(domain, tx);
        }

        Self {
            adapter,
            next_arrival_sequence,
            domain_queues,
            domain_states,
            timed_out,
            outcome_tx,
        }
    }

    async fn execute_single(
        adapter: &Arc<dyn DeviceAdapter>,
        states: &Arc<std::sync::Mutex<HashMap<DeviceDomain, DomainState>>>,
        timed_out: &Arc<std::sync::Mutex<HashMap<DeviceDomain, Vec<TimedOutCommand>>>>,
        outcome_tx: &broadcast::Sender<CommandOutcome>,
        id: CommandId,
        arrival_sequence: u64,
        command: DeviceCommand,
    ) -> CommandOutcome {
        let domain = command.domain();
        let before = states
            .lock()
            .unwrap()
            .get(&domain)
            .cloned()
            .unwrap_or_else(|| adapter.get_domain_state(domain));
        let timeout_duration = confirmation_timeout(domain);

        let exec_fut = adapter.execute_command(command.clone());
        match tokio::time::timeout(timeout_duration, exec_fut).await {
            Ok(Ok(_)) => {
                let started = tokio::time::Instant::now();
                loop {
                    let observed = adapter.get_domain_state(domain);
                    if command_confirmed(&command, &before, &observed) {
                        states.lock().unwrap().insert(domain, observed.clone());
                        break CommandOutcome::Applied {
                            command_id: id.clone(),
                            arrival_sequence,
                            domain,
                            revision: observed.revision,
                        };
                    }
                    if started.elapsed() >= timeout_duration {
                        timed_out.lock().unwrap().entry(domain).or_default().push(
                            TimedOutCommand {
                                id: id.clone(),
                                arrival_sequence,
                                command: command.clone(),
                                before: before.clone(),
                            },
                        );
                        let outcome = CommandOutcome::Timeout {
                            command_id: id,
                            arrival_sequence,
                            domain,
                        };
                        let _ = outcome_tx.send(outcome.clone());
                        break outcome;
                    }
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            }
            Ok(Err(err)) => {
                let mut guard = states.lock().unwrap();
                if let Some(state) = guard.get_mut(&domain) {
                    state.lifecycle = DomainLifecycle::Degraded;
                    state.error = Some(err.clone());
                }
                CommandOutcome::Rejected {
                    command_id: id,
                    arrival_sequence,
                    domain,
                    reason: err,
                }
            }
            Err(_) => {
                let mut guard = states.lock().unwrap();
                if let Some(state) = guard.get_mut(&domain) {
                    state.lifecycle = DomainLifecycle::Degraded;
                    state.error = Some("Command timed out".into());
                }
                timed_out
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
                CommandOutcome::Timeout {
                    command_id: id,
                    arrival_sequence,
                    domain,
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
                let revision =
                    previous.map_or(observed.revision, |state| state.revision.saturating_add(1));
                let state = DomainState {
                    revision,
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
                                revision: state.revision,
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
        outcome: &shilpo_device_protocol::CommandOutcome,
    ) -> Option<DomainState> {
        let revision = match outcome {
            shilpo_device_protocol::CommandOutcome::Applied { revision, .. } => *revision,
            _ => return None,
        };
        let state = self.get_domain_state(domain);
        (state.revision == revision).then_some(state)
    }

    pub fn submit_command(
        &self,
        command: DeviceCommand,
        client_protocol_version: u32,
    ) -> Result<(CommandId, oneshot::Receiver<CommandOutcome>), String> {
        check_protocol_version(client_protocol_version)
            .map_err(|err| format!("Protocol version error: {err}"))?;

        let domain = command.domain();
        let queue = self
            .domain_queues
            .get(&domain)
            .ok_or_else(|| format!("No queue for domain {domain:?}"))?;

        let arrival_sequence = self.next_arrival_sequence.fetch_add(1, Ordering::SeqCst);
        let id = CommandId(uuid::Uuid::new_v4().to_string());
        let (reply_tx, reply_rx) = oneshot::channel();

        queue
            .send(PendingDeviceCommand {
                id: id.clone(),
                arrival_sequence,
                command,
                reply: reply_tx,
            })
            .map_err(|_| "Domain queue channel closed".to_string())?;

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
    use shilpo_device_protocol::DomainPayload as P;
    use shilpo_device_protocol::{
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

fn device_connected(payload: &shilpo_device_protocol::BluetoothPayload, address: &str) -> bool {
    payload
        .devices
        .iter()
        .any(|device| device.address == address && device.connected)
}

fn vpn_active(payload: &shilpo_device_protocol::NetworkPayload, name: &str) -> bool {
    payload
        .active_vpns
        .iter()
        .any(|vpn| (vpn.id == name || vpn.uuid == name) && vpn.is_active)
}

#[cfg(test)]
mod tests {
    use super::*;
    use shilpo_device_protocol::{AudioAction, BrightnessAction, DomainPayload};
    use std::sync::Mutex;

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
                    revision: 1,
                    lifecycle: DomainLifecycle::Ready,
                    payload: DomainPayload::Audio(shilpo_device_protocol::AudioPayload {
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
                    revision: 1,
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
                    state.revision += 1;
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
            | CommandOutcome::Timeout {
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
        assert!(service.submit_command(cmd.clone(), 1).is_ok());

        let err = service.submit_command(cmd, 99).unwrap_err();
        assert!(err.contains("version mismatch"));
    }

    #[tokio::test]
    async fn test_per_domain_independent_execution_and_coalescing() {
        let adapter = Arc::new(InMemoryDeviceAdapter::new());
        let service = DeviceDaemonService::new(adapter.clone());

        let cmd1 = DeviceCommand::Audio(AudioAction::SetVolume(20));
        let cmd2 = DeviceCommand::Audio(AudioAction::SetVolume(40));

        let (_id1, rx1) = service.submit_command(cmd1, 1).unwrap();
        let (_id2, rx2) = service.submit_command(cmd2, 1).unwrap();

        let outcome1 = rx1.await.unwrap();
        let outcome2 = rx2.await.unwrap();

        assert!(matches!(
            outcome1,
            CommandOutcome::Applied { .. } | CommandOutcome::Rejected { .. }
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
            shilpo_device_protocol::DomainPayload::Audio(payload) if payload.volume == 40
        ));
    }

    #[tokio::test]
    async fn test_command_timeout_and_domain_degradation() {
        let adapter = Arc::new(InMemoryDeviceAdapter::new());
        adapter.set_forced_delay(Some(Duration::from_secs(5)));

        let service = DeviceDaemonService::new(adapter);
        let cmd = DeviceCommand::Audio(AudioAction::SetVolume(90));

        let (_id, rx) = service.submit_command(cmd, 1).unwrap();
        let outcome = rx.await.unwrap();

        assert!(matches!(outcome, CommandOutcome::Timeout { .. }));
        let audio_state = service.get_domain_state(DeviceDomain::Audio);
        assert_eq!(audio_state.lifecycle, DomainLifecycle::Degraded);
    }

    #[tokio::test]
    async fn applied_requires_observed_state_confirmation() {
        let service = DeviceDaemonService::new(Arc::new(DeferredAdapter::new(None)));
        let (_, reply) = service
            .submit_command(DeviceCommand::Audio(AudioAction::SetVolume(90)), 1)
            .unwrap();

        assert!(matches!(
            reply.await.unwrap(),
            CommandOutcome::Timeout { .. }
        ));
    }

    #[tokio::test]
    async fn timed_out_command_is_reconciled_when_state_arrives_late() {
        let service = DeviceDaemonService::new(Arc::new(DeferredAdapter::new(Some(
            Duration::from_millis(150),
        ))));
        let mut outcomes = service.subscribe_outcomes();
        let (_, reply) = service
            .submit_command(DeviceCommand::Audio(AudioAction::SetVolume(90)), 1)
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
                revision: 2,
                ..
            } if &command_id == timed_out_id && arrival_sequence == timed_out_sequence
        ));
    }

    #[tokio::test]
    async fn different_domains_execute_concurrently() {
        let adapter = Arc::new(InMemoryDeviceAdapter::new());
        adapter.set_forced_delay(Some(Duration::from_millis(50)));
        let service = DeviceDaemonService::new(adapter);
        let started = tokio::time::Instant::now();
        let (_, audio) = service
            .submit_command(DeviceCommand::Audio(AudioAction::SetVolume(60)), 1)
            .unwrap();
        let (_, brightness) = service
            .submit_command(
                DeviceCommand::Brightness(BrightnessAction::SetBrightness(60)),
                1,
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
}
