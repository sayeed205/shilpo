use std::sync::Arc;
use std::time::Duration;

use shilpo_services::device_protocol::{
    AudioAction, AudioPayload, BrightnessAction, CancellationReason, CommandId, CommandOutcome,
    CommandOutcomeRecord, DeviceCommand, DeviceDomain, DomainLifecycle, DomainPayload, DomainState,
    DomainVersion, PROTOCOL_VERSION, RejectionReason,
};
use shilpo_services::{DeviceAdapter, DeviceClient, DeviceDaemonService, InMemoryDeviceAdapter};

// ---------------------------------------------------------------------------
// Scenarios 01-09: Freshness, Lifecycle, and Projection
// ---------------------------------------------------------------------------

#[test]
fn scenario_01_initial_projection_is_deterministic_and_unavailable() {
    let client = DeviceClient::new();
    assert!(!client.is_connected());

    for domain in DeviceDomain::ALL {
        let snap = client.get_domain_state(domain);
        assert_eq!(snap.domain, domain);
        assert_eq!(snap.lifecycle, DomainLifecycle::Unavailable);
        assert_eq!(snap.version, DomainVersion::ZERO);
        assert!(snap.error.is_none());
    }
}

#[tokio::test]
async fn all_nine_device_domains_project_deterministically() {
    let client = DeviceClient::new();
    let service = DeviceDaemonService::new(Arc::new(InMemoryDeviceAdapter::new()));

    assert_eq!(DeviceDomain::ALL.len(), 9);

    for domain in DeviceDomain::ALL {
        // 1. Initial client state is deterministic Unavailable
        let client_state = client.get_domain_state(domain);
        assert_eq!(client_state.domain, domain);
        assert_eq!(client_state.lifecycle, DomainLifecycle::Unavailable);
        assert_eq!(client_state.version, DomainVersion::ZERO);

        // 2. Initial daemon service state is deterministic Ready with matching payload
        let service_state = service.get_domain_state(domain);
        assert_eq!(service_state.domain, domain);
        assert_eq!(service_state.lifecycle, DomainLifecycle::Ready);
        assert_eq!(service_state.version, DomainVersion::new(1, 1));
        assert!(service_state.error.is_none());

        match domain {
            DeviceDomain::Audio => {
                assert!(matches!(service_state.payload, DomainPayload::Audio(_)))
            }
            DeviceDomain::Bluetooth => {
                assert!(matches!(service_state.payload, DomainPayload::Bluetooth(_)))
            }
            DeviceDomain::Brightness => {
                assert!(matches!(
                    service_state.payload,
                    DomainPayload::Brightness(_)
                ))
            }
            DeviceDomain::Network => {
                assert!(matches!(service_state.payload, DomainPayload::Network(_)))
            }
            DeviceDomain::NightLight => {
                assert!(matches!(
                    service_state.payload,
                    DomainPayload::NightLight(_)
                ))
            }
            DeviceDomain::PowerProfile => {
                assert!(matches!(
                    service_state.payload,
                    DomainPayload::PowerProfile(_)
                ))
            }
            DeviceDomain::Media => {
                assert!(matches!(service_state.payload, DomainPayload::Media(_)))
            }
            DeviceDomain::Battery => {
                assert!(matches!(service_state.payload, DomainPayload::Battery(_)))
            }
            DeviceDomain::Caffeine => {
                assert!(matches!(service_state.payload, DomainPayload::Caffeine(_)))
            }
        }
    }
}

#[test]
fn scenario_02_initial_start_follows_unavailable_connecting_ready() {
    let client = DeviceClient::new();
    assert_eq!(
        client.get_domain_state(DeviceDomain::Audio).lifecycle,
        DomainLifecycle::Unavailable
    );
    assert_eq!(
        client.get_domain_state(DeviceDomain::Audio).version,
        DomainVersion::ZERO
    );

    // Install generation 1
    let connecting_state = DomainState {
        domain: DeviceDomain::Audio,
        version: DomainVersion::new(0, 1),
        lifecycle: DomainLifecycle::Connecting,
        payload: DomainPayload::empty(DeviceDomain::Audio),
        error: None,
    };
    client.update_local_domain_state(connecting_state);
    assert_eq!(
        client.get_domain_state(DeviceDomain::Audio).lifecycle,
        DomainLifecycle::Connecting
    );
    assert_eq!(
        client.get_domain_state(DeviceDomain::Audio).version,
        DomainVersion::new(0, 1)
    );

    // Transition to Ready
    let ready_state = DomainState {
        domain: DeviceDomain::Audio,
        version: DomainVersion::new(0, 2),
        lifecycle: DomainLifecycle::Ready,
        payload: DomainPayload::Audio(AudioPayload {
            volume: 50,
            ..Default::default()
        }),
        error: None,
    };
    client.update_local_domain_state(ready_state);
    let snap = client.get_domain_state(DeviceDomain::Audio);
    assert_eq!(snap.lifecycle, DomainLifecycle::Ready);
    assert_eq!(snap.version, DomainVersion::new(0, 2));
}

#[tokio::test(start_paused = true)]
async fn scenario_03_reconnect_retains_safe_payload_and_records_last_error() {
    let adapter = Arc::new(InMemoryDeviceAdapter::new());
    let service = DeviceDaemonService::new(adapter);

    // Establish known state with custom volume 85
    let (_, outcome) = service
        .submit_command(
            DeviceCommand::Audio(AudioAction::SetVolume(85)),
            PROTOCOL_VERSION,
        )
        .expect("submit must succeed");

    assert!(matches!(
        outcome.await.expect("outcome must arrive"),
        CommandOutcome::Applied { .. }
    ));

    let active_state = service.get_domain_state(DeviceDomain::Audio);
    assert_eq!(active_state.lifecycle, DomainLifecycle::Ready);
    if let DomainPayload::Audio(payload) = &active_state.payload {
        assert_eq!(payload.volume, 85);
    } else {
        panic!("expected Audio payload");
    }

    // Owner fails and reconnects into generation 2
    assert_eq!(service.increment_owner_generation(), 2);

    let reconnecting_state = service.get_domain_state(DeviceDomain::Audio);
    assert_eq!(reconnecting_state.lifecycle, DomainLifecycle::Reconnecting);
    assert_eq!(reconnecting_state.version, DomainVersion::new(2, 0));
    assert_eq!(
        reconnecting_state.error,
        Some("device owner replaced".into())
    );

    // Payload must be preserved during reconnect!
    if let DomainPayload::Audio(payload) = reconnecting_state.payload {
        assert_eq!(payload.volume, 85);
    } else {
        panic!("expected preserved Audio payload");
    }
}

#[tokio::test(start_paused = true)]
async fn scenario_04_strictly_newer_revision_in_same_generation_is_accepted() {
    let adapter = Arc::new(InMemoryDeviceAdapter::new());
    let service = DeviceDaemonService::new(adapter);

    // Command 1: revision -> 2
    let (id1, outcome1) = service
        .submit_command(
            DeviceCommand::Audio(AudioAction::SetVolume(60)),
            PROTOCOL_VERSION,
        )
        .expect("submit must succeed");
    let res1 = outcome1.await.expect("outcome must arrive");
    assert_eq!(
        res1,
        CommandOutcome::Applied {
            command_id: id1,
            arrival_sequence: 1,
            domain: DeviceDomain::Audio,
            version: DomainVersion::new(1, 2),
        }
    );
    assert_eq!(
        service.get_domain_state(DeviceDomain::Audio).version,
        DomainVersion::new(1, 2)
    );

    // Command 2: revision -> 3
    let (id2, outcome2) = service
        .submit_command(
            DeviceCommand::Audio(AudioAction::SetVolume(70)),
            PROTOCOL_VERSION,
        )
        .expect("submit must succeed");
    let res2 = outcome2.await.expect("outcome must arrive");
    assert_eq!(
        res2,
        CommandOutcome::Applied {
            command_id: id2,
            arrival_sequence: 2,
            domain: DeviceDomain::Audio,
            version: DomainVersion::new(1, 3),
        }
    );
    assert_eq!(
        service.get_domain_state(DeviceDomain::Audio).version,
        DomainVersion::new(1, 3)
    );
}

#[test]
fn scenario_05_stale_generation_is_rejected() {
    assert!(DomainVersion::new(2, 0) > DomainVersion::new(1, u64::MAX));

    let client = DeviceClient::new();
    assert_eq!(client.installed_owner_generation(), 0);

    // Install generation 2 state
    let state_gen2 = DomainState {
        domain: DeviceDomain::Audio,
        version: DomainVersion::new(2, 1),
        lifecycle: DomainLifecycle::Ready,
        payload: DomainPayload::Audio(AudioPayload {
            volume: 50,
            ..Default::default()
        }),
        error: None,
    };
    // Uninstalled generation 2 is rejected if installed_gen is 0
    client.update_local_domain_state(state_gen2.clone());
    assert_eq!(client.stale_updates(), 1);

    // Stale generation 1 is also rejected once generation 2 is installed
    let mut stale_gen1 = state_gen2;
    stale_gen1.version = DomainVersion::new(1, 99);
    client.update_local_domain_state(stale_gen1);
    assert_eq!(client.stale_updates(), 2);
}

#[test]
fn scenario_06_stale_revision_is_rejected() {
    assert!(DomainVersion::new(1, 5) > DomainVersion::new(1, 3));

    let client = DeviceClient::new();

    let current = DomainState {
        domain: DeviceDomain::Audio,
        version: DomainVersion::new(0, 5),
        lifecycle: DomainLifecycle::Ready,
        payload: DomainPayload::Audio(AudioPayload {
            volume: 50,
            ..Default::default()
        }),
        error: None,
    };
    client.update_local_domain_state(current.clone());
    assert_eq!(
        client.get_domain_state(DeviceDomain::Audio).version,
        DomainVersion::new(0, 5)
    );

    // Stale revision (0, 3) rejected
    let mut stale = current;
    stale.version = DomainVersion::new(0, 3);
    stale.payload = DomainPayload::Audio(AudioPayload {
        volume: 30,
        ..Default::default()
    });
    client.update_local_domain_state(stale);

    assert_eq!(client.stale_updates(), 1);
    assert_eq!(
        client.get_domain_state(DeviceDomain::Audio).version,
        DomainVersion::new(0, 5)
    );
}

#[test]
fn scenario_07_conflicting_payload_at_same_version_is_rejected_and_diagnosed() {
    let client = DeviceClient::new();

    let state_a = DomainState {
        domain: DeviceDomain::Audio,
        version: DomainVersion::new(0, 5),
        lifecycle: DomainLifecycle::Ready,
        payload: DomainPayload::Audio(AudioPayload {
            volume: 40,
            ..Default::default()
        }),
        error: None,
    };
    client.update_local_domain_state(state_a.clone());
    assert_eq!(client.stale_updates(), 0);

    // Conflicting payload with same version (0, 5) but volume 90
    let mut state_b = state_a.clone();
    state_b.payload = DomainPayload::Audio(AudioPayload {
        volume: 90,
        ..Default::default()
    });
    client.update_local_domain_state(state_b);

    assert_eq!(client.stale_updates(), 1);
    assert_eq!(client.get_domain_state(DeviceDomain::Audio), state_a);

    // Identical update with same version is accepted idempotently without incrementing stale_updates
    client.update_local_domain_state(state_a.clone());
    assert_eq!(client.stale_updates(), 1);
}

#[tokio::test(start_paused = true)]
async fn scenario_08_new_owner_generation_permits_revision_reset() {
    let adapter = Arc::new(InMemoryDeviceAdapter::new());
    let service = DeviceDaemonService::new(adapter.clone());

    // Advance to revision 3 in generation 1
    let (_, o1) = service
        .submit_command(
            DeviceCommand::Audio(AudioAction::SetVolume(60)),
            PROTOCOL_VERSION,
        )
        .unwrap();
    o1.await.unwrap();
    let (_, o2) = service
        .submit_command(
            DeviceCommand::Audio(AudioAction::SetVolume(70)),
            PROTOCOL_VERSION,
        )
        .unwrap();
    o2.await.unwrap();
    assert_eq!(
        service.get_domain_state(DeviceDomain::Audio).version,
        DomainVersion::new(1, 3)
    );

    // Restart owner into generation 2 -> revision resets to 0
    assert_eq!(service.increment_owner_generation(), 2);
    assert_eq!(
        service.get_domain_state(DeviceDomain::Audio).version,
        DomainVersion::new(2, 0)
    );

    // State refresh in generation 2 produces revision 1 (strictly newer than (2, 0))
    // even though revision 1 < 3
    let _ = adapter
        .execute_command(DeviceCommand::Audio(AudioAction::SetVolume(80)))
        .await;
    let changed = service.refresh_domain_states();
    assert!(!changed.is_empty());
    assert_eq!(
        service.get_domain_state(DeviceDomain::Audio).version,
        DomainVersion::new(2, 1)
    );
}

#[test]
fn scenario_09_slow_subscriber_converges_to_latest_atomic_snapshot() {
    let client = DeviceClient::new();

    for r in 1..=5 {
        let state = DomainState {
            domain: DeviceDomain::Audio,
            version: DomainVersion::new(0, r),
            lifecycle: DomainLifecycle::Ready,
            payload: DomainPayload::Audio(AudioPayload {
                volume: (r * 10) as u8,
                ..Default::default()
            }),
            error: None,
        };
        client.update_local_domain_state(state);
    }

    let snap = client.get_domain_state(DeviceDomain::Audio);
    assert_eq!(snap.version, DomainVersion::new(0, 5));
    if let DomainPayload::Audio(payload) = snap.payload {
        assert_eq!(payload.volume, 50);
    } else {
        panic!("expected Audio payload");
    }
}

// ---------------------------------------------------------------------------
// Scenarios 10-15: Commands, Mailbox Policies, and Supervision
// ---------------------------------------------------------------------------

#[tokio::test(start_paused = true)]
async fn scenario_10_accepted_command_receives_exactly_one_terminal_outcome() {
    let adapter = Arc::new(InMemoryDeviceAdapter::new());
    let service = DeviceDaemonService::new(adapter);

    let (id, reply_rx) = service
        .submit_command(
            DeviceCommand::Audio(AudioAction::SetVolume(75)),
            PROTOCOL_VERSION,
        )
        .expect("command must be accepted");

    let outcome = reply_rx.await.expect("outcome must arrive");
    assert_eq!(
        outcome,
        CommandOutcome::Applied {
            command_id: id,
            arrival_sequence: 1,
            domain: DeviceDomain::Audio,
            version: DomainVersion::new(1, 2),
        }
    );
}

#[tokio::test(start_paused = true)]
async fn scenario_11_backend_acknowledgement_alone_does_not_complete_a_convergence_command() {
    let adapter = Arc::new(InMemoryDeviceAdapter::new());
    adapter.set_forced_delay(Some(Duration::from_millis(500)));
    let service = DeviceDaemonService::new(adapter.clone());

    let (id, mut reply_rx) = service
        .submit_command(
            DeviceCommand::Audio(AudioAction::SetVolume(33)),
            PROTOCOL_VERSION,
        )
        .expect("command must be accepted");

    // Advance clock partially (200ms) - command is in-flight, not completed yet
    tokio::time::advance(Duration::from_millis(200)).await;
    assert!(reply_rx.try_recv().is_err());

    // Advance clock past execution delay (another 400ms) - now completed
    tokio::time::advance(Duration::from_millis(400)).await;
    let outcome = reply_rx.await.expect("outcome must arrive");
    assert_eq!(
        outcome,
        CommandOutcome::Applied {
            command_id: id,
            arrival_sequence: 1,
            domain: DeviceDomain::Audio,
            version: DomainVersion::new(1, 2),
        }
    );
}

#[tokio::test(start_paused = true)]
async fn scenario_12_lossless_mailbox_rejects_overflow_without_dropping_accepted_commands() {
    let adapter = Arc::new(InMemoryDeviceAdapter::new());
    adapter.set_forced_delay(Some(Duration::from_secs(10)));
    let service = DeviceDaemonService::new(adapter.clone());

    // ToggleMute is a Lossless command (coalescing_key() == None)
    assert!(
        DeviceCommand::Audio(AudioAction::ToggleMute)
            .coalescing_key()
            .is_none()
    );

    let mut receivers = Vec::new();
    // Fill the bounded capacity of 16 items
    for _ in 0..16 {
        let (_, rx) = service
            .submit_command(
                DeviceCommand::Audio(AudioAction::ToggleMute),
                PROTOCOL_VERSION,
            )
            .expect("must accept up to capacity");
        receivers.push(rx);
    }

    // 17th command overflows the mailbox
    let (_, overflow_rx) = service
        .submit_command(
            DeviceCommand::Audio(AudioAction::ToggleMute),
            PROTOCOL_VERSION,
        )
        .expect("submit returns overload outcome");

    let overflow_outcome = overflow_rx.await.expect("rejection outcome must arrive");
    assert!(matches!(
        overflow_outcome,
        CommandOutcome::Rejected {
            reason: RejectionReason::Overloaded,
            ..
        }
    ));
    assert_eq!(service.telemetry().overloads, 1);

    // Clear delay and advance time: all 16 accepted commands complete successfully
    adapter.set_forced_delay(None);
    tokio::time::advance(Duration::from_millis(100)).await;

    for rx in receivers {
        assert!(matches!(
            rx.await.expect("accepted command must complete"),
            CommandOutcome::Applied { .. }
        ));
    }
}

#[tokio::test(start_paused = true)]
async fn scenario_13_replace_latest_supersedes_pending_command_with_same_key_emits_cancelled_superseded()
 {
    let adapter = Arc::new(InMemoryDeviceAdapter::new());
    adapter.set_forced_delay(Some(Duration::from_millis(500)));
    let service = DeviceDaemonService::new(adapter.clone());

    // SetVolume is ReplaceLatest with key "audio.volume"
    assert_eq!(
        DeviceCommand::Audio(AudioAction::SetVolume(10)).coalescing_key(),
        Some("audio.volume".into())
    );

    // 1. In-flight command
    let (_, rx1) = service
        .submit_command(
            DeviceCommand::Audio(AudioAction::SetVolume(10)),
            PROTOCOL_VERSION,
        )
        .expect("submit 1");
    tokio::task::yield_now().await;

    // 2. Pending command with same key
    let (_, rx2) = service
        .submit_command(
            DeviceCommand::Audio(AudioAction::SetVolume(20)),
            PROTOCOL_VERSION,
        )
        .expect("submit 2");

    // 3. New command with same key supersedes pending command 2
    let (_, rx3) = service
        .submit_command(
            DeviceCommand::Audio(AudioAction::SetVolume(30)),
            PROTOCOL_VERSION,
        )
        .expect("submit 3");

    let outcome2 = rx2.await.expect("superseded outcome must arrive");
    assert!(matches!(
        outcome2,
        CommandOutcome::Cancelled {
            reason: CancellationReason::Superseded,
            ..
        }
    ));
    assert_eq!(service.telemetry().supersessions, 1);

    // Advance time for command 1 to complete
    tokio::time::advance(Duration::from_millis(600)).await;
    assert!(matches!(rx1.await.unwrap(), CommandOutcome::Applied { .. }));

    // Advance time for command 3 to complete
    tokio::time::advance(Duration::from_millis(600)).await;
    assert!(matches!(rx3.await.unwrap(), CommandOutcome::Applied { .. }));
}

#[tokio::test(start_paused = true)]
async fn scenario_14_different_replace_latest_keys_do_not_replace_each_other() {
    let adapter = Arc::new(InMemoryDeviceAdapter::new());
    adapter.set_forced_delay(Some(Duration::from_millis(500)));
    let service = DeviceDaemonService::new(adapter.clone());

    // Key 1: "audio.volume"
    let (_, rx1) = service
        .submit_command(
            DeviceCommand::Audio(AudioAction::SetVolume(10)),
            PROTOCOL_VERSION,
        )
        .expect("submit audio");
    tokio::task::yield_now().await;

    // Key 2: "brightness.level"
    let (_, rx2) = service
        .submit_command(
            DeviceCommand::Brightness(BrightnessAction::SetBrightness(75)),
            PROTOCOL_VERSION,
        )
        .expect("submit brightness");

    assert_eq!(service.telemetry().supersessions, 0);

    // Advance time: both distinct commands complete
    tokio::time::advance(Duration::from_millis(600)).await;
    assert!(matches!(rx1.await.unwrap(), CommandOutcome::Applied { .. }));
    assert!(matches!(rx2.await.unwrap(), CommandOutcome::Applied { .. }));
}

#[tokio::test(start_paused = true)]
async fn scenario_15_owner_replacement_cancels_old_generation_pending_and_in_flight_commands_exactly_once()
 {
    let adapter = Arc::new(InMemoryDeviceAdapter::new());
    adapter.set_forced_delay(Some(Duration::from_secs(10)));
    let service = DeviceDaemonService::new(adapter);

    // Command 1: in-flight
    let (_, in_flight_rx) = service
        .submit_command(
            DeviceCommand::Audio(AudioAction::ToggleMute),
            PROTOCOL_VERSION,
        )
        .expect("command 1 accepted");

    // Command 2: pending in queue
    let (_, pending_rx) = service
        .submit_command(
            DeviceCommand::Audio(AudioAction::SetVolume(80)),
            PROTOCOL_VERSION,
        )
        .expect("command 2 accepted");

    // Owner replacement
    assert_eq!(service.increment_owner_generation(), 2);

    for rx in [in_flight_rx, pending_rx] {
        let outcome = rx.await.expect("terminal cancellation must arrive");
        assert!(matches!(
            outcome,
            CommandOutcome::Cancelled {
                reason: CancellationReason::OwnerReplaced,
                ..
            }
        ));
    }

    let state = service.get_domain_state(DeviceDomain::Audio);
    assert_eq!(state.version, DomainVersion::new(2, 0));
    assert_eq!(state.lifecycle, DomainLifecycle::Reconnecting);
}

// ---------------------------------------------------------------------------
// Scenarios 20-21: Telemetry and Typed Outcomes
// ---------------------------------------------------------------------------

#[tokio::test(start_paused = true)]
async fn scenario_20_telemetry_reports_generation_queue_depth_capacity_overloads_supersessions_restarts_stale_updates_last_error()
 {
    let adapter = Arc::new(InMemoryDeviceAdapter::new());
    let service = DeviceDaemonService::new(adapter.clone());

    let initial = service.telemetry();
    assert!(initial.queue_capacity > 0);
    assert_eq!(initial.owner_generation, 1);
    assert_eq!(initial.current_queue_depth, 0);
    assert_eq!(initial.overloads, 0);
    assert_eq!(initial.supersessions, 0);
    assert_eq!(initial.restarts, 0);
    assert_eq!(initial.stale_updates, 0);
    assert!(initial.last_error.is_none());

    // Trigger supersession
    adapter.set_forced_delay(Some(Duration::from_secs(10)));
    let _ = service.submit_command(
        DeviceCommand::Audio(AudioAction::SetVolume(10)),
        PROTOCOL_VERSION,
    );
    let _ = service.submit_command(
        DeviceCommand::Audio(AudioAction::SetVolume(20)),
        PROTOCOL_VERSION,
    );
    assert_eq!(service.telemetry().supersessions, 1);

    // Trigger restart
    service.increment_owner_generation();
    let telem = service.telemetry();
    assert_eq!(telem.owner_generation, 2);
    assert_eq!(telem.restarts, 1);
    assert_eq!(telem.last_error, Some("device owner replaced".into()));
}

#[tokio::test(start_paused = true)]
async fn scenario_21_reconciled_and_timed_out_commands_have_typed_terminal_outcomes() {
    // 1. Typed rejection reason round trips without string defaults
    let outcome = CommandOutcome::Rejected {
        command_id: CommandId("cmd-1".into()),
        arrival_sequence: 1,
        domain: DeviceDomain::Audio,
        reason: RejectionReason::Overloaded,
    };
    let decoded = CommandOutcome::try_from(CommandOutcomeRecord::from(outcome.clone()))
        .expect("typed reason must decode");
    assert_eq!(decoded, outcome);

    // 2. Invalid reason code is rejected
    let record = CommandOutcomeRecord {
        kind: 1,
        command_id: CommandId("cmd-2".into()),
        arrival_sequence: 2,
        domain: 0,
        owner_generation: 0,
        revision: 0,
        rejection_reason: 99,
        cancellation_reason: 0,
    };
    assert!(CommandOutcome::try_from(record).is_err());

    // 3. Command timeout and subsequent state reconciliation
    let adapter = Arc::new(InMemoryDeviceAdapter::new());
    adapter.set_forced_delay(Some(Duration::from_secs(10)));
    let service = DeviceDaemonService::new(adapter.clone());
    let mut outcome_sub = service.subscribe_outcomes();

    let (id, rx) = service
        .submit_command(
            DeviceCommand::Audio(AudioAction::SetVolume(42)),
            PROTOCOL_VERSION,
        )
        .expect("submit");

    // Advance time past confirmation timeout (3 seconds for Audio)
    tokio::time::advance(Duration::from_secs(4)).await;

    let timed_out_outcome = rx.await.expect("timed out outcome");
    assert_eq!(
        timed_out_outcome,
        CommandOutcome::TimedOut {
            command_id: id.clone(),
            arrival_sequence: 1,
            domain: DeviceDomain::Audio,
            last_observed_version: DomainVersion::new(1, 1),
        }
    );

    // Now update backend state and trigger refresh_domain_states
    adapter.set_forced_delay(None);
    let _ = adapter
        .execute_command(DeviceCommand::Audio(AudioAction::SetVolume(42)))
        .await;

    let changed = service.refresh_domain_states();
    assert!(!changed.is_empty());

    // Reconciled outcome is broadcast
    let reconciled = outcome_sub.recv().await.expect("reconciled outcome");
    assert_eq!(
        reconciled,
        CommandOutcome::ReconciledApplied {
            command_id: id,
            arrival_sequence: 1,
            domain: DeviceDomain::Audio,
            version: DomainVersion::new(1, 2),
        }
    );
}
