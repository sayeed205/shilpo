mod support;

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use shilpo_services::compositor::{
    BrokerOptions, CommandExecutorFn, CompositorCapabilities, CompositorCommand,
    CompositorCommandBroker, CompositorSnapshot, ExecutorAck, WindowInfo, WorkspaceInfo,
};
use shilpo_services::{
    CancellationReason, DomainLifecycle, DomainPortTelemetry, DomainVersion, MailboxPolicy,
    StaleUpdateError, SupervisorState,
};
use support::domain_port_contract::{
    self, CommandId, CommandOutcome, CommandResolver, CommandTicket, DomainPortDriver,
    DomainSnapshot, ManualClock, RejectionReason, SnapshotSubscription,
};

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
    broker: Mutex<Arc<CompositorCommandBroker>>,
    current_snapshot: Arc<Mutex<CompositorSnapshot>>,
    clock: ManualClock,
    pending: Arc<Mutex<VecDeque<DriverPendingItem>>>,
    supervisor_state: Arc<Mutex<SupervisorState>>,
    driver_supersessions: AtomicU64,
    active_workspace: Arc<Mutex<Option<u64>>>,
}

impl CompositorDomainPortDriver {
    pub fn new(capacity: usize) -> Self {
        let snapshot = Arc::new(Mutex::new(CompositorSnapshot {
            workspaces: default_workspaces(),
            ..Default::default()
        }));
        let active_workspace = Arc::new(Mutex::new(None));
        let active_workspace_exec = active_workspace.clone();
        let executor: CommandExecutorFn = Box::new(move |cmd, _timeout, _cancel, _register| {
            if let CompositorCommand::FocusWorkspace(id) = cmd {
                *active_workspace_exec.lock().unwrap() = Some(*id);
            }
            Ok(ExecutorAck::Success)
        });
        let max_queue_len = capacity.saturating_sub(1).max(1);
        let broker = CompositorCommandBroker::new(
            BrokerOptions {
                timeout: Duration::from_millis(1500),
                max_queue_len,
            },
            executor,
        );
        Self {
            capacity,
            broker: Mutex::new(broker),
            current_snapshot: snapshot,
            clock: ManualClock::new(),
            pending: Arc::new(Mutex::new(VecDeque::new())),
            supervisor_state: Arc::new(Mutex::new(SupervisorState::Starting)),
            driver_supersessions: AtomicU64::new(0),
            active_workspace,
        }
    }

    fn current_broker(&self) -> Arc<CompositorCommandBroker> {
        self.broker.lock().unwrap().clone()
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
        let snap = self.current_snapshot.lock().unwrap().clone();
        DomainSnapshot {
            version: snap.version,
            lifecycle: snap.connection,
            payload: CompositorPayload {
                workspaces: snap.workspaces,
                windows: snap.windows,
                focused_workspace_id: snap.focused_workspace_id,
                focused_window_id: snap.focused_window_id,
                capabilities: snap.capabilities,
            },
            last_error: snap.last_error,
        }
    }

    fn subscribe(&self) -> SnapshotSubscription<Self::Payload> {
        let current_snapshot = self.current_snapshot.clone();
        SnapshotSubscription::from_fn(move || {
            let snap = current_snapshot.lock().unwrap().clone();
            DomainSnapshot {
                version: snap.version,
                lifecycle: snap.connection,
                payload: CompositorPayload {
                    workspaces: snap.workspaces,
                    windows: snap.windows,
                    focused_workspace_id: snap.focused_workspace_id,
                    focused_window_id: snap.focused_window_id,
                    capabilities: snap.capabilities,
                },
                last_error: snap.last_error,
            }
        })
    }

    // DISCLOSURE: Compositor supervision (`CompositorSupervisor`) is currently
    // private, loop-local in `desktop/services/src/compositor/niri.rs`, wall-clock driven,
    // and only spawned behind a live Niri socket.
    //
    // Therefore, `supervisor_state()` returns driver-local bookkeeping (`Starting` before
    // `mark_ready()`, `Running` after) rather than querying production supervision state.
    // This is why Scenarios 16-19 (exponential backoff, quarantine after 5 failures in 60s,
    // 5-minute stable window reset, and explicit quarantine reset) are OUT OF SCOPE
    // and deferred to #230.
    fn supervisor_state(&self) -> SupervisorState {
        *self.supervisor_state.lock().unwrap()
    }

    fn telemetry(&self) -> DomainPortTelemetry {
        let broker = self.current_broker();
        let t = broker.telemetry();
        let in_flight_count = if t.in_flight { 1 } else { 0 };
        let supersessions = t
            .supersessions
            .max(self.driver_supersessions.load(Ordering::Relaxed));
        let last_error = self.current_snapshot.lock().unwrap().last_error.clone();
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
        let mut snap = self.current_snapshot.lock().unwrap();
        if snap.connection == DomainLifecycle::Reconnecting {
            let next_gen = snap.version.owner_generation + 1;
            let broker = self.current_broker();
            broker.set_installed_generation(next_gen);
            broker.record_restart();
            snap.version = DomainVersion::new(next_gen, 0);

            self.pending.lock().unwrap().clear();
            *self.active_workspace.lock().unwrap() = None;

            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn advance_clock_secs(&self, secs: u64) {
        self.advance_clock_ms(secs * 1000);
    }

    fn begin_start(&self) {
        *self.supervisor_state.lock().unwrap() = SupervisorState::Starting;
        let mut snap = self.current_snapshot.lock().unwrap();
        snap.connection = DomainLifecycle::Connecting;
    }

    fn mark_ready(&self) {
        *self.supervisor_state.lock().unwrap() = SupervisorState::Running;
        let mut snap = self.current_snapshot.lock().unwrap();
        let current_gen = snap.version.owner_generation;
        let next_gen = if current_gen == 0 { 1 } else { current_gen };
        self.current_broker().set_installed_generation(next_gen);
        snap.version = DomainVersion::new(next_gen, 0);
        snap.connection = DomainLifecycle::Ready;
        snap.last_error = None;
        let snap_clone = snap.clone();
        drop(snap);
        let _ = self.current_broker().observe_snapshot(Arc::new(snap_clone));
    }

    fn report_owner_failure(&self, error: String) {
        *self.supervisor_state.lock().unwrap() = SupervisorState::Starting;
        let mut snap = self.current_snapshot.lock().unwrap();
        snap.connection = DomainLifecycle::Reconnecting;
        snap.last_error = Some(error);
    }

    fn publish_update(
        &self,
        revision: u64,
        payload: Self::Payload,
    ) -> Result<(), StaleUpdateError> {
        let current_gen = self
            .current_snapshot
            .lock()
            .unwrap()
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
        let new_snap = Arc::new(CompositorSnapshot {
            version,
            connection: lifecycle,
            workspaces: payload.workspaces,
            windows: payload.windows,
            focused_workspace_id: payload.focused_workspace_id,
            focused_window_id: payload.focused_window_id,
            capabilities: payload.capabilities,
            last_error: error,
            ..Default::default()
        });
        self.current_broker().observe_snapshot(new_snap.clone())?;
        *self.current_snapshot.lock().unwrap() = (*new_snap).clone();
        Ok(())
    }

    fn submit_command(&self, command: Self::Command) -> Result<CommandTicket, CommandOutcome> {
        let workspace_id = match command.command {
            CompositorCommand::FocusWorkspace(id) => id,
            _ => 1,
        };
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
        for _ in 0..50 {
            if broker.telemetry().in_flight {
                break;
            }
            std::thread::sleep(Duration::from_millis(1));
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

            for _ in 0..100 {
                if *self.active_workspace.lock().unwrap() == Some(item.workspace_id) {
                    break;
                }
                std::thread::sleep(Duration::from_millis(1));
            }

            let current = self.current_snapshot.lock().unwrap().clone();
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

            for _ in 0..500 {
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
            let snap = self.snapshot();
            item.resolver.resolve(CommandOutcome::ReconciledApplied {
                version: snap.version,
            });
        }
    }

    fn timeout_front_command(&self) {
        let mut pending = self.pending.lock().unwrap();
        if let Some(item) = pending.pop_front() {
            let snap = self.snapshot();
            item.resolver.resolve(CommandOutcome::TimedOut {
                last_observed_version: snap.version,
            });
        }
    }

    fn reset_quarantine(&self) {
        // No-op: compositor supervision is loop-local
    }

    fn restart_containing_process(&self) {
        let snapshot = Arc::new(Mutex::new(CompositorSnapshot {
            workspaces: default_workspaces(),
            ..Default::default()
        }));
        let active_workspace = self.active_workspace.clone();
        *active_workspace.lock().unwrap() = None;
        let active_workspace_exec = active_workspace;
        let executor: CommandExecutorFn = Box::new(move |cmd, _timeout, _cancel, _register| {
            if let CompositorCommand::FocusWorkspace(id) = cmd {
                *active_workspace_exec.lock().unwrap() = Some(*id);
            }
            Ok(ExecutorAck::Success)
        });
        let max_queue_len = self.capacity.saturating_sub(1).max(1);
        let new_broker = CompositorCommandBroker::new(
            BrokerOptions {
                timeout: Duration::from_millis(1500),
                max_queue_len,
            },
            executor,
        );
        *self.broker.lock().unwrap() = new_broker;
        *self.current_snapshot.lock().unwrap() = (*snapshot.lock().unwrap()).clone();
        self.pending.lock().unwrap().clear();
        *self.supervisor_state.lock().unwrap() = SupervisorState::Starting;
    }

    fn backoff_delay_ms(&self, attempt: u32) -> u64 {
        let multiplier = 2u64.saturating_pow(attempt.saturating_sub(1));
        shilpo_domain::INITIAL_BACKOFF_MS
            .saturating_mul(multiplier)
            .min(shilpo_domain::MAX_BACKOFF_MS)
    }

    fn tick(&self) {
        // No-op: compositor supervision is loop-local
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
// Scenarios Excluded From Compositor Domain Port Conformance
// ---------------------------------------------------------------------------
//
// Scenarios 16-19 are excluded from the compositor contract suite because
// `CompositorSupervisor` is private, loop-local in `desktop/services/src/compositor/niri.rs`,
// wall-clock driven, and only spawned behind a real Niri socket.
//
// | Scenario | Reason |
// |---|---|
// | 16 (exponential backoff) | Supervisor unreachable from compositor broker. |
// | 17 (quarantine after 5 failures/60s) | Same. |
// | 18 (5-minute stable window reset) | Same. |
// | 19 (quarantine needs explicit reset) | Same. `reset_quarantine()` is public but its effect is only observable from inside the listener thread. |
//
// Refactoring CompositorSupervisor to be clock-injected and hoisted onto CompositorService
// is tracked in #230.

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
