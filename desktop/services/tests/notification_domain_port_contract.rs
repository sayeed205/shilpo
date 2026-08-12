use shilpo_services::{
    CancellationReason, DomainLifecycle, DomainVersion, Notification, NotificationCommand,
    NotificationCommandOutcome, NotificationPort, NotificationRejectionReason, StaleUpdateError,
    SupervisorState, TestNotificationAdapter,
};

#[test]
fn scenario_01_unavailable_connecting_ready() {
    let adapter = TestNotificationAdapter::new(10);
    let snap = adapter.snapshot();
    assert_eq!(snap.lifecycle, DomainLifecycle::Unavailable);
    assert_eq!(snap.version, DomainVersion::ZERO);
    assert_eq!(adapter.supervisor_state(), SupervisorState::Starting);

    adapter.begin_start();
    let snap = adapter.snapshot();
    assert_eq!(snap.lifecycle, DomainLifecycle::Connecting);
    assert_eq!(snap.version.owner_generation, 1);

    adapter.mark_ready();
    let snap = adapter.snapshot();
    assert_eq!(snap.lifecycle, DomainLifecycle::Ready);
    assert_eq!(adapter.supervisor_state(), SupervisorState::Running);
}

#[test]
fn scenario_02_reconnect_retains_history_dnd_and_records_last_error() {
    let adapter = TestNotificationAdapter::new(10);
    adapter.begin_start();
    adapter.mark_ready();

    adapter.set_dnd_enabled(true);
    adapter.push_notification(Notification::new("Saved", "History item"));

    adapter.report_owner_failure("D-Bus connection closed".into());

    let snap = adapter.snapshot();
    assert_eq!(snap.lifecycle, DomainLifecycle::Reconnecting);
    assert!(snap.dnd_enabled);
    assert_eq!(snap.history.len(), 1);
    assert_eq!(snap.last_error.as_deref(), Some("D-Bus connection closed"));
}

#[test]
fn scenario_03_generation_revision_freshness_and_conflict_rejection() {
    let adapter = TestNotificationAdapter::new(10);
    adapter.begin_start();
    adapter.mark_ready();

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
    let adapter = TestNotificationAdapter::new(2);
    adapter.set_auto_converge(false);
    adapter.begin_start();
    adapter.mark_ready();

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
    let adapter = TestNotificationAdapter::new(10);
    adapter.set_auto_converge(false);
    adapter.begin_start();
    adapter.mark_ready();

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
    let adapter = TestNotificationAdapter::new(10);
    adapter.set_auto_converge(false);
    adapter.begin_start();
    adapter.mark_ready();

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
        Some(NotificationCommandOutcome::Applied { .. })
    ));
    assert!(matches!(
        t_action.outcome(),
        Some(NotificationCommandOutcome::Applied { .. })
    ));
}

#[test]
fn scenario_07_owner_replacement_cancellation() {
    let adapter = TestNotificationAdapter::new(10);
    adapter.set_auto_converge(false);
    adapter.begin_start();
    adapter.mark_ready();

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
    let adapter = TestNotificationAdapter::new(10);
    adapter.begin_start();
    adapter.mark_ready();

    // Trip into quarantine after 5 failures in 60s
    for i in 1..=5 {
        if i < 5 {
            adapter.report_owner_failure(format!("failure {i}"));
            adapter.begin_start();
            adapter.mark_ready();
        } else {
            adapter.report_owner_failure(format!("failure {i}"));
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
    adapter.mark_ready();
    adapter.advance_clock_secs(301);
    adapter.report_owner_failure("single failure after stability".into());
    assert!(matches!(
        adapter.supervisor_state(),
        SupervisorState::Backoff { attempt: 1, .. }
    ));
}

#[test]
fn scenario_09_slow_subscriber_latest_snapshot_convergence() {
    let adapter = TestNotificationAdapter::new(10);
    adapter.set_auto_converge(false);
    adapter.begin_start();
    adapter.mark_ready();

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
    let adapter = TestNotificationAdapter::new(3);
    adapter.begin_start();
    adapter.mark_ready();

    let telem = adapter.telemetry();
    assert_eq!(telem.owner_generation, 1);
    assert_eq!(telem.queue_capacity, 3);
    assert_eq!(telem.current_queue_depth, 0);
    assert_eq!(telem.overloads, 0);
    assert_eq!(telem.supersessions, 0);
}
