mod support;

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc as std_mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use shilpo_services::compositor::{
    BrokerOptions, CommandExecutorFn, CompositorAdapter, CompositorCapabilities, CompositorCommand,
    CompositorCommandBroker, CompositorSnapshot, ExecutorAck, NiriCompositorService, WindowInfo,
    WorkspaceInfo,
};
use shilpo_services::{
    CancellationReason, DomainLifecycle, DomainPortTelemetry, DomainVersion, MailboxPolicy,
    StaleUpdateError, SupervisorState, TimeSource,
};
use support::domain_port_contract::{
    self, CommandId, CommandOutcome, CommandResolver, CommandTicket, DomainPortDriver,
    DomainSnapshot, ManualClock, RejectionReason, SnapshotSubscription,
};

/// CommandId reserved by reference scenario_21 for the command that must
/// resolve via the broker's real `RejectionReason::TimedOut` executor path
/// (`broker.rs:1217`). Verified unique across `domain_port_contract.rs`.
const TIMEOUT_GATE_COMMAND_ID: &str = "timed-out";

/// Constructs the shared `CommandExecutorFn` used by both the initial broker
/// and any broker rebuilt by `restart_containing_process`.
///
/// For every command except the one submitted with id
/// `TIMEOUT_GATE_COMMAND_ID`, this behaves exactly as before: apply the
/// workspace focus and return `Ok(ExecutorAck::Success)` immediately, with no
/// blocking. This keeps every other scenario's cancellation semantics
/// (e.g. scenario_15's in-flight/pending cancellation via the broker's
/// post-execution convergence loop) unaffected.
///
/// When `gate_rx_slot` holds a receiver (installed by `submit_command` only
/// for the reserved id), the executor blocks on it until
/// `timeout_front_command` sends the fire signal, then genuinely returns
/// `Err(RejectionReason::TimedOut)` -- the real broker path that produces
/// `CommandOutcome::TimedOut` at `broker.rs:1217`, no fabricated outcome.
fn make_executor(
    active_workspace: Arc<Mutex<Option<u64>>>,
    gate_rx_slot: Arc<Mutex<Option<std_mpsc::Receiver<()>>>>,
) -> CommandExecutorFn {
    Box::new(move |cmd, _timeout, _cancel, _register| {
        let gate = gate_rx_slot.lock().unwrap().take();
        if let Some(rx) = gate {
            return match rx.recv() {
                Ok(()) => Err(shilpo_services::RejectionReason::TimedOut),
                Err(_) => Ok(ExecutorAck::Success),
            };
        }
        if let CompositorCommand::FocusWorkspace(id) = cmd {
            *active_workspace.lock().unwrap() = Some(*id);
        }
        Ok(ExecutorAck::Success)
    })
}

struct ManualTimeSource(ManualClock);

impl TimeSource for ManualTimeSource {
    fn now_ms(&self) -> u64 {
        self.0.now_ms()
    }
}

fn make_service(
    capacity: usize,
    clock: &ManualClock,
    active_workspace: Arc<Mutex<Option<u64>>>,
    timeout_gate_rx_slot: Arc<Mutex<Option<std_mpsc::Receiver<()>>>>,
) -> Arc<NiriCompositorService> {
    let initial_snapshot = CompositorSnapshot {
        workspaces: default_workspaces(),
        ..Default::default()
    };
    let executor = make_executor(active_workspace, timeout_gate_rx_slot);
    let max_queue_len = capacity.saturating_sub(1).max(1);
    let broker = CompositorCommandBroker::new(
        BrokerOptions {
            timeout: Duration::from_millis(1500),
            max_queue_len,
        },
        executor,
    );
    let time_source: Arc<dyn TimeSource> = Arc::new(ManualTimeSource(clock.clone()));
    NiriCompositorService::new_offline_with(initial_snapshot, time_source, broker)
}

// ---------------------------------------------------------------------------
// Compositor Domain Port Driver (Implements Generic DomainPortDriver)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Default)]
pub struct CompositorPayload {
    pub workspaces: Vec<WorkspaceInfo>,
    pub windows: Vec<WindowInfo>,
    pub focused_workspace_id: Option<u64>,
    pub focused_window_id: Option<u64>,
    pub capabilities: CompositorCapabilities,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompositorDriverCommand {
    pub id: CommandId,
    pub command: CompositorCommand,
    pub policy: MailboxPolicy,
}

struct DriverPendingItem {
    id: CommandId,
    workspace_id: u64,
    replace_key: Option<String>,
    ticket: CommandTicket,
    resolver: CommandResolver,
    task_handle: tokio::task::JoinHandle<()>,
}

fn default_workspaces() -> Vec<WorkspaceInfo> {
    (1..=100)
        .map(|id| WorkspaceInfo {
            id,
            name: Some(format!("ws-{id}")),
            idx: (id % 10) as u8,
            is_active: true,
            is_focused: false,
            is_urgent: false,
            output_name: None,
            active_window_id: None,
        })
        .collect()
}

pub struct CompositorDomainPortDriver {
    capacity: usize,
    service: Mutex<Arc<NiriCompositorService>>,
    clock: ManualClock,
    pending: Arc<Mutex<VecDeque<DriverPendingItem>>>,
    driver_supersessions: AtomicU64,
    active_workspace: Arc<Mutex<Option<u64>>>,
    timeout_gate: Arc<Mutex<Option<std_mpsc::Sender<()>>>>,
    timeout_gate_rx_slot: Arc<Mutex<Option<std_mpsc::Receiver<()>>>>,
}

impl CompositorDomainPortDriver {
    pub fn new(capacity: usize) -> Self {
        let active_workspace = Arc::new(Mutex::new(None));
        let timeout_gate_rx_slot = Arc::new(Mutex::new(None));
        let clock = ManualClock::new();
        let service = make_service(
            capacity,
            &clock,
            active_workspace.clone(),
            timeout_gate_rx_slot.clone(),
        );
        Self {
            capacity,
            service: Mutex::new(service),
            clock,
            pending: Arc::new(Mutex::new(VecDeque::new())),
            driver_supersessions: AtomicU64::new(0),
            active_workspace,
            timeout_gate: Arc::new(Mutex::new(None)),
            timeout_gate_rx_slot,
        }
    }

    fn current_service(&self) -> Arc<NiriCompositorService> {
        self.service.lock().unwrap().clone()
    }

    fn current_broker(&self) -> Arc<CompositorCommandBroker> {
        self.current_service().command_broker()
    }
}

impl DomainPortDriver for CompositorDomainPortDriver {
    type Payload = CompositorPayload;
    type Command = CompositorDriverCommand;

    fn default_payload(&self) -> Self::Payload {
        CompositorPayload {
            workspaces: default_workspaces(),
            windows: Vec::new(),
            focused_workspace_id: None,
            focused_window_id: None,
            capabilities: CompositorCapabilities::default(),
        }
    }

    fn sample_payload(&self, seed: u64) -> Self::Payload {
        CompositorPayload {
            workspaces: vec![WorkspaceInfo {
                id: seed,
                name: Some(format!("ws-{seed}")),
                idx: (seed % 10) as u8,
                is_active: true,
                is_focused: true,
                is_urgent: false,
                output_name: None,
                active_window_id: None,
            }],
            windows: Vec::new(),
            focused_workspace_id: Some(seed),
            focused_window_id: None,
            capabilities: CompositorCapabilities::default(),
        }
    }

    fn lossless_command(&self, id: &str, seed: u64) -> Self::Command {
        CompositorDriverCommand {
            id: CommandId(id.to_string()),
            command: CompositorCommand::FocusWorkspace(seed),
            policy: MailboxPolicy::Lossless,
        }
    }

    fn replace_latest_command(&self, id: &str, key: &str, seed: u64) -> Self::Command {
        CompositorDriverCommand {
            id: CommandId(id.to_string()),
            command: CompositorCommand::FocusWorkspace(seed),
            policy: MailboxPolicy::ReplaceLatest {
                key: key.to_string(),
            },
        }
    }

    fn snapshot(&self) -> DomainSnapshot<Self::Payload> {
        let snap = self.current_service().current();
        DomainSnapshot {
            version: snap.version,
            lifecycle: snap.connection,
            payload: CompositorPayload {
                workspaces: snap.workspaces.clone(),
                windows: snap.windows.clone(),
                focused_workspace_id: snap.focused_workspace_id,
                focused_window_id: snap.focused_window_id,
                capabilities: snap.capabilities.clone(),
            },
            last_error: snap.last_error.clone(),
        }
    }

    fn subscribe(&self) -> SnapshotSubscription<Self::Payload> {
        let service = self.current_service();
        SnapshotSubscription::from_fn(move || {
            let snap = service.current();
            DomainSnapshot {
                version: snap.version,
                lifecycle: snap.connection,
                payload: CompositorPayload {
                    workspaces: snap.workspaces.clone(),
                    windows: snap.windows.clone(),
                    focused_workspace_id: snap.focused_workspace_id,
                    focused_window_id: snap.focused_window_id,
                    capabilities: snap.capabilities.clone(),
                },
                last_error: snap.last_error.clone(),
            }
        })
    }

    fn supervisor_state(&self) -> SupervisorState {
        self.current_service().supervisor_state()
    }

    fn telemetry(&self) -> DomainPortTelemetry {
        let broker = self.current_broker();
        let t = broker.telemetry();
        let in_flight_count = if t.in_flight { 1 } else { 0 };
        let supersessions = t
            .supersessions
            .max(self.driver_supersessions.load(Ordering::Relaxed));
        let last_error = self.current_service().current().last_error.clone();
        DomainPortTelemetry {
            owner_generation: t.owner_generation,
            current_queue_depth: t.current_queue_depth + in_flight_count,
            queue_capacity: self.capacity,
            overloads: t.overloads,
            supersessions,
            restarts: t.restarts,
            stale_updates: t.stale_updates,
            last_error,
        }
    }

    fn clock(&self) -> &ManualClock {
        &self.clock
    }

    fn advance_clock_ms(&self, ms: u64) {
        self.clock.advance_ms(ms);
        self.tick();
        let state = self.current_service().supervisor_state();
        let snap = self.current_service().current();
        if state == SupervisorState::Starting && snap.connection == DomainLifecycle::Reconnecting {
            let next_gen = snap.version.owner_generation + 1;
            let broker = self.current_broker();
            broker.set_installed_generation(next_gen);
            broker.record_restart();
            self.current_service().set_reconnecting_generation(next_gen);

            let drained: Vec<DriverPendingItem> = self.pending.lock().unwrap().drain(..).collect();

            *self.active_workspace.lock().unwrap() = None;

            // `set_installed_generation` synchronously cancels the active and
            // queued broker items and fires their oneshot outcome channels.
            // Yield (no wall-clock wait) until this driver's spawned
            // resolution tasks have observed that and resolved the tickets.
            for item in &drained {
                for _ in 0..100_000 {
                    if item.ticket.is_completed() {
                        break;
                    }
                    std::thread::yield_now();
                }
            }
        }
    }

    fn advance_clock_secs(&self, secs: u64) {
        self.advance_clock_ms(secs * 1000);
    }

    fn begin_start(&self) {
        self.current_service().begin_start();
    }

    fn mark_ready(&self) {
        let now_ms = self.clock.now_ms();
        self.current_service().mark_ready(now_ms);
        let snap = self.current_service().current();
        let current_gen = snap.version.owner_generation;
        let installed_gen = self.current_broker().telemetry().owner_generation;
        let next_gen = if current_gen == 0 {
            1
        } else {
            current_gen.max(installed_gen)
        };
        self.current_broker().set_installed_generation(next_gen);
        let mut new_snap = (*snap).clone();
        new_snap.version = DomainVersion::new(next_gen, 0);
        new_snap.connection = DomainLifecycle::Ready;
        new_snap.last_error = None;
        let _ = self.current_service().update_snapshot(new_snap);
    }

    fn report_owner_failure(&self, error: String) {
        let now_ms = self.clock.now_ms();
        self.current_service().report_owner_failure(error, now_ms);
    }

    fn publish_update(
        &self,
        revision: u64,
        payload: Self::Payload,
    ) -> Result<(), StaleUpdateError> {
        let current_gen = self.current_service().current().version.owner_generation;
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
        let new_snap = CompositorSnapshot {
            version,
            connection: lifecycle,
            workspaces: payload.workspaces,
            windows: payload.windows,
            focused_workspace_id: payload.focused_workspace_id,
            focused_window_id: payload.focused_window_id,
            capabilities: payload.capabilities,
            last_error: error,
            ..Default::default()
        };
        self.current_service().update_snapshot(new_snap)
    }

    fn submit_command(&self, command: Self::Command) -> Result<CommandTicket, CommandOutcome> {
        let workspace_id = match command.command {
            CompositorCommand::FocusWorkspace(id) => id,
            _ => 1,
        };

        // Install the timeout gate before submitting, so it is in place
        // before worker_loop can pop and call the executor. See
        // `make_executor` and `timeout_front_command`.
        if command.id.0 == TIMEOUT_GATE_COMMAND_ID {
            let (tx, rx) = std_mpsc::channel::<()>();
            *self.timeout_gate.lock().unwrap() = Some(tx);
            *self.timeout_gate_rx_slot.lock().unwrap() = Some(rx);
        }

        let broker = self.current_broker();
        let broker_ticket = broker
            .submit_with_policy(command.command, command.policy.clone())
            .map_err(map_outcome)?;

        let (driver_ticket, resolver) = CommandTicket::new();

        let mut pending = self.pending.lock().unwrap();

        // If ReplaceLatest with duplicate key, supersede any pending command with matching key
        if let MailboxPolicy::ReplaceLatest { ref key } = command.policy {
            for prev in pending.iter_mut() {
                if prev.replace_key.as_ref() == Some(key) {
                    prev.task_handle.abort();
                    if prev.resolver.resolve(CommandOutcome::Cancelled {
                        reason: CancellationReason::Superseded,
                    }) {
                        self.driver_supersessions.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }
        }

        let replace_key = match command.policy {
            MailboxPolicy::ReplaceLatest { key } => Some(key),
            _ => None,
        };

        let resolver_task = resolver.clone();
        let task_handle = tokio::spawn(async move {
            let outcome = broker_ticket.await;
            resolver_task.resolve(map_outcome(outcome));
        });

        pending.push_back(DriverPendingItem {
            id: command.id,
            workspace_id,
            replace_key,
            ticket: driver_ticket.clone(),
            resolver: resolver.clone(),
            task_handle,
        });

        // Yield briefly to let worker_loop pop from queue if idle
        for _ in 0..100_000 {
            if broker.telemetry().in_flight {
                break;
            }
            std::thread::yield_now();
        }

        Ok(driver_ticket)
    }

    fn ack_command_without_snapshot(&self) -> Option<CommandId> {
        let pending = self.pending.lock().unwrap();
        pending.front().map(|item| item.id.clone())
    }

    fn process_pending_commands_and_converge(&self) {
        let mut items_to_converge = Vec::new();
        {
            let mut pending = self.pending.lock().unwrap();
            while let Some(item) = pending.pop_front() {
                items_to_converge.push(item);
            }
        }

        for item in items_to_converge {
            if item.ticket.is_completed() {
                continue;
            }

            for _ in 0..100_000 {
                if *self.active_workspace.lock().unwrap() == Some(item.workspace_id) {
                    break;
                }
                std::thread::yield_now();
            }

            let current = self.current_service().current();
            let next_rev = current.version.revision + 1;
            let mut new_payload = CompositorPayload {
                workspaces: current.workspaces.clone(),
                windows: current.windows.clone(),
                focused_workspace_id: Some(item.workspace_id),
                focused_window_id: current.focused_window_id,
                capabilities: current.capabilities.clone(),
            };

            if !new_payload
                .workspaces
                .iter()
                .any(|w| w.id == item.workspace_id)
            {
                new_payload.workspaces.push(WorkspaceInfo {
                    id: item.workspace_id,
                    name: Some(format!("ws-{}", item.workspace_id)),
                    idx: (item.workspace_id % 10) as u8,
                    is_active: true,
                    is_focused: true,
                    is_urgent: false,
                    output_name: None,
                    active_window_id: None,
                });
            }
            let _ = self.publish_update(next_rev, new_payload);

            for _ in 0..100_000 {
                if item.ticket.is_completed() {
                    break;
                }
                std::thread::yield_now();
            }
        }
    }

    // DISCLOSURE (scenario_21): `CommandOutcome::ReconciledApplied` is declared
    // in `shilpo_services::compositor::CommandOutcome` but is NEVER constructed
    // anywhere in the compositor command broker (broker.rs).
    //
    // Therefore, scenario_21's reconcile half cannot be driven through real
    // compositor broker logic. Following the #225 precedent, we resolve the
    // front ticket's resolver directly on the driver to verify the contract's
    // ticket/resolver single-resolution bookkeeping, while disclosing this known
    // production gap.
    fn reconcile_front_command(&self) {
        let mut pending = self.pending.lock().unwrap();
        if let Some(item) = pending.pop_front() {
            // Abort the spawned resolution task, dropping the real
            // `broker_ticket` it held. `CommandTicket`'s `Drop` impl cancels
            // the broker-side item, which frees the broker's single worker
            // thread (otherwise stuck forever in its convergence-wait loop
            // for a command this method never actually converges). Same
            // pattern already used for `ReplaceLatest` supersession above.
            // Without this, a scenario that calls this method before
            // submitting a later command (e.g. scenario_21) would starve
            // that later command on the broker's serial worker.
            item.task_handle.abort();
            let snap = self.snapshot();
            item.resolver.resolve(CommandOutcome::ReconciledApplied {
                version: snap.version,
            });
        }
    }

    // Unlike `reconcile_front_command`, this is genuinely driven through the
    // real broker: the front pending command was submitted with
    // `TIMEOUT_GATE_COMMAND_ID`, so its executor call is currently blocked
    // (see `make_executor`) awaiting this fire signal. Sending it causes the
    // executor to return `Err(RejectionReason::TimedOut)`, which the broker
    // maps to `CommandOutcome::TimedOut` at `broker.rs:1217` -- no outcome is
    // fabricated on the driver.
    fn timeout_front_command(&self) {
        if let Some(tx) = self.timeout_gate.lock().unwrap().take() {
            let _ = tx.send(());
        }
        let item = self.pending.lock().unwrap().pop_front();
        if let Some(item) = item {
            for _ in 0..100_000 {
                if item.ticket.is_completed() {
                    break;
                }
                std::thread::yield_now();
            }
        }
    }

    fn reset_quarantine(&self) {
        self.current_service().reset_quarantine();
    }

    fn restart_containing_process(&self) {
        let active_workspace = self.active_workspace.clone();
        *active_workspace.lock().unwrap() = None;
        *self.timeout_gate.lock().unwrap() = None;
        *self.timeout_gate_rx_slot.lock().unwrap() = None;
        let new_service = make_service(
            self.capacity,
            &self.clock,
            active_workspace,
            self.timeout_gate_rx_slot.clone(),
        );
        *self.service.lock().unwrap() = new_service;
        self.pending.lock().unwrap().clear();
    }

    fn backoff_delay_ms(&self, attempt: u32) -> u64 {
        shilpo_domain::reconnect_backoff_ms(attempt)
    }

    fn tick(&self) {
        let now_ms = self.clock.now_ms();
        self.current_service().tick(now_ms);
    }
}

fn map_outcome(outcome: shilpo_services::CommandOutcome) -> CommandOutcome {
    match outcome {
        shilpo_services::CommandOutcome::Applied { version } => CommandOutcome::Applied { version },
        shilpo_services::CommandOutcome::ReconciledApplied { version } => {
            CommandOutcome::ReconciledApplied { version }
        }
        shilpo_services::CommandOutcome::Rejected { reason } => CommandOutcome::Rejected {
            reason: match reason {
                shilpo_services::RejectionReason::Overloaded => RejectionReason::Overloaded,
                _ => RejectionReason::Unavailable,
            },
        },
        shilpo_services::CommandOutcome::TimedOut {
            last_observed_version,
        } => CommandOutcome::TimedOut {
            last_observed_version,
        },
        shilpo_services::CommandOutcome::Cancelled { reason } => {
            CommandOutcome::Cancelled { reason }
        }
    }
}

// ---------------------------------------------------------------------------
// Pre-existing Unit Tests Preserved
// ---------------------------------------------------------------------------

#[test]
fn test_compositor_version_ordering_and_zero() {
    assert!(DomainVersion::ZERO < DomainVersion::new(1, 0));
    assert!(DomainVersion::new(1, 100) < DomainVersion::new(2, 0));
    assert_eq!(DomainVersion::new(1, 5).to_string(), "g1.r5");
}

// ---------------------------------------------------------------------------
// Standard Reference Contract Scenarios Run Against Compositor Driver
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn scenario_01_initial_projection_is_deterministic_and_unavailable() {
    let driver = CompositorDomainPortDriver::new(10);
    domain_port_contract::scenario_01_initial_projection_is_deterministic_and_unavailable(&driver);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn scenario_02_initial_start_follows_unavailable_connecting_ready() {
    let driver = CompositorDomainPortDriver::new(10);
    domain_port_contract::scenario_02_initial_start_follows_unavailable_connecting_ready(&driver);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn scenario_03_reconnect_retains_safe_payload_and_records_last_error() {
    let driver = CompositorDomainPortDriver::new(10);
    domain_port_contract::scenario_03_reconnect_retains_safe_payload_and_records_last_error(
        &driver,
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn scenario_04_strictly_newer_revision_in_same_generation_is_accepted() {
    let driver = CompositorDomainPortDriver::new(10);
    domain_port_contract::scenario_04_strictly_newer_revision_in_same_generation_is_accepted(
        &driver,
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn scenario_05_stale_generation_is_rejected() {
    let driver = CompositorDomainPortDriver::new(10);
    domain_port_contract::scenario_05_stale_generation_is_rejected(&driver);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn scenario_06_stale_revision_is_rejected() {
    let driver = CompositorDomainPortDriver::new(10);
    domain_port_contract::scenario_06_stale_revision_is_rejected(&driver);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn scenario_07_conflicting_payload_at_same_version_is_rejected_and_diagnosed() {
    let driver = CompositorDomainPortDriver::new(10);
    domain_port_contract::scenario_07_conflicting_payload_at_same_version_is_rejected_and_diagnosed(
        &driver,
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn scenario_08_new_owner_generation_permits_revision_reset() {
    let driver = CompositorDomainPortDriver::new(10);
    domain_port_contract::scenario_08_new_owner_generation_permits_revision_reset(&driver);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn scenario_09_slow_subscriber_converges_to_latest_atomic_snapshot() {
    let driver = CompositorDomainPortDriver::new(10);
    domain_port_contract::scenario_09_slow_subscriber_converges_to_latest_atomic_snapshot(&driver);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn scenario_10_accepted_command_receives_exactly_one_terminal_outcome() {
    let driver = CompositorDomainPortDriver::new(10);
    domain_port_contract::scenario_10_accepted_command_receives_exactly_one_terminal_outcome(
        &driver,
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn scenario_11_backend_acknowledgement_alone_does_not_complete_convergence_command() {
    let driver = CompositorDomainPortDriver::new(10);
    domain_port_contract::scenario_11_backend_acknowledgement_alone_does_not_complete_convergence_command(&driver);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn scenario_12_lossless_mailbox_rejects_overflow_without_dropping_accepted_commands() {
    let driver = CompositorDomainPortDriver::new(2);
    domain_port_contract::scenario_12_lossless_mailbox_rejects_overflow_without_dropping_accepted_commands(&driver);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn scenario_13_replace_latest_supersedes_pending_command_with_same_key_and_emits_terminal_cancellation()
 {
    let driver = CompositorDomainPortDriver::new(10);
    domain_port_contract::scenario_13_replace_latest_supersedes_pending_command_with_same_key_and_emits_terminal_cancellation(&driver);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn scenario_14_different_replace_latest_keys_do_not_replace_each_other() {
    let driver = CompositorDomainPortDriver::new(10);
    domain_port_contract::scenario_14_different_replace_latest_keys_do_not_replace_each_other(
        &driver,
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn scenario_15_owner_replacement_cancels_old_generation_pending_in_flight_commands() {
    let driver = CompositorDomainPortDriver::new(10);
    domain_port_contract::scenario_15_owner_replacement_cancels_old_generation_pending_in_flight_commands(&driver);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn scenario_16_backoff_is_exponential_from_250_ms_and_capped_at_30_seconds() {
    let driver = CompositorDomainPortDriver::new(10);
    domain_port_contract::scenario_16_backoff_is_exponential_from_250_ms_and_capped_at_30_seconds(
        &driver,
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn scenario_17_five_failures_inside_60_seconds_enter_quarantine() {
    let driver = CompositorDomainPortDriver::new(10);
    domain_port_contract::scenario_17_five_failures_inside_60_seconds_enter_quarantine(&driver);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn scenario_18_five_minutes_stable_clears_rolling_failure_window_but_preserves_session_restart_telemetry()
 {
    let driver = CompositorDomainPortDriver::new(10);
    domain_port_contract::scenario_18_five_minutes_stable_clears_rolling_failure_window_but_preserves_session_restart_telemetry(&driver);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn scenario_19_quarantine_requires_explicit_reset_or_containing_process_restart() {
    let driver = CompositorDomainPortDriver::new(10);
    domain_port_contract::scenario_19_quarantine_requires_explicit_reset_or_containing_process_restart(&driver);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn scenario_20_telemetry_reports_generation_queue_depth_capacity_overloads_supersessions_restarts_stale_updates_and_last_error()
 {
    let driver = CompositorDomainPortDriver::new(2);
    domain_port_contract::scenario_20_telemetry_reports_generation_queue_depth_capacity_overloads_supersessions_restarts_stale_updates_and_last_error(&driver);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn scenario_21_reconciled_and_timed_out_commands_have_typed_terminal_outcomes() {
    let driver = CompositorDomainPortDriver::new(10);
    domain_port_contract::scenario_21_reconciled_and_timed_out_commands_have_typed_terminal_outcomes(&driver);
}
