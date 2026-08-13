use std::{sync::Arc, time::Duration};

use shilpo_services::{
    DeviceDaemonService, InMemoryDeviceAdapter,
    device_protocol::{
        AudioAction, CancellationReason, CommandId, CommandOutcome, CommandOutcomeRecord,
        DeviceCommand, DeviceDomain, DomainLifecycle, DomainVersion, PROTOCOL_VERSION,
        RejectionReason,
    },
};

#[test]
fn domain_version_orders_generation_before_revision() {
    assert!(DomainVersion::new(2, 0) > DomainVersion::new(1, u64::MAX));
    assert!(DomainVersion::new(1, 2) > DomainVersion::new(1, 1));
}

#[test]
fn command_outcome_record_rejects_unknown_typed_reason_codes() {
    let record = CommandOutcomeRecord {
        kind: 1,
        command_id: CommandId("command".into()),
        arrival_sequence: 1,
        domain: 0,
        owner_generation: 0,
        revision: 0,
        rejection_reason: 99,
        cancellation_reason: 0,
    };
    assert!(CommandOutcome::try_from(record).is_err());
}

#[tokio::test]
async fn owner_replacement_cancels_in_flight_and_resets_generation() {
    let adapter = Arc::new(InMemoryDeviceAdapter::new());
    adapter.set_forced_delay(Some(Duration::from_secs(1)));
    let service = DeviceDaemonService::new(adapter);

    let (_, outcome) = service
        .submit_command(
            DeviceCommand::Audio(AudioAction::ToggleMute),
            PROTOCOL_VERSION,
        )
        .expect("command must be accepted");
    assert_eq!(service.increment_owner_generation(), 2);

    assert!(matches!(
        outcome.await.expect("terminal outcome must arrive"),
        CommandOutcome::Cancelled {
            reason: CancellationReason::OwnerReplaced,
            ..
        }
    ));
    let state = service.get_domain_state(DeviceDomain::Audio);
    assert_eq!(state.version, DomainVersion::new(2, 0));
    assert_eq!(state.lifecycle, DomainLifecycle::Reconnecting);
}

#[tokio::test]
async fn telemetry_reports_positive_queue_capacity() {
    let service = DeviceDaemonService::new(Arc::new(InMemoryDeviceAdapter::new()));
    let telemetry = service.telemetry();
    assert!(telemetry.queue_capacity > 0);
    assert_eq!(telemetry.owner_generation, 1);
}

#[test]
fn typed_rejection_reason_round_trips_without_string_defaults() {
    let outcome = CommandOutcome::Rejected {
        command_id: CommandId("command".into()),
        arrival_sequence: 1,
        domain: DeviceDomain::Audio,
        reason: RejectionReason::Overloaded,
    };
    let decoded = CommandOutcome::try_from(CommandOutcomeRecord::from(outcome.clone()))
        .expect("typed reason must decode");
    assert_eq!(decoded, outcome);
}
