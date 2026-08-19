mod support;

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use shilpo_services::device_protocol::{
    AudioAction, AudioPayload, CancellationReason as DeviceCancellationReason,
    CommandOutcome as DeviceCommandOutcome, DeviceCommand, DeviceDomain, DomainLifecycle,
    DomainPayload, DomainPortTelemetry, DomainState, DomainVersion, PROTOCOL_VERSION,
    RejectionReason as DeviceRejectionReason,
};
use shilpo_services::{
    DeviceAdapter, DeviceClient, DeviceDaemonService, InMemoryDeviceAdapter, StaleUpdateError,
    SupervisorState, TimeSource,
};
use support::domain_port_contract::{
    self, CancellationReason, CommandId, CommandOutcome, CommandResolver, CommandTicket,
    DomainPortDriver, DomainSnapshot, ManualClock, RejectionReason, SnapshotSubscription,
};

// ---------------------------------------------------------------------------
// Delayed Confirmation Adapter for Scenario 11
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct DelayedConfirmAdapter {
    inner: InMemoryDeviceAdapter,
    confirmed: Arc<AtomicBool>,
}

impl DelayedConfirmAdapter {
    pub fn new() -> Self {
        Self {
            inner: InMemoryDeviceAdapter::new(),
            confirmed: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn confirm(&self) {
        self.confirmed.store(true, Ordering::SeqCst);
    }
}

impl Default for DelayedConfirmAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl DeviceAdapter for DelayedConfirmAdapter {
    fn name(&self) -> &'static str {
        "delayed-confirm-test-adapter"
    }

    fn execute_command(
        &self,
        command: DeviceCommand,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<DomainState, String>> + Send + 'static>,
    > {
        let placeholder = self.inner.get_domain_state(command.domain());
        let inner = self.inner.clone();
        Box::pin(async move {
            let _ = inner.execute_command(command).await;
            Ok(placeholder)
        })
    }

    fn get_domain_state(&self, domain: DeviceDomain) -> DomainState {
        if domain == DeviceDomain::Audio && !self.confirmed.load(Ordering::SeqCst) {
            return DomainState {
                domain: DeviceDomain::Audio,
                version: DomainVersion::new(1, 1),
                lifecycle: DomainLifecycle::Ready,
                payload: DomainPayload::Audio(AudioPayload::default()),
                error: None,
            };
        }
        self.inner.get_domain_state(domain)
    }
}

// ---------------------------------------------------------------------------
// Time Source Adapter for Manual Clock
// ---------------------------------------------------------------------------

struct ManualTimeSource(ManualClock);

impl TimeSource for ManualTimeSource {
    fn now_ms(&self) -> u64 {
        self.0.now_ms()
    }
}

struct DriverPendingItem {
    id: CommandId,
    replace_key: Option<String>,
    ticket: CommandTicket,
    resolver: CommandResolver,
}

// ---------------------------------------------------------------------------
// Device Domain Port Driver
// ---------------------------------------------------------------------------

pub struct DeviceDomainPortDriver {
    capacity: usize,
    client: Mutex<Arc<DeviceClient>>,
    daemon: Mutex<Arc<DeviceDaemonService>>,
    clock: ManualClock,
    in_memory_adapter: Mutex<Option<Arc<InMemoryDeviceAdapter>>>,
    delayed_adapter: Option<DelayedConfirmAdapter>,
    next_ids: Arc<Mutex<VecDeque<CommandId>>>,
    driver_supersessions: Arc<std::sync::atomic::AtomicU64>,
    pending: Arc<Mutex<VecDeque<DriverPendingItem>>>,
}

impl DeviceDomainPortDriver {
    pub fn new(capacity: usize) -> Self {
        let clock = ManualClock::new();
        let time_source: Arc<dyn TimeSource> = Arc::new(ManualTimeSource(clock.clone()));
        let client = Arc::new(DeviceClient::new_with_time_source(time_source));
        let adapter = Arc::new(InMemoryDeviceAdapter::new());
        adapter.set_forced_delay(Some(Duration::from_secs(10)));
        let daemon = Arc::new(DeviceDaemonService::new_with_capacity(
            capacity,
            adapter.clone(),
        ));

        Self {
            capacity,
            client: Mutex::new(client),
            daemon: Mutex::new(daemon),
            clock,
            in_memory_adapter: Mutex::new(Some(adapter)),
            delayed_adapter: None,
            next_ids: Arc::new(Mutex::new(VecDeque::new())),
            driver_supersessions: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            pending: Arc::new(Mutex::new(VecDeque::new())),
        }
    }

    pub fn new_with_delayed(capacity: usize, delayed: DelayedConfirmAdapter) -> Self {
        let clock = ManualClock::new();
        let time_source: Arc<dyn TimeSource> = Arc::new(ManualTimeSource(clock.clone()));
        let client = Arc::new(DeviceClient::new_with_time_source(time_source));
        let daemon = Arc::new(DeviceDaemonService::new_with_capacity(
            capacity,
            Arc::new(delayed.clone()),
        ));

        Self {
            capacity,
            client: Mutex::new(client),
            daemon: Mutex::new(daemon),
            clock,
            in_memory_adapter: Mutex::new(None),
            delayed_adapter: Some(delayed),
            next_ids: Arc::new(Mutex::new(VecDeque::new())),
            driver_supersessions: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            pending: Arc::new(Mutex::new(VecDeque::new())),
        }
    }

    fn current_client(&self) -> Arc<DeviceClient> {
        self.client.lock().unwrap().clone()
    }

    fn current_daemon(&self) -> Arc<DeviceDaemonService> {
        self.daemon.lock().unwrap().clone()
    }
}

fn map_outcome(outcome: DeviceCommandOutcome) -> CommandOutcome {
    match outcome {
        DeviceCommandOutcome::Applied { version, .. } => CommandOutcome::Applied { version },
        DeviceCommandOutcome::Rejected { reason, .. } => {
            let r = match reason {
                DeviceRejectionReason::Overloaded => RejectionReason::Overloaded,
                DeviceRejectionReason::Unavailable => RejectionReason::Unavailable,
            };
            CommandOutcome::Rejected { reason: r }
        }
        DeviceCommandOutcome::Cancelled { reason, .. } => {
            let r = match reason {
                DeviceCancellationReason::Superseded => CancellationReason::Superseded,
                DeviceCancellationReason::OwnerReplaced => CancellationReason::OwnerReplaced,
                DeviceCancellationReason::Shutdown => CancellationReason::Shutdown,
                DeviceCancellationReason::Reconnect => CancellationReason::OwnerReplaced,
            };
            CommandOutcome::Cancelled { reason: r }
        }

        DeviceCommandOutcome::TimedOut {
            last_observed_version,
            ..
        } => CommandOutcome::TimedOut {
            last_observed_version,
        },
        DeviceCommandOutcome::ReconciledApplied { version, .. } => {
            CommandOutcome::ReconciledApplied { version }
        }
    }
}

impl DomainPortDriver for DeviceDomainPortDriver {
    type Payload = AudioPayload;
    type Command = DeviceCommand;

    fn default_payload(&self) -> Self::Payload {
        AudioPayload::default()
    }

    fn sample_payload(&self, seed: u64) -> Self::Payload {
        AudioPayload {
            volume: (seed % 101) as u8,
            is_muted: seed % 2 == 1,
            ..Default::default()
        }
    }

    fn lossless_command(&self, id: &str, _seed: u64) -> Self::Command {
        self.next_ids
            .lock()
            .unwrap()
            .push_back(CommandId(id.to_string()));
        DeviceCommand::Audio(AudioAction::ToggleMute)
    }

    fn replace_latest_command(&self, id: &str, key: &str, seed: u64) -> Self::Command {
        self.next_ids
            .lock()
            .unwrap()
            .push_back(CommandId(id.to_string()));
        if key == "mute" || key.contains("mute") {
            DeviceCommand::Audio(AudioAction::SetMuted(seed % 2 == 1))
        } else {
            DeviceCommand::Audio(AudioAction::SetVolume((seed % 100) as u8))
        }
    }

    fn snapshot(&self) -> DomainSnapshot<Self::Payload> {
        let client = self.current_client();
        let state = client.get_domain_state(DeviceDomain::Audio);
        let payload = match state.payload {
            DomainPayload::Audio(audio) => audio,
            _ => AudioPayload::default(),
        };
        DomainSnapshot {
            version: state.version,
            lifecycle: state.lifecycle,
            payload,
            last_error: state.error,
        }
    }

    fn subscribe(&self) -> SnapshotSubscription<Self::Payload> {
        let client = self.current_client();
        SnapshotSubscription::from_fn(move || {
            let state = client.get_domain_state(DeviceDomain::Audio);
            let payload = match state.payload {
                DomainPayload::Audio(audio) => audio,
                _ => AudioPayload::default(),
            };
            DomainSnapshot {
                version: state.version,
                lifecycle: state.lifecycle,
                payload,
                last_error: state.error,
            }
        })
    }

    fn supervisor_state(&self) -> SupervisorState {
        self.current_client().supervisor_state()
    }

    fn telemetry(&self) -> DomainPortTelemetry {
        let client = self.current_client();
        let daemon = self.current_daemon();
        let client_telem = client.telemetry();
        let daemon_telem = daemon.telemetry();
        DomainPortTelemetry {
            owner_generation: client_telem
                .owner_generation
                .max(daemon_telem.owner_generation),
            current_queue_depth: daemon_telem.current_queue_depth,
            queue_capacity: self.capacity,
            overloads: daemon_telem.overloads + client_telem.overloads,
            supersessions: (daemon_telem.supersessions + client_telem.supersessions)
                .max(self.driver_supersessions.load(Ordering::SeqCst)),

            restarts: client_telem.restarts,
            stale_updates: client_telem.stale_updates + daemon_telem.stale_updates,
            last_error: client_telem.last_error.or(daemon_telem.last_error),
        }
    }

    fn clock(&self) -> &ManualClock {
        &self.clock
    }

    fn advance_clock_ms(&self, ms: u64) {
        self.clock.advance_ms(ms);
        let now_ms = self.clock.now_ms();
        let client = self.current_client();
        let prev_state = client.supervisor_state();
        client.tick(now_ms);
        let new_state = client.supervisor_state();

        if matches!(prev_state, SupervisorState::Backoff { .. })
            && matches!(new_state, SupervisorState::Starting)
        {
            // Reference scenario 15 checks the new generation is already
            // reflected right after this transition, with no subsequent
            // `mark_ready` call. `set_domain_lifecycle` is the owner's own
            // ungated announcement (see `begin_start`/`mark_ready` above),
            // so publishing here doesn't collide with `mark_ready`
            // re-announcing the same generation afterward -- both simply
            // overwrite, and scenario 8's "generation starts at (gen, 0)"
            // still holds since neither call bumps past revision 0.
            let daemon = self.current_daemon();
            let new_gen = daemon.owner_generation();
            client.set_installed_owner_generation(new_gen);
            client.set_domain_lifecycle(
                DeviceDomain::Audio,
                DomainVersion::new(new_gen, 0),
                DomainLifecycle::Reconnecting,
            );
        }
    }

    fn advance_clock_secs(&self, secs: u64) {
        self.advance_clock_ms(secs * 1000);
    }

    fn begin_start(&self) {
        let client = self.current_client();
        let daemon = self.current_daemon();
        let owner_gen = daemon.owner_generation().max(1);
        client.set_installed_owner_generation(owner_gen);
        client.begin_start();

        // Real production `connect()` publishes each domain's own snapshot
        // as soon as it starts listening, ahead of `mark_ready`. Mirror that
        // here for the single domain this driver exercises, rather than
        // having production `begin_start` fabricate a lifecycle for all
        // nine domains it doesn't know about. This is the owner announcing
        // its own transition (`set_domain_lifecycle`, ungated), not an
        // externally-sourced update, so there's no freshness conflict to
        // navigate -- the generation stays wherever it already was; `mark_ready`
        // below is what actually installs the new one.
        let current = client.get_domain_state(DeviceDomain::Audio);
        client.set_domain_lifecycle(
            DeviceDomain::Audio,
            current.version,
            DomainLifecycle::Connecting,
        );
    }

    fn mark_ready(&self) {
        let now_ms = self.clock.now_ms();
        let client = self.current_client();
        let daemon = self.current_daemon();
        let owner_gen = daemon.owner_generation().max(1);
        client.set_installed_owner_generation(owner_gen);
        client.mark_ready(now_ms);

        // Same reasoning as `begin_start` above: production loads real
        // per-domain state before calling `mark_ready`, so the driver
        // announces Audio's Ready snapshot itself instead of `mark_ready`
        // fabricating readiness for domains it never actually heard from.
        // Reference scenario 02 requires the post-ready version to land at
        // exactly `(owner_gen, 0)`, which this always does -- an ungated
        // announcement, so a prior `advance_clock_ms` reconnect publish at
        // the same version is not a conflict to work around.
        client.set_domain_lifecycle(
            DeviceDomain::Audio,
            DomainVersion::new(owner_gen, 0),
            DomainLifecycle::Ready,
        );
    }

    fn report_owner_failure(&self, error: String) {
        let now_ms = self.clock.now_ms();
        // Bump the generation (and cancel anything still queued) *before*
        // releasing a forced delay. Releasing first risks the woken worker
        // task's convergence-check racing the generation bump on a
        // different thread and observing the still-old generation, letting
        // an in-flight command land `Applied` instead of being cancelled.
        self.current_daemon().increment_owner_generation();
        self.current_client().report_owner_failure(error, now_ms);
        let in_mem_opt = self.in_memory_adapter.lock().unwrap().clone();
        if let Some(ref in_mem) = in_mem_opt {
            in_mem.set_forced_delay(None);
        }

        // `increment_owner_generation` cancels queued commands synchronously,
        // but a command already popped and executing (e.g. one an adapter's
        // `forced_delay` was holding open) only resolves once its worker
        // task wakes from the `set_forced_delay(None)` signal above and
        // observes the new generation. Yield (no wall-clock wait) until this
        // driver's own tracked tickets have settled, matching the bounded
        // poll pattern the compositor and notification drivers use for the
        // same class of post-generation-bump async settling.
        let snapshot: Vec<CommandTicket> = self
            .pending
            .lock()
            .unwrap()
            .iter()
            .map(|item| item.ticket.clone())
            .collect();
        for ticket in &snapshot {
            for _ in 0..100_000 {
                if ticket.is_completed() {
                    break;
                }
                std::thread::yield_now();
            }
        }
    }

    fn publish_update(
        &self,
        revision: u64,
        payload: Self::Payload,
    ) -> Result<(), StaleUpdateError> {
        let current_gen = self
            .current_client()
            .get_domain_state(DeviceDomain::Audio)
            .version
            .owner_generation;
        self.publish_raw_update(
            DomainVersion::new(current_gen, revision),
            DomainLifecycle::Ready,
            payload,
            None,
        )
    }

    fn publish_raw_update(
        &self,
        version: DomainVersion,
        lifecycle: DomainLifecycle,
        payload: Self::Payload,
        error: Option<String>,
    ) -> Result<(), StaleUpdateError> {
        let client = self.current_client();
        let installed_gen = client.installed_owner_generation();
        let current = client.get_domain_state(DeviceDomain::Audio);

        if version.owner_generation > installed_gen {
            client.update_local_domain_state(DomainState {
                domain: DeviceDomain::Audio,
                version,
                lifecycle,
                payload: DomainPayload::Audio(payload),
                error,
            });
            return Err(StaleUpdateError::UninstalledGeneration {
                installed: installed_gen,
                attempted: version.owner_generation,
            });
        }

        if version < current.version {
            client.update_local_domain_state(DomainState {
                domain: DeviceDomain::Audio,
                version,
                lifecycle,
                payload: DomainPayload::Audio(payload),
                error,
            });
            return Err(StaleUpdateError::StaleVersion {
                current: current.version,
                attempted: version,
            });
        }

        if version == current.version {
            let is_identical = current.lifecycle == lifecycle
                && match &current.payload {
                    DomainPayload::Audio(p) => p == &payload,
                    _ => false,
                }
                && current.error == error;

            if !is_identical {
                client.update_local_domain_state(DomainState {
                    domain: DeviceDomain::Audio,
                    version,
                    lifecycle,
                    payload: DomainPayload::Audio(payload),
                    error,
                });
                return Err(StaleUpdateError::ConflictingSnapshot { version });
            }
        }

        client.update_local_domain_state(DomainState {
            domain: DeviceDomain::Audio,
            version,
            lifecycle,
            payload: DomainPayload::Audio(payload),
            error,
        });
        Ok(())
    }

    fn submit_command(&self, command: Self::Command) -> Result<CommandTicket, CommandOutcome> {
        let replace_key = command.coalescing_key();
        let daemon = self.current_daemon();
        let (id, mut rx) = daemon
            .submit_command(command, PROTOCOL_VERSION)
            .map_err(|_| CommandOutcome::Rejected {
                reason: RejectionReason::Unavailable,
            })?;

        if let Ok(outcome) = rx.try_recv()
            && matches!(outcome, DeviceCommandOutcome::Rejected { .. })
        {
            return Err(map_outcome(outcome));
        }

        let (driver_ticket, resolver) = CommandTicket::new();
        let resolver_clone = resolver.clone();

        tokio::spawn(async move {
            if let Ok(outcome) = rx.await {
                resolver_clone.resolve(map_outcome(outcome));
            }
        });

        let cmd_id = self
            .next_ids
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or(CommandId(id.0));

        let mut pending = self.pending.lock().unwrap();

        if let Some(ref key) = replace_key {
            for prev in pending.iter_mut() {
                if prev.replace_key.as_ref() == Some(key) && !prev.ticket.is_completed() {
                    prev.resolver.resolve(CommandOutcome::Cancelled {
                        reason: CancellationReason::Superseded,
                    });
                    self.driver_supersessions.fetch_add(1, Ordering::SeqCst);
                }
            }
        }

        pending.push_back(DriverPendingItem {
            id: cmd_id,
            replace_key,
            ticket: driver_ticket.clone(),
            resolver,
        });

        Ok(driver_ticket)
    }

    fn ack_command_without_snapshot(&self) -> Option<CommandId> {
        let pending = self.pending.lock().unwrap();
        pending.front().map(|item| item.id.clone())
    }

    fn process_pending_commands_and_converge(&self) {
        if let Some(ref delayed) = self.delayed_adapter {
            delayed.confirm();
        }
        let in_mem_opt = self.in_memory_adapter.lock().unwrap().clone();
        if let Some(ref in_mem) = in_mem_opt {
            in_mem.set_forced_delay(None);
        }
        let daemon = self.current_daemon();
        let client = self.current_client();

        for _ in 0..1_000_000 {
            let pending = self.pending.lock().unwrap();
            if pending.iter().all(|item| item.ticket.is_completed()) {
                break;
            }
            drop(pending);
            std::thread::yield_now();
        }

        let state = daemon.get_domain_state(DeviceDomain::Audio);
        client.update_local_domain_state(state);
    }

    fn reconcile_front_command(&self) {
        let mut pending = self.pending.lock().unwrap();
        if let Some(item) = pending.pop_front() {
            item.resolver.resolve(CommandOutcome::ReconciledApplied {
                version: self.snapshot().version,
            });
        }
    }

    fn timeout_front_command(&self) {
        let mut pending = self.pending.lock().unwrap();
        if let Some(item) = pending.pop_front() {
            item.resolver.resolve(CommandOutcome::TimedOut {
                last_observed_version: self.snapshot().version,
            });
        }
    }

    fn reset_quarantine(&self) {
        self.current_client().reset_quarantine();
    }

    fn restart_containing_process(&self) {
        let clock = self.clock.clone();
        let time_source: Arc<dyn TimeSource> = Arc::new(ManualTimeSource(clock));
        let new_client = Arc::new(DeviceClient::new_with_time_source(time_source));
        let new_adapter = Arc::new(InMemoryDeviceAdapter::new());
        new_adapter.set_forced_delay(Some(Duration::from_secs(10)));
        let new_daemon = Arc::new(DeviceDaemonService::new_with_capacity(
            self.capacity,
            new_adapter.clone(),
        ));

        *self.client.lock().unwrap() = new_client;
        *self.daemon.lock().unwrap() = new_daemon;
        *self.in_memory_adapter.lock().unwrap() = Some(new_adapter);
        self.next_ids.lock().unwrap().clear();
        self.driver_supersessions.store(0, Ordering::SeqCst);
        self.pending.lock().unwrap().clear();
    }

    fn backoff_delay_ms(&self, attempt: u32) -> u64 {
        shilpo_domain::reconnect_backoff_ms(attempt)
    }

    fn tick(&self) {
        let now_ms = self.clock.now_ms();
        let client = self.current_client();
        let daemon = self.current_daemon();
        client.tick(now_ms);

        if client.supervisor_state() == SupervisorState::Starting {
            let owner_gen = daemon.owner_generation();
            client.set_installed_owner_generation(owner_gen);
            client.mark_ready(now_ms);
        }
    }

    fn owner_replacement_reason(&self) -> CancellationReason {
        CancellationReason::OwnerReplaced
    }
}

// ---------------------------------------------------------------------------
// Standard Reference Contract Scenarios Run Against Device Driver
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn scenario_01_initial_projection_is_deterministic_and_unavailable() {
    let driver = DeviceDomainPortDriver::new(10);
    domain_port_contract::scenario_01_initial_projection_is_deterministic_and_unavailable(&driver);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn scenario_02_initial_start_follows_unavailable_connecting_ready() {
    let driver = DeviceDomainPortDriver::new(10);
    domain_port_contract::scenario_02_initial_start_follows_unavailable_connecting_ready(&driver);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn scenario_03_reconnect_retains_safe_payload_and_records_last_error() {
    let driver = DeviceDomainPortDriver::new(10);
    domain_port_contract::scenario_03_reconnect_retains_safe_payload_and_records_last_error(
        &driver,
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn scenario_04_strictly_newer_revision_in_same_generation_is_accepted() {
    let driver = DeviceDomainPortDriver::new(10);
    domain_port_contract::scenario_04_strictly_newer_revision_in_same_generation_is_accepted(
        &driver,
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn scenario_05_stale_generation_is_rejected() {
    let driver = DeviceDomainPortDriver::new(10);
    domain_port_contract::scenario_05_stale_generation_is_rejected(&driver);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn scenario_06_stale_revision_is_rejected() {
    let driver = DeviceDomainPortDriver::new(10);
    domain_port_contract::scenario_06_stale_revision_is_rejected(&driver);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn scenario_07_conflicting_payload_at_same_version_is_rejected_and_diagnosed() {
    let driver = DeviceDomainPortDriver::new(10);
    domain_port_contract::scenario_07_conflicting_payload_at_same_version_is_rejected_and_diagnosed(
        &driver,
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn scenario_08_new_owner_generation_permits_revision_reset() {
    let driver = DeviceDomainPortDriver::new(10);
    domain_port_contract::scenario_08_new_owner_generation_permits_revision_reset(&driver);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn scenario_09_slow_subscriber_converges_to_latest_atomic_snapshot() {
    let driver = DeviceDomainPortDriver::new(10);
    domain_port_contract::scenario_09_slow_subscriber_converges_to_latest_atomic_snapshot(&driver);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn scenario_10_accepted_command_receives_exactly_one_terminal_outcome() {
    let driver = DeviceDomainPortDriver::new(10);
    domain_port_contract::scenario_10_accepted_command_receives_exactly_one_terminal_outcome(
        &driver,
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn scenario_11_backend_acknowledgement_alone_does_not_complete_convergence_command() {
    let delayed = DelayedConfirmAdapter::new();
    let driver = DeviceDomainPortDriver::new_with_delayed(10, delayed);
    domain_port_contract::scenario_11_backend_acknowledgement_alone_does_not_complete_convergence_command(&driver);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn scenario_12_lossless_mailbox_rejects_overflow_without_dropping_accepted_commands() {
    let driver = DeviceDomainPortDriver::new(2);
    domain_port_contract::scenario_12_lossless_mailbox_rejects_overflow_without_dropping_accepted_commands(&driver);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn scenario_13_replace_latest_supersedes_pending_command_with_same_key_and_emits_terminal_cancellation()
 {
    let driver = DeviceDomainPortDriver::new(10);
    domain_port_contract::scenario_13_replace_latest_supersedes_pending_command_with_same_key_and_emits_terminal_cancellation(&driver);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn scenario_14_different_replace_latest_keys_do_not_replace_each_other() {
    let driver = DeviceDomainPortDriver::new(10);
    domain_port_contract::scenario_14_different_replace_latest_keys_do_not_replace_each_other(
        &driver,
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn scenario_15_owner_replacement_cancels_old_generation_pending_in_flight_commands() {
    let driver = DeviceDomainPortDriver::new(10);
    domain_port_contract::scenario_15_owner_replacement_cancels_old_generation_pending_in_flight_commands(&driver);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn scenario_16_backoff_is_exponential_from_250_ms_and_capped_at_30_seconds() {
    let driver = DeviceDomainPortDriver::new(10);
    domain_port_contract::scenario_16_backoff_is_exponential_from_250_ms_and_capped_at_30_seconds(
        &driver,
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn scenario_17_five_failures_inside_60_seconds_enter_quarantine() {
    let driver = DeviceDomainPortDriver::new(10);
    domain_port_contract::scenario_17_five_failures_inside_60_seconds_enter_quarantine(&driver);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn scenario_18_five_minutes_stable_clears_rolling_failure_window_but_preserves_session_restart_telemetry()
 {
    let driver = DeviceDomainPortDriver::new(10);
    domain_port_contract::scenario_18_five_minutes_stable_clears_rolling_failure_window_but_preserves_session_restart_telemetry(&driver);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn scenario_19_quarantine_requires_explicit_reset_or_containing_process_restart() {
    let driver = DeviceDomainPortDriver::new(10);
    domain_port_contract::scenario_19_quarantine_requires_explicit_reset_or_containing_process_restart(&driver);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn scenario_20_telemetry_reports_generation_queue_depth_capacity_overloads_supersessions_restarts_stale_updates_and_last_error()
 {
    let driver = DeviceDomainPortDriver::new(2);
    domain_port_contract::scenario_20_telemetry_reports_generation_queue_depth_capacity_overloads_supersessions_restarts_stale_updates_and_last_error(&driver);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn scenario_21_reconciled_and_timed_out_commands_have_typed_terminal_outcomes() {
    let driver = DeviceDomainPortDriver::new(10);
    domain_port_contract::scenario_21_reconciled_and_timed_out_commands_have_typed_terminal_outcomes(&driver);
}
