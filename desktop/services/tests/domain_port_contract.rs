mod support;

use support::domain_port_contract::{self, ReferenceDomainPort};

#[test]
fn initial_projection_is_deterministic_and_unavailable() {
    let driver = ReferenceDomainPort::new(10);
    domain_port_contract::scenario_01_initial_projection_is_deterministic_and_unavailable(&driver);
}

#[test]
fn initial_start_follows_unavailable_connecting_ready() {
    let driver = ReferenceDomainPort::new(10);
    domain_port_contract::scenario_02_initial_start_follows_unavailable_connecting_ready(&driver);
}

#[test]
fn reconnect_retains_safe_payload_and_records_last_error() {
    let driver = ReferenceDomainPort::new(10);
    domain_port_contract::scenario_03_reconnect_retains_safe_payload_and_records_last_error(
        &driver,
    );
}

#[test]
fn strictly_newer_revision_in_same_generation_is_accepted() {
    let driver = ReferenceDomainPort::new(10);
    domain_port_contract::scenario_04_strictly_newer_revision_in_same_generation_is_accepted(
        &driver,
    );
}

#[test]
fn stale_generation_is_rejected() {
    let driver = ReferenceDomainPort::new(10);
    domain_port_contract::scenario_05_stale_generation_is_rejected(&driver);
}

#[test]
fn stale_revision_is_rejected() {
    let driver = ReferenceDomainPort::new(10);
    domain_port_contract::scenario_06_stale_revision_is_rejected(&driver);
}

#[test]
fn conflicting_payload_at_same_version_is_rejected_and_diagnosed() {
    let driver = ReferenceDomainPort::new(10);
    domain_port_contract::scenario_07_conflicting_payload_at_same_version_is_rejected_and_diagnosed(
        &driver,
    );
}

#[test]
fn new_owner_generation_permits_revision_reset() {
    let driver = ReferenceDomainPort::new(10);
    domain_port_contract::scenario_08_new_owner_generation_permits_revision_reset(&driver);
}

#[test]
fn slow_subscriber_converges_to_latest_atomic_snapshot() {
    let driver = ReferenceDomainPort::new(10);
    domain_port_contract::scenario_09_slow_subscriber_converges_to_latest_atomic_snapshot(&driver);
}

#[test]
fn accepted_command_receives_exactly_one_terminal_outcome() {
    let driver = ReferenceDomainPort::new(10);
    domain_port_contract::scenario_10_accepted_command_receives_exactly_one_terminal_outcome(
        &driver,
    );
}

#[test]
fn backend_acknowledgement_alone_does_not_complete_convergence_command() {
    let driver = ReferenceDomainPort::new(10);
    domain_port_contract::scenario_11_backend_acknowledgement_alone_does_not_complete_convergence_command(&driver);
}

#[test]
fn lossless_mailbox_rejects_overflow_without_dropping_accepted_commands() {
    let driver = ReferenceDomainPort::new(2);
    domain_port_contract::scenario_12_lossless_mailbox_rejects_overflow_without_dropping_accepted_commands(&driver);
}

#[test]
fn replace_latest_supersedes_pending_command_with_same_key_and_emits_terminal_cancellation() {
    let driver = ReferenceDomainPort::new(10);
    domain_port_contract::scenario_13_replace_latest_supersedes_pending_command_with_same_key_and_emits_terminal_cancellation(&driver);
}

#[test]
fn different_replace_latest_keys_do_not_replace_each_other() {
    let driver = ReferenceDomainPort::new(10);
    domain_port_contract::scenario_14_different_replace_latest_keys_do_not_replace_each_other(
        &driver,
    );
}

#[test]
fn owner_replacement_cancels_old_generation_pending_in_flight_commands() {
    let driver = ReferenceDomainPort::new(10);
    domain_port_contract::scenario_15_owner_replacement_cancels_old_generation_pending_in_flight_commands(&driver);
}

#[test]
fn backoff_is_exponential_from_250_ms_and_capped_at_30_seconds() {
    let driver = ReferenceDomainPort::new(10);
    domain_port_contract::scenario_16_backoff_is_exponential_from_250_ms_and_capped_at_30_seconds(
        &driver,
    );
}

#[test]
fn five_failures_inside_60_seconds_enter_quarantine() {
    let driver = ReferenceDomainPort::new(10);
    domain_port_contract::scenario_17_five_failures_inside_60_seconds_enter_quarantine(&driver);
}

#[test]
fn five_minutes_stable_clears_rolling_failure_window_but_preserves_session_restart_telemetry() {
    let driver = ReferenceDomainPort::new(10);
    domain_port_contract::scenario_18_five_minutes_stable_clears_rolling_failure_window_but_preserves_session_restart_telemetry(&driver);
}

#[test]
fn quarantine_requires_explicit_reset_or_containing_process_restart() {
    let driver = ReferenceDomainPort::new(10);
    domain_port_contract::scenario_19_quarantine_requires_explicit_reset_or_containing_process_restart(&driver);
}

#[test]
fn telemetry_reports_generation_queue_depth_capacity_overloads_supersessions_restarts_stale_updates_and_last_error()
 {
    let driver = ReferenceDomainPort::new(2);
    domain_port_contract::scenario_20_telemetry_reports_generation_queue_depth_capacity_overloads_supersessions_restarts_stale_updates_and_last_error(&driver);
}

#[test]
fn reconciled_and_timed_out_commands_have_typed_terminal_outcomes() {
    let driver = ReferenceDomainPort::new(10);
    domain_port_contract::scenario_21_reconciled_and_timed_out_commands_have_typed_terminal_outcomes(
        &driver,
    );
}

#[test]
#[should_panic(expected = "domain command mailbox capacity must be positive")]
fn mailbox_capacity_must_be_positive() {
    let _ = ReferenceDomainPort::new(0);
}
