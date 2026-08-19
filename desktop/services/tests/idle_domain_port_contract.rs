mod support;

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use shilpo_domain::{INITIAL_BACKOFF_MS, MAX_BACKOFF_MS};
use shilpo_services::idle::{
    IdleCommand, IdleCommandOutcome, IdleDomainState, IdleRejectionReason, InhibitSource,
    MockIdleActionSink, MockIdleNotifier,
};
use shilpo_services::{
    CancellationReason, DomainLifecycle, DomainPortTelemetry, DomainVersion, StaleUpdateError,
    SupervisorState, TimeSource,
};
use support::domain_port_contract::{
    self, CommandId, CommandOutcome, CommandTicket, DomainPortDriver, DomainSnapshot, ManualClock,
    RejectionReason, SnapshotSubscription,
};

struct DriverPendingItem {
    id: CommandId,
    ticket: shilpo_services::idle::CommandTicket,
    resolver: support::domain_port_contract::CommandResolver,
}

pub struct IdleDomainPortDriver {
    capacity: usize,
    adapter: Mutex<Arc<IdleDomainState>>,
    backend: Arc<MockIdleNotifier>,
    action_sink: Arc<MockIdleActionSink>,
    clock: ManualClock,
    pending: Arc<Mutex<VecDeque<DriverPendingItem>>>,
    next_id: Mutex<Option<CommandId>>,
}

impl IdleDomainPortDriver {
    pub fn new(capacity: usize) -> Self {
        let clock = ManualClock::new();
        let backend = Arc::new(MockIdleNotifier::new());
        let action_sink = Arc::new(MockIdleActionSink::new());

        struct ClockTimeSource(ManualClock);
        impl TimeSource for ClockTimeSource {
            fn now_ms(&self) -> u64 {
                self.0.now_ms()
            }
        }

        let adapter = Arc::new(IdleDomainState::new(
            capacity,
            backend.clone(),
            action_sink.clone(),
            Arc::new(ClockTimeSource(clock.clone())),
        ));

        Self {
            capacity,
            adapter: Mutex::new(adapter),
            backend,
            action_sink,
            clock,
            pending: Arc::new(Mutex::new(VecDeque::new())),
            next_id: Mutex::new(None),
        }
    }

    fn current_adapter(&self) -> Arc<IdleDomainState> {
        self.adapter.lock().unwrap().clone()
    }
}

impl DomainPortDriver for IdleDomainPortDriver {
    type Payload = u64; // live_idle_seconds
    type Command = IdleCommand;

    fn default_payload(&self) -> Self::Payload {
        0
    }

    fn sample_payload(&self, seed: u64) -> Self::Payload {
        seed
    }

    fn lossless_command(&self, id: &str, seed: u64) -> Self::Command {
        *self.next_id.lock().unwrap() = Some(CommandId(id.to_string()));
        IdleCommand::AddInhibit {
            source: InhibitSource::Named(format!("{id}_{seed}")),
        }
    }

    fn replace_latest_command(&self, id: &str, _key: &str, seed: u64) -> Self::Command {
        *self.next_id.lock().unwrap() = Some(CommandId(id.to_string()));
        IdleCommand::ConfigureBehaviors {
            behaviors: std::collections::BTreeMap::new(),
            grace_seconds: seed as f64,
        }
    }

    fn snapshot(&self) -> DomainSnapshot<Self::Payload> {
        let snap = self.current_adapter().snapshot();
        DomainSnapshot {
            version: snap.version,
            lifecycle: snap.lifecycle,
            payload: snap.live_idle_seconds,
            last_error: snap.last_error,
        }
    }

    fn subscribe(&self) -> SnapshotSubscription<Self::Payload> {
        let rx = self.current_adapter().subscribe();
        SnapshotSubscription::from_fn(move || {
            let snap = rx.borrow().clone();
            DomainSnapshot {
                version: snap.version,
                lifecycle: snap.lifecycle,
                payload: snap.live_idle_seconds,
                last_error: snap.last_error,
            }
        })
    }

    fn supervisor_state(&self) -> SupervisorState {
        self.current_adapter().supervisor_state()
    }

    fn telemetry(&self) -> DomainPortTelemetry {
        self.current_adapter().telemetry()
    }

    fn clock(&self) -> &ManualClock {
        &self.clock
    }

    fn advance_clock_ms(&self, ms: u64) {
        self.clock.advance_ms(ms);
        let adapter = self.current_adapter();
        adapter.tick(self.clock.now_ms());

        let pending = self.pending.lock().unwrap();
        for item in pending.iter() {
            if let Some(outcome) = item.ticket.outcome() {
                item.resolver.resolve(map_outcome(outcome));
            }
        }
    }

    fn advance_clock_secs(&self, secs: u64) {
        self.advance_clock_ms(secs * 1000);
    }

    fn begin_start(&self) {
        let adapter = self.current_adapter();
        adapter.begin_start();
        let pending = self.pending.lock().unwrap();
        for item in pending.iter() {
            if let Some(outcome) = item.ticket.outcome() {
                item.resolver.resolve(map_outcome(outcome));
            }
        }
    }

    fn mark_ready(&self) {
        self.current_adapter().mark_ready(self.clock.now_ms());
    }

    fn report_owner_failure(&self, error: String) {
        self.current_adapter()
            .report_owner_failure(error, self.clock.now_ms());
    }

    fn publish_update(
        &self,
        revision: u64,
        payload: Self::Payload,
    ) -> Result<(), StaleUpdateError> {
        self.current_adapter().publish_update(revision, payload)
    }

    fn publish_raw_update(
        &self,
        version: DomainVersion,
        lifecycle: DomainLifecycle,
        payload: Self::Payload,
        error: Option<String>,
    ) -> Result<(), StaleUpdateError> {
        self.current_adapter()
            .publish_raw_update(version, lifecycle, payload, error)
    }

    fn submit_command(&self, command: Self::Command) -> Result<CommandTicket, CommandOutcome> {
        let adapter = self.current_adapter();
        let cmd_id = match &command {
            IdleCommand::AddInhibit {
                source: InhibitSource::Named(name),
            } => {
                let id = name.split('_').next().unwrap_or(name);
                CommandId(id.to_string())
            }
            _ => self
                .next_id
                .lock()
                .unwrap()
                .take()
                .unwrap_or_else(|| CommandId(uuid::Uuid::new_v4().to_string())),
        };

        let ticket = adapter.submit_command(command).map_err(map_outcome)?;
        let (driver_ticket_internal, resolver) = CommandTicket::new();

        let ticket_clone = ticket.clone();
        let driver_ticket = CommandTicket::from_outcome_fn(move || {
            if let Some(outcome) = driver_ticket_internal.outcome() {
                Some(outcome)
            } else {
                ticket_clone.outcome().map(map_outcome)
            }
        });

        let mut pending = self.pending.lock().unwrap();
        pending.push_back(DriverPendingItem {
            id: cmd_id,
            ticket,
            resolver,
        });

        Ok(driver_ticket)
    }

    fn ack_command_without_snapshot(&self) -> Option<CommandId> {
        let pending = self.pending.lock().unwrap();
        pending.front().map(|item| item.id.clone())
    }

    fn process_pending_commands_and_converge(&self) {
        let adapter = self.current_adapter();
        adapter.process_pending_commands();
        let mut pending = self.pending.lock().unwrap();
        while let Some(item) = pending.pop_front() {
            if let Some(outcome) = item.ticket.outcome() {
                item.resolver.resolve(map_outcome(outcome));
            }
        }
    }

    fn reconcile_front_command(&self) {
        let adapter = self.current_adapter();
        let mut pending = self.pending.lock().unwrap();
        if let Some(item) = pending.pop_front() {
            item.resolver.resolve(CommandOutcome::ReconciledApplied {
                version: adapter.snapshot().version,
            });
        }
    }

    fn timeout_front_command(&self) {
        let adapter = self.current_adapter();
        let mut pending = self.pending.lock().unwrap();
        if let Some(item) = pending.pop_front() {
            item.resolver.resolve(CommandOutcome::TimedOut {
                last_observed_version: adapter.snapshot().version,
            });
        }
    }

    fn reset_quarantine(&self) {
        self.current_adapter().reset_quarantine();
    }

    fn restart_containing_process(&self) {
        struct ClockTimeSource(ManualClock);
        impl TimeSource for ClockTimeSource {
            fn now_ms(&self) -> u64 {
                self.0.now_ms()
            }
        }

        let new_adapter = Arc::new(IdleDomainState::new(
            self.capacity,
            self.backend.clone(),
            self.action_sink.clone(),
            Arc::new(ClockTimeSource(self.clock.clone())),
        ));
        *self.adapter.lock().unwrap() = new_adapter;
        self.pending.lock().unwrap().clear();
    }

    fn backoff_delay_ms(&self, attempt: u32) -> u64 {
        let multiplier = 2u64.saturating_pow(attempt.saturating_sub(1));
        INITIAL_BACKOFF_MS
            .saturating_mul(multiplier)
            .min(MAX_BACKOFF_MS)
    }

    fn tick(&self) {
        self.current_adapter().tick(self.clock.now_ms());
    }

    fn owner_replacement_reason(&self) -> CancellationReason {
        CancellationReason::OwnerReplaced
    }
}

fn map_outcome(outcome: IdleCommandOutcome) -> CommandOutcome {
    match outcome {
        IdleCommandOutcome::Applied { version } => CommandOutcome::Applied { version },
        IdleCommandOutcome::ReconciledApplied { version } => {
            CommandOutcome::ReconciledApplied { version }
        }
        IdleCommandOutcome::Rejected { reason } => CommandOutcome::Rejected {
            reason: match reason {
                IdleRejectionReason::Unavailable => RejectionReason::Unavailable,
                IdleRejectionReason::Overloaded => RejectionReason::Overloaded,
                IdleRejectionReason::Unsupported { .. } => RejectionReason::Unavailable,
                IdleRejectionReason::InvalidConfig { .. } => RejectionReason::Unavailable,
                IdleRejectionReason::NotFound => RejectionReason::Unavailable,
            },
        },
        IdleCommandOutcome::TimedOut {
            last_observed_version,
        } => CommandOutcome::TimedOut {
            last_observed_version,
        },
        IdleCommandOutcome::Cancelled { reason } => CommandOutcome::Cancelled { reason },
    }
}

// ---------------------------------------------------------------------------
// Standard Reference Contract Scenarios Run Against Idle Driver
// ---------------------------------------------------------------------------

#[test]
fn scenario_01_initial_projection_is_deterministic_and_unavailable() {
    let driver = IdleDomainPortDriver::new(10);
    domain_port_contract::scenario_01_initial_projection_is_deterministic_and_unavailable(&driver);
}

#[test]
fn scenario_02_initial_start_follows_unavailable_connecting_ready() {
    let driver = IdleDomainPortDriver::new(10);
    domain_port_contract::scenario_02_initial_start_follows_unavailable_connecting_ready(&driver);
}

#[test]
fn scenario_03_reconnect_retains_safe_payload_and_records_last_error() {
    let driver = IdleDomainPortDriver::new(10);
    domain_port_contract::scenario_03_reconnect_retains_safe_payload_and_records_last_error(
        &driver,
    );
}

#[test]
fn scenario_04_strictly_newer_revision_in_same_generation_is_accepted() {
    let driver = IdleDomainPortDriver::new(10);
    domain_port_contract::scenario_04_strictly_newer_revision_in_same_generation_is_accepted(
        &driver,
    );
}

#[test]
fn scenario_05_stale_generation_is_rejected() {
    let driver = IdleDomainPortDriver::new(10);
    domain_port_contract::scenario_05_stale_generation_is_rejected(&driver);
}

#[test]
fn scenario_06_stale_revision_is_rejected() {
    let driver = IdleDomainPortDriver::new(10);
    domain_port_contract::scenario_06_stale_revision_is_rejected(&driver);
}

#[test]
fn scenario_07_conflicting_payload_at_same_version_is_rejected_and_diagnosed() {
    let driver = IdleDomainPortDriver::new(10);
    domain_port_contract::scenario_07_conflicting_payload_at_same_version_is_rejected_and_diagnosed(
        &driver,
    );
}

#[test]
fn scenario_08_new_owner_generation_permits_revision_reset() {
    let driver = IdleDomainPortDriver::new(10);
    domain_port_contract::scenario_08_new_owner_generation_permits_revision_reset(&driver);
}

#[test]
fn scenario_09_slow_subscriber_converges_to_latest_atomic_snapshot() {
    let driver = IdleDomainPortDriver::new(10);
    domain_port_contract::scenario_09_slow_subscriber_converges_to_latest_atomic_snapshot(&driver);
}

#[test]
fn scenario_10_accepted_command_receives_exactly_one_terminal_outcome() {
    let driver = IdleDomainPortDriver::new(10);
    domain_port_contract::scenario_10_accepted_command_receives_exactly_one_terminal_outcome(
        &driver,
    );
}

#[test]
fn scenario_11_backend_acknowledgement_alone_does_not_complete_convergence_command() {
    let driver = IdleDomainPortDriver::new(10);
    domain_port_contract::scenario_11_backend_acknowledgement_alone_does_not_complete_convergence_command(&driver);
}

#[test]
fn scenario_12_lossless_mailbox_rejects_overflow_without_dropping_accepted_commands() {
    let driver = IdleDomainPortDriver::new(2);
    domain_port_contract::scenario_12_lossless_mailbox_rejects_overflow_without_dropping_accepted_commands(&driver);
}

#[test]
fn scenario_13_replace_latest_supersedes_pending_command_with_same_key_and_emits_terminal_cancellation()
 {
    let driver = IdleDomainPortDriver::new(10);
    domain_port_contract::scenario_13_replace_latest_supersedes_pending_command_with_same_key_and_emits_terminal_cancellation(&driver);
}

#[test]
fn scenario_15_owner_replacement_cancels_old_generation_pending_in_flight_commands() {
    let driver = IdleDomainPortDriver::new(10);
    domain_port_contract::scenario_15_owner_replacement_cancels_old_generation_pending_in_flight_commands(&driver);
}

#[test]
fn scenario_16_backoff_is_exponential_from_250_ms_and_capped_at_30_seconds() {
    let driver = IdleDomainPortDriver::new(10);
    domain_port_contract::scenario_16_backoff_is_exponential_from_250_ms_and_capped_at_30_seconds(
        &driver,
    );
}

#[test]
fn scenario_17_five_failures_inside_60_seconds_enter_quarantine() {
    let driver = IdleDomainPortDriver::new(10);
    domain_port_contract::scenario_17_five_failures_inside_60_seconds_enter_quarantine(&driver);
}

#[test]
fn scenario_18_five_minutes_stable_clears_rolling_failure_window_but_preserves_session_restart_telemetry()
 {
    let driver = IdleDomainPortDriver::new(10);
    domain_port_contract::scenario_18_five_minutes_stable_clears_rolling_failure_window_but_preserves_session_restart_telemetry(&driver);
}

#[test]
fn scenario_19_quarantine_requires_explicit_reset_or_containing_process_restart() {
    let driver = IdleDomainPortDriver::new(10);
    domain_port_contract::scenario_19_quarantine_requires_explicit_reset_or_containing_process_restart(&driver);
}

#[test]
fn scenario_20_telemetry_reports_generation_queue_depth_capacity_overloads_supersessions_restarts_stale_updates_and_last_error()
 {
    let driver = IdleDomainPortDriver::new(2);
    domain_port_contract::scenario_20_telemetry_reports_generation_queue_depth_capacity_overloads_supersessions_restarts_stale_updates_and_last_error(&driver);
}

#[test]
fn scenario_21_reconciled_and_timed_out_commands_have_typed_terminal_outcomes() {
    let driver = IdleDomainPortDriver::new(10);
    domain_port_contract::scenario_21_reconciled_and_timed_out_commands_have_typed_terminal_outcomes(
        &driver,
    );
}
