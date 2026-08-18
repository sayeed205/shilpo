use std::sync::Arc;
use std::time::Duration;

use shilpo_services::device_protocol::{
    AudioAction, AudioPayload, CommandId, CommandOutcome, CommandOutcomeRecord, DeviceCommand,
    DeviceDomain, DomainLifecycle, DomainPayload, DomainState, DomainVersion, PROTOCOL_VERSION,
    RejectionReason,
};
use shilpo_services::{DeviceAdapter, DeviceClient, DeviceDaemonService, InMemoryDeviceAdapter};

// ---------------------------------------------------------------------------
// Device-Specific Contract Tests Retained Outside Reference Suite
// ---------------------------------------------------------------------------

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
fn freshness_rejects_uninstalled_generation_and_equal_version_conflicts() {
    let client = DeviceClient::new();
    assert_eq!(client.installed_owner_generation(), 0);

    // 1. Uninstalled generation is rejected
    let uninstalled_gen2 = DomainState {
        domain: DeviceDomain::Audio,
        version: DomainVersion::new(2, 1),
        lifecycle: DomainLifecycle::Ready,
        payload: DomainPayload::Audio(AudioPayload {
            volume: 50,
            ..Default::default()
        }),
        error: None,
    };
    client.update_local_domain_state(uninstalled_gen2);
    assert_eq!(client.stale_updates(), 1);
    assert_eq!(
        client.get_domain_state(DeviceDomain::Audio).version,
        DomainVersion::ZERO
    );

    // 2. Conflicting payload at equal version is rejected
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
    assert_eq!(client.stale_updates(), 1);

    let mut state_b = state_a.clone();
    state_b.payload = DomainPayload::Audio(AudioPayload {
        volume: 90,
        ..Default::default()
    });
    client.update_local_domain_state(state_b);

    assert_eq!(client.stale_updates(), 2);
    assert_eq!(client.get_domain_state(DeviceDomain::Audio), state_a);

    // 3. Identical update at equal version is idempotent
    client.update_local_domain_state(state_a.clone());
    assert_eq!(client.stale_updates(), 2);
}

#[tokio::test(start_paused = true)]
async fn device_command_outcome_record_codecs_and_timeout_reconciliation() {
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
