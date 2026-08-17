use std::sync::Arc;

use shilpo_services::{
    CancellationReason, DomainLifecycle, DomainVersion, Notification, NotificationCommand,
    NotificationCommandOutcome, NotificationDomainState, NotificationPort,
    NotificationRejectionReason, NotificationService, StaleUpdateError, SupervisorState,
    TimeSource,
};

#[test]
fn scenario_01_unavailable_connecting_ready() {
    let adapter = NotificationDomainState::new(10);
    let snap = adapter.snapshot();
    assert_eq!(snap.lifecycle, DomainLifecycle::Unavailable);
    assert_eq!(snap.version, DomainVersion::ZERO);
    assert_eq!(adapter.supervisor_state(), SupervisorState::Starting);

    adapter.begin_start();
    let snap = adapter.snapshot();
    assert_eq!(snap.lifecycle, DomainLifecycle::Connecting);
    assert_eq!(snap.version.owner_generation, 1);

    adapter.mark_ready(0);
    let snap = adapter.snapshot();
    assert_eq!(snap.lifecycle, DomainLifecycle::Ready);
    assert_eq!(adapter.supervisor_state(), SupervisorState::Running);
}

#[test]
fn scenario_02_reconnect_retains_history_dnd_and_records_last_error() {
    let adapter = NotificationDomainState::new(10);
    adapter.begin_start();
    adapter.mark_ready(0);

    adapter.set_dnd_enabled(true);
    adapter.push_notification(Notification::new("Saved", "History item"));

    adapter.report_owner_failure("D-Bus connection closed".into(), 100);

    let snap = adapter.snapshot();
    assert_eq!(snap.lifecycle, DomainLifecycle::Reconnecting);
    assert!(snap.dnd_enabled);
    assert_eq!(snap.history.len(), 1);
    assert_eq!(snap.last_error.as_deref(), Some("D-Bus connection closed"));
}

#[test]
fn scenario_03_generation_revision_freshness_and_conflict_rejection() {
    let adapter = NotificationDomainState::new(10);
    adapter.begin_start();
    adapter.mark_ready(0);

    adapter.publish_update(1, vec![], vec![], false).unwrap();
    let current_version = adapter.snapshot().version;

    // Stale revision attempt
    let stale_err = adapter
        .publish_raw_update(
            DomainVersion::new(1, 0),
            DomainLifecycle::Ready,
            vec![],
            vec![],
            false,
            None,
        )
        .unwrap_err();
    assert!(matches!(stale_err, StaleUpdateError::StaleVersion { .. }));

    // Conflicting update at same version
    let conflict_err = adapter
        .publish_raw_update(
            current_version,
            DomainLifecycle::Ready,
            vec![Notification::new("Conflict", "Body")],
            vec![],
            false,
            None,
        )
        .unwrap_err();
    assert!(matches!(
        conflict_err,
        StaleUpdateError::ConflictingSnapshot { .. }
    ));

    // Uninstalled generation attempt
    let uninst_err = adapter
        .publish_raw_update(
            DomainVersion::new(5, 0),
            DomainLifecycle::Ready,
            vec![],
            vec![],
            false,
            None,
        )
        .unwrap_err();
    assert!(matches!(
        uninst_err,
        StaleUpdateError::UninstalledGeneration { .. }
    ));

    let telem = adapter.telemetry();
    assert_eq!(telem.stale_updates, 3);
}

#[test]
fn scenario_04_bounded_lossless_overflow() {
    let adapter = NotificationDomainState::new(2);
    adapter.set_auto_converge(false);
    adapter.begin_start();
    adapter.mark_ready(0);

    let t1 = adapter
        .submit_command(NotificationCommand::Dismiss(1))
        .unwrap();
    let t2 = adapter
        .submit_command(NotificationCommand::Dismiss(2))
        .unwrap();

    let overflow_res = adapter.submit_command(NotificationCommand::Dismiss(3));
    assert_eq!(
        overflow_res.unwrap_err(),
        NotificationCommandOutcome::Rejected {
            reason: NotificationRejectionReason::Overloaded,
        }
    );

    let telem = adapter.telemetry();
    assert_eq!(telem.overloads, 1);

    // Previously accepted commands remain queued and un-dropped
    assert!(!t1.is_completed());
    assert!(!t2.is_completed());
}

#[test]
fn scenario_05_replace_latest_dnd_supersession() {
    let adapter = NotificationDomainState::new(10);
    adapter.set_auto_converge(false);
    adapter.begin_start();
    adapter.mark_ready(0);

    let t1 = adapter
        .submit_command(NotificationCommand::SetDnd(true))
        .unwrap();
    let t2 = adapter
        .submit_command(NotificationCommand::SetDnd(false))
        .unwrap();

    assert_eq!(
        t1.outcome(),
        Some(NotificationCommandOutcome::Cancelled {
            reason: CancellationReason::Superseded
        })
    );
    assert!(!t2.is_completed());

    let telem = adapter.telemetry();
    assert_eq!(telem.supersessions, 1);

    adapter.process_pending_commands_and_converge();
    assert!(!adapter.snapshot().dnd_enabled);
}

#[test]
fn scenario_06_exactly_once_dismiss_action_dnd_outcomes() {
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
        Some(NotificationCommandOutcome::Applied { .. })
    ));
    assert!(matches!(
        t_dismiss.outcome(),
        Some(NotificationCommandOutcome::ReconciledApplied { .. })
    ));
    assert!(matches!(
        t_action.outcome(),
        Some(NotificationCommandOutcome::ReconciledApplied { .. })
    ));
}

#[test]
fn scenario_07_owner_replacement_cancellation() {
    let adapter = NotificationDomainState::new(10);
    adapter.set_auto_converge(false);
    adapter.begin_start();
    adapter.mark_ready(0);

    let t1 = adapter
        .submit_command(NotificationCommand::Dismiss(1))
        .unwrap();

    // Owner generation replaced due to reconnect/restart
    adapter.begin_start();

    assert_eq!(
        t1.outcome(),
        Some(NotificationCommandOutcome::Cancelled {
            reason: CancellationReason::OwnerReplaced
        })
    );
}

#[test]
fn scenario_08_backoff_quarantine_stable_reset_explicit_reset() {
    let adapter = NotificationDomainState::new(10);
    adapter.begin_start();
    adapter.mark_ready(0);

    // Trip into quarantine after 5 failures in 60s
    for i in 1..=5 {
        let t_ms = i * 1_000;
        if i < 5 {
            adapter.report_owner_failure(format!("failure {i}"), t_ms);
            adapter.begin_start();
            adapter.mark_ready(t_ms);
        } else {
            adapter.report_owner_failure(format!("failure {i}"), t_ms);
        }
    }

    assert_eq!(adapter.supervisor_state(), SupervisorState::Quarantined);
    assert_eq!(adapter.snapshot().lifecycle, DomainLifecycle::Unavailable);

    // Commands rejected during quarantine
    let rejected = adapter.submit_command(NotificationCommand::SetDnd(true));
    assert_eq!(
        rejected.unwrap_err(),
        NotificationCommandOutcome::Rejected {
            reason: NotificationRejectionReason::Unavailable,
        }
    );

    // Explicit reset clears quarantine
    adapter.reset_quarantine();
    assert_eq!(adapter.supervisor_state(), SupervisorState::Starting);

    // Mark ready and test 5-minute stable reset
    adapter.mark_ready(10_000);
    adapter.tick(311_000);
    adapter.report_owner_failure("single failure after stability".into(), 312_000);
    assert!(matches!(
        adapter.supervisor_state(),
        SupervisorState::Backoff { attempt: 1, .. }
    ));
}

#[test]
fn scenario_09_slow_subscriber_latest_snapshot_convergence() {
    let adapter = NotificationDomainState::new(10);
    adapter.set_auto_converge(false);
    adapter.begin_start();
    adapter.mark_ready(0);

    let mut watch_rx = adapter.subscribe();

    adapter.set_dnd_enabled(true);
    adapter.process_pending_commands_and_converge();

    adapter.push_notification(Notification::new("Heading", "Details"));
    adapter.process_pending_commands_and_converge();

    let latest = watch_rx.borrow_and_update().clone();
    assert!(latest.dnd_enabled);
    assert_eq!(latest.history.len(), 1);
}

#[test]
fn scenario_10_telemetry_counters_and_queue_bounds() {
    let adapter = NotificationDomainState::new(3);
    adapter.begin_start();
    adapter.mark_ready(0);

    let telem = adapter.telemetry();
    assert_eq!(telem.owner_generation, 1);
    assert_eq!(telem.queue_capacity, 3);
    assert_eq!(telem.current_queue_depth, 0);
    assert_eq!(telem.overloads, 0);
    assert_eq!(telem.supersessions, 0);
}

#[test]
fn scenario_11_idempotent_command_is_reconciled() {
    let adapter = NotificationDomainState::new_ready(4);
    let ticket = adapter
        .submit_command(NotificationCommand::Dismiss(404))
        .unwrap();

    assert_eq!(
        ticket.outcome(),
        Some(NotificationCommandOutcome::ReconciledApplied {
            version: adapter.snapshot().version,
        })
    );
}

#[test]
fn scenario_12_rolling_window_eviction_spaced_failures_do_not_quarantine() {
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
fn scenario_13_backoff_progression_honors_retry_at_ms_and_caps() {
    let adapter = NotificationDomainState::new(10);
    adapter.begin_start();
    adapter.mark_ready(0);

    // Failure 1 at t = 1000 -> 250ms
    adapter.report_owner_failure("failure 1".into(), 1000);
    assert_eq!(
        adapter.supervisor_state(),
        SupervisorState::Backoff {
            attempt: 1,
            retry_at_ms: 1250,
        }
    );

    // Failure 2 at t = 1500 -> 500ms
    adapter.report_owner_failure("failure 2".into(), 1500);
    assert_eq!(
        adapter.supervisor_state(),
        SupervisorState::Backoff {
            attempt: 2,
            retry_at_ms: 2000,
        }
    );

    // Failure 3 at t = 2200 -> 1000ms
    adapter.report_owner_failure("failure 3".into(), 2200);
    assert_eq!(
        adapter.supervisor_state(),
        SupervisorState::Backoff {
            attempt: 3,
            retry_at_ms: 3200,
        }
    );

    // Failure 4 at t = 3500 -> 2000ms
    adapter.report_owner_failure("failure 4".into(), 3500);
    assert_eq!(
        adapter.supervisor_state(),
        SupervisorState::Backoff {
            attempt: 4,
            retry_at_ms: 5500,
        }
    );
}

#[test]
fn scenario_14_idle_domain_supervisor_backoff_expires_without_command_traffic() {
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
/// Its value never advances, so equality against it cannot be satisfied by coincidence the
/// way an elapsed-time comparison against a real clock can.
struct ManualTimeSource {
    time: u64,
}

impl TimeSource for ManualTimeSource {
    fn now_ms(&self) -> u64 {
        self.time
    }
}

#[tokio::test]
async fn scenario_15_architectural_guard_production_time_source_wiring() {
    // Drives the production D-Bus-connected constructor over a peer-to-peer connection
    // (no real session bus involved: `request_name` on a non-bus zbus connection resolves
    // locally). If the production path ever stopped honoring the injected time source and
    // fell back to a fresh clock, this would fail immediately rather than merely drift.
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
