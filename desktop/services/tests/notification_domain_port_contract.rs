mod support;

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use shilpo_domain::{INITIAL_BACKOFF_MS, MAX_BACKOFF_MS};
use shilpo_services::notifications::{
    NotificationCommand, NotificationDomainState, NotificationPort, NotificationRejectionReason,
};
use shilpo_services::{
    DomainLifecycle, DomainPortTelemetry, DomainVersion, Notification, NotificationService,
    StaleUpdateError, SupervisorState, TimeSource,
};
use support::domain_port_contract::{
    self, CommandId, CommandOutcome, CommandTicket, DomainPortDriver, DomainSnapshot, ManualClock,
    RejectionReason, SnapshotSubscription,
};

// ---------------------------------------------------------------------------
// Notification Domain Port Driver (Implements Generic DomainPortDriver)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NotificationPayload {
    pub notifications: Vec<Notification>,
    pub history: Vec<Notification>,
    pub dnd_enabled: bool,
}

struct DriverPendingItem {
    id: CommandId,
    ticket: shilpo_services::notifications::CommandTicket,
    resolver: support::domain_port_contract::CommandResolver,
}

pub struct NotificationDomainPortDriver {
    capacity: usize,
    adapter: Mutex<Arc<NotificationDomainState>>,
    clock: ManualClock,
    pending: Arc<Mutex<VecDeque<DriverPendingItem>>>,
    next_id: Mutex<Option<CommandId>>,
}

impl NotificationDomainPortDriver {
    pub fn new(capacity: usize) -> Self {
        let adapter = Arc::new(NotificationDomainState::new(capacity));
        adapter.set_auto_converge(false);
        Self {
            capacity,
            adapter: Mutex::new(adapter),
            clock: ManualClock::new(),
            pending: Arc::new(Mutex::new(VecDeque::new())),
            next_id: Mutex::new(None),
        }
    }

    fn current_adapter(&self) -> Arc<NotificationDomainState> {
        self.adapter.lock().unwrap().clone()
    }
}

impl DomainPortDriver for NotificationDomainPortDriver {
    type Payload = NotificationPayload;
    type Command = NotificationCommand;

    fn default_payload(&self) -> Self::Payload {
        NotificationPayload::default()
    }

    fn sample_payload(&self, seed: u64) -> Self::Payload {
        let mut notif = Notification::new(format!("n{seed}"), "body");
        if let Some(ts) = chrono::DateTime::from_timestamp_millis(seed as i64 * 1000) {
            notif.timestamp = ts.into();
        }
        NotificationPayload {
            notifications: vec![notif],
            history: vec![],
            dnd_enabled: seed % 2 == 1,
        }
    }

    fn lossless_command(&self, id: &str, _seed: u64) -> Self::Command {
        NotificationCommand::Push(Notification::new(id, "body"))
    }

    fn replace_latest_command(&self, id: &str, _key: &str, seed: u64) -> Self::Command {
        *self.next_id.lock().unwrap() = Some(CommandId(id.to_string()));
        NotificationCommand::SetDnd(seed != 0)
    }

    fn snapshot(&self) -> DomainSnapshot<Self::Payload> {
        let adapter = self.current_adapter();
        let snap = adapter.snapshot();
        DomainSnapshot {
            version: snap.version,
            lifecycle: snap.lifecycle,
            payload: NotificationPayload {
                notifications: snap.notifications,
                history: snap.history,
                dnd_enabled: snap.dnd_enabled,
            },
            last_error: snap.last_error,
        }
    }

    fn subscribe(&self) -> SnapshotSubscription<Self::Payload> {
        let adapter = self.current_adapter();
        let rx = adapter.subscribe();
        SnapshotSubscription::from_fn(move || {
            let snap = rx.borrow().clone();
            DomainSnapshot {
                version: snap.version,
                lifecycle: snap.lifecycle,
                payload: NotificationPayload {
                    notifications: snap.notifications,
                    history: snap.history,
                    dnd_enabled: snap.dnd_enabled,
                },
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
        // Sync any cancellations caused by clock advancement / owner replacement
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
        self.current_adapter().publish_update(
            revision,
            payload.notifications,
            payload.history,
            payload.dnd_enabled,
        )
    }

    fn publish_raw_update(
        &self,
        version: DomainVersion,
        lifecycle: DomainLifecycle,
        payload: Self::Payload,
        error: Option<String>,
    ) -> Result<(), StaleUpdateError> {
        self.current_adapter().publish_raw_update(
            version,
            lifecycle,
            payload.notifications,
            payload.history,
            payload.dnd_enabled,
            error,
        )
    }

    fn submit_command(&self, command: Self::Command) -> Result<CommandTicket, CommandOutcome> {
        let adapter = self.current_adapter();
        let cmd_id = match &command {
            NotificationCommand::Push(notif) => CommandId(notif.summary.clone()),
            NotificationCommand::InvokeAction { action_key, .. } => CommandId(action_key.clone()),
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
        adapter.process_pending_commands_and_converge();
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
        let new_adapter = Arc::new(NotificationDomainState::new(self.capacity));
        new_adapter.set_auto_converge(false);
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
}

fn map_outcome(outcome: shilpo_services::NotificationCommandOutcome) -> CommandOutcome {
    match outcome {
        shilpo_services::NotificationCommandOutcome::Applied { version } => {
            CommandOutcome::Applied { version }
        }
        shilpo_services::NotificationCommandOutcome::ReconciledApplied { version } => {
            CommandOutcome::ReconciledApplied { version }
        }
        shilpo_services::NotificationCommandOutcome::Rejected { reason } => {
            CommandOutcome::Rejected {
                reason: match reason {
                    NotificationRejectionReason::Unavailable => RejectionReason::Unavailable,
                    NotificationRejectionReason::Overloaded => RejectionReason::Overloaded,
                    NotificationRejectionReason::NotFound => RejectionReason::Unavailable,
                },
            }
        }
        shilpo_services::NotificationCommandOutcome::TimedOut {
            last_observed_version,
        } => CommandOutcome::TimedOut {
            last_observed_version,
        },
        shilpo_services::NotificationCommandOutcome::Cancelled { reason } => {
            CommandOutcome::Cancelled { reason }
        }
    }
}

// ---------------------------------------------------------------------------
// Standard Reference Contract Scenarios Run Against Notification Driver
// ---------------------------------------------------------------------------

#[test]
fn scenario_01_initial_projection_is_deterministic_and_unavailable() {
    let driver = NotificationDomainPortDriver::new(10);
    domain_port_contract::scenario_01_initial_projection_is_deterministic_and_unavailable(&driver);
}

#[test]
fn scenario_02_initial_start_follows_unavailable_connecting_ready() {
    let driver = NotificationDomainPortDriver::new(10);
    domain_port_contract::scenario_02_initial_start_follows_unavailable_connecting_ready(&driver);
}

#[test]
fn scenario_03_reconnect_retains_safe_payload_and_records_last_error() {
    let driver = NotificationDomainPortDriver::new(10);
    domain_port_contract::scenario_03_reconnect_retains_safe_payload_and_records_last_error(
        &driver,
    );
}

#[test]
fn scenario_04_strictly_newer_revision_in_same_generation_is_accepted() {
    let driver = NotificationDomainPortDriver::new(10);
    domain_port_contract::scenario_04_strictly_newer_revision_in_same_generation_is_accepted(
        &driver,
    );
}

#[test]
fn scenario_05_stale_generation_is_rejected() {
    let driver = NotificationDomainPortDriver::new(10);
    domain_port_contract::scenario_05_stale_generation_is_rejected(&driver);
}

#[test]
fn scenario_06_stale_revision_is_rejected() {
    let driver = NotificationDomainPortDriver::new(10);
    domain_port_contract::scenario_06_stale_revision_is_rejected(&driver);
}

#[test]
fn scenario_07_conflicting_payload_at_same_version_is_rejected_and_diagnosed() {
    let driver = NotificationDomainPortDriver::new(10);
    domain_port_contract::scenario_07_conflicting_payload_at_same_version_is_rejected_and_diagnosed(
        &driver,
    );
}

#[test]
fn scenario_08_new_owner_generation_permits_revision_reset() {
    let driver = NotificationDomainPortDriver::new(10);
    domain_port_contract::scenario_08_new_owner_generation_permits_revision_reset(&driver);
}

#[test]
fn scenario_09_slow_subscriber_converges_to_latest_atomic_snapshot() {
    let driver = NotificationDomainPortDriver::new(10);
    domain_port_contract::scenario_09_slow_subscriber_converges_to_latest_atomic_snapshot(&driver);
}

#[test]
fn scenario_10_accepted_command_receives_exactly_one_terminal_outcome() {
    let driver = NotificationDomainPortDriver::new(10);
    domain_port_contract::scenario_10_accepted_command_receives_exactly_one_terminal_outcome(
        &driver,
    );
}

#[test]
fn scenario_11_backend_acknowledgement_alone_does_not_complete_convergence_command() {
    let driver = NotificationDomainPortDriver::new(10);
    domain_port_contract::scenario_11_backend_acknowledgement_alone_does_not_complete_convergence_command(&driver);
}

#[test]
fn scenario_12_lossless_mailbox_rejects_overflow_without_dropping_accepted_commands() {
    let driver = NotificationDomainPortDriver::new(2);
    domain_port_contract::scenario_12_lossless_mailbox_rejects_overflow_without_dropping_accepted_commands(&driver);
}

#[test]
fn scenario_13_replace_latest_supersedes_pending_command_with_same_key_and_emits_terminal_cancellation()
 {
    let driver = NotificationDomainPortDriver::new(10);
    domain_port_contract::scenario_13_replace_latest_supersedes_pending_command_with_same_key_and_emits_terminal_cancellation(&driver);
}

// NOTE ON SCENARIO 14 DISCLOSURE:
// Reference Scenario 14 ("different_replace_latest_keys_do_not_replace_each_other") is NOT
// applicable to the Notification domain port because `NotificationCommand` defines only a single
// ReplaceLatest command variant (`SetDnd`, key: "set_dnd"). There is no second distinct ReplaceLatest
// key in the notification domain specification.

#[test]
fn scenario_15_owner_replacement_cancels_old_generation_pending_in_flight_commands() {
    let driver = NotificationDomainPortDriver::new(10);
    domain_port_contract::scenario_15_owner_replacement_cancels_old_generation_pending_in_flight_commands(&driver);
}

#[test]
fn scenario_16_backoff_is_exponential_from_250_ms_and_capped_at_30_seconds() {
    let driver = NotificationDomainPortDriver::new(10);
    domain_port_contract::scenario_16_backoff_is_exponential_from_250_ms_and_capped_at_30_seconds(
        &driver,
    );
}

#[test]
fn scenario_17_five_failures_inside_60_seconds_enter_quarantine() {
    let driver = NotificationDomainPortDriver::new(10);
    domain_port_contract::scenario_17_five_failures_inside_60_seconds_enter_quarantine(&driver);
}

#[test]
fn scenario_18_five_minutes_stable_clears_rolling_failure_window_but_preserves_session_restart_telemetry()
 {
    let driver = NotificationDomainPortDriver::new(10);
    domain_port_contract::scenario_18_five_minutes_stable_clears_rolling_failure_window_but_preserves_session_restart_telemetry(&driver);
}

#[test]
fn scenario_19_quarantine_requires_explicit_reset_or_containing_process_restart() {
    let driver = NotificationDomainPortDriver::new(10);
    domain_port_contract::scenario_19_quarantine_requires_explicit_reset_or_containing_process_restart(&driver);
}

#[test]
fn scenario_20_telemetry_reports_generation_queue_depth_capacity_overloads_supersessions_restarts_stale_updates_and_last_error()
 {
    let driver = NotificationDomainPortDriver::new(2);
    domain_port_contract::scenario_20_telemetry_reports_generation_queue_depth_capacity_overloads_supersessions_restarts_stale_updates_and_last_error(&driver);
}

#[test]
fn scenario_21_reconciled_and_timed_out_commands_have_typed_terminal_outcomes() {
    let driver = NotificationDomainPortDriver::new(10);
    domain_port_contract::scenario_21_reconciled_and_timed_out_commands_have_typed_terminal_outcomes(
        &driver,
    );
}

// ---------------------------------------------------------------------------
// Domain-Specific Notification Contract Tests (Preserved from original suite)
// ---------------------------------------------------------------------------

#[test]
fn notification_specific_scenario_06_exactly_once_dismiss_action_dnd_outcomes() {
    let adapter = NotificationDomainState::new(10);
    adapter.set_auto_converge(false);
    adapter.begin_start();
    adapter.mark_ready(0);

    let t_dnd = adapter
        .submit_command(NotificationCommand::SetDnd(true))
        .unwrap();
    let t_dismiss = adapter
        .submit_command(NotificationCommand::Dismiss(1))
        .unwrap();
    let t_action = adapter
        .submit_command(NotificationCommand::InvokeAction {
            id: 1,
            action_key: "default".into(),
        })
        .unwrap();

    adapter.process_pending_commands_and_converge();

    assert!(matches!(
        t_dnd.outcome(),
        Some(shilpo_services::NotificationCommandOutcome::Applied { .. })
    ));
    assert!(matches!(
        t_dismiss.outcome(),
        Some(shilpo_services::NotificationCommandOutcome::ReconciledApplied { .. })
    ));
    assert!(matches!(
        t_action.outcome(),
        Some(shilpo_services::NotificationCommandOutcome::ReconciledApplied { .. })
    ));
}

#[test]
fn notification_specific_scenario_11_idempotent_command_is_reconciled() {
    let adapter = NotificationDomainState::new_ready(4);
    let ticket = adapter
        .submit_command(NotificationCommand::Dismiss(404))
        .unwrap();

    assert_eq!(
        ticket.outcome(),
        Some(
            shilpo_services::NotificationCommandOutcome::ReconciledApplied {
                version: adapter.snapshot().version,
            }
        )
    );
}

#[test]
fn notification_specific_scenario_12_rolling_window_eviction_spaced_failures_do_not_quarantine() {
    let adapter = NotificationDomainState::new(10);
    adapter.begin_start();
    adapter.mark_ready(0);

    // Record 6 failures spaced 70 seconds apart (> 60s rolling window)
    for i in 1..=6 {
        let t_ms = (i - 1) * 70_000;
        adapter.report_owner_failure(format!("spaced failure {i}"), t_ms);

        // Window eviction keeps failure count at 1
        assert_eq!(
            adapter.supervisor_state(),
            SupervisorState::Backoff {
                attempt: 1,
                retry_at_ms: t_ms + 250,
            }
        );

        // Reconnect after each failure
        adapter.begin_start();
        adapter.mark_ready(t_ms + 250);
        assert_eq!(adapter.supervisor_state(), SupervisorState::Running);
    }

    // Must never have entered quarantine
    assert_ne!(adapter.supervisor_state(), SupervisorState::Quarantined);
}

#[test]
fn notification_specific_scenario_14_idle_domain_supervisor_backoff_expires_without_command_traffic()
 {
    let adapter = NotificationDomainState::new(10);
    adapter.begin_start();
    adapter.mark_ready(0);

    adapter.report_owner_failure("failure".into(), 1000);
    assert_eq!(
        adapter.supervisor_state(),
        SupervisorState::Backoff {
            attempt: 1,
            retry_at_ms: 1250,
        }
    );

    // Tick before expiry
    adapter.tick(1200);
    assert!(matches!(
        adapter.supervisor_state(),
        SupervisorState::Backoff { .. }
    ));

    // Tick at/after retry_at_ms without any command submitted
    adapter.tick(1250);
    assert_eq!(adapter.supervisor_state(), SupervisorState::Starting);
    assert_eq!(adapter.snapshot().lifecycle, DomainLifecycle::Reconnecting);
    assert_eq!(adapter.snapshot().version.owner_generation, 2);
}

/// Fixed, non-monotonic time source used only to prove which clock a constructor installed.
struct ManualTimeSource {
    time: u64,
}

impl TimeSource for ManualTimeSource {
    fn now_ms(&self) -> u64 {
        self.time
    }
}

#[tokio::test]
async fn notification_specific_scenario_15_architectural_guard_production_time_source_wiring() {
    let (server_stream, client_stream) = std::os::unix::net::UnixStream::pair().unwrap();
    let guid = zbus::Guid::generate();
    let server_builder = zbus::connection::Builder::async_io_unix_stream(server_stream)
        .server(guid)
        .unwrap()
        .p2p();
    let client_builder = zbus::connection::Builder::async_io_unix_stream(client_stream).p2p();
    let (server_conn, _client_conn) =
        tokio::try_join!(server_builder.build(), client_builder.build())
            .expect("build p2p connections");

    let manual: Arc<dyn TimeSource> = Arc::new(ManualTimeSource { time: 42_000 });
    let service =
        NotificationService::new_with_connection_and_time_source(server_conn, manual.clone())
            .await
            .expect("construct notification service over p2p connection");

    assert_eq!(
        service.time_source().now_ms(),
        42_000,
        "production constructor must expose exactly the injected time source, not a \
         separately constructed clock"
    );
}
