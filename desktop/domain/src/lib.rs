use serde::{Deserialize, Serialize};

/// Initial delay for exponential backoff on owner failure (250 ms).
pub const INITIAL_BACKOFF_MS: u64 = 250;

/// Maximum backoff cap for owner reconnect attempts (30 seconds).
pub const MAX_BACKOFF_MS: u64 = 30_000;

/// Rolling window duration for tracking owner failure frequency (60 seconds).
pub const FAILURE_WINDOW_MS: u64 = 60_000;

/// Continuous stable running duration required to clear the failure window (5 minutes).
pub const STABLE_RESET_MS: u64 = 300_000;

/// Number of failures within the rolling failure window that trips the supervisor into quarantine.
pub const QUARANTINE_FAILURES: usize = 5;

/// Calculates exponential backoff delay in milliseconds for reconnect attempts.
///
/// Follows `INITIAL_BACKOFF_MS * 2^(attempt.saturating_sub(1).min(7))`, clamped
/// to `MAX_BACKOFF_MS`. Both `attempt: 0` and `attempt: 1` yield 250 ms.
pub fn reconnect_backoff_ms(attempt: u32) -> u64 {
    INITIAL_BACKOFF_MS
        .saturating_mul(2u64.saturating_pow(attempt.saturating_sub(1).min(7)))
        .min(MAX_BACKOFF_MS)
}

/// DomainVersion tuple containing owner_generation and revision with strict lexicographical ordering.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, zvariant::Type,
)]
pub struct DomainVersion {
    pub owner_generation: u64,
    pub revision: u64,
}

impl DomainVersion {
    pub const ZERO: Self = Self {
        owner_generation: 0,
        revision: 0,
    };

    pub fn new(owner_generation: u64, revision: u64) -> Self {
        Self {
            owner_generation,
            revision,
        }
    }
}

impl std::fmt::Display for DomainVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "g{}.r{}", self.owner_generation, self.revision)
    }
}

/// Lifecycle connection state of a domain service or adapter.
///
/// This collapses two prior definitions that disagreed on `Copy`: the
/// compositor's `CompositorConnection` lacked it, the device crate's
/// `DomainLifecycle` had it. `Copy` is kept, matching the fieldless
/// device-side definition; both variants and payloads are unit-only, so
/// this widens no consumer's obligations, and `Clone` remains available
/// wherever `Copy` was previously relied on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DomainLifecycle {
    #[default]
    Unavailable,
    Connecting,
    Ready,
    Reconnecting,
    Degraded,
}

impl DomainLifecycle {
    pub fn is_ready(&self) -> bool {
        matches!(self, Self::Ready)
    }

    pub fn state_name(&self) -> &'static str {
        match self {
            Self::Unavailable => "unavailable",
            Self::Connecting => "connecting",
            Self::Ready => "ready",
            Self::Reconnecting => "reconnecting",
            Self::Degraded => "degraded",
        }
    }
}

/// Supervisor operational state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SupervisorState {
    Starting,
    Running,
    Backoff { attempt: u32, retry_at_ms: u64 },
    Quarantined,
    Stopping,
    Stopped,
}

/// Concrete domain supervisor managing rolling failure window, exponential backoff,
/// quarantine, stable reset, and lifecycle state transitions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DomainSupervisor {
    state: SupervisorState,
    failure_timestamps_ms: Vec<u64>,
    last_running_timestamp_ms: Option<u64>,
}

impl DomainSupervisor {
    pub fn new() -> Self {
        Self {
            state: SupervisorState::Starting,
            failure_timestamps_ms: Vec::new(),
            last_running_timestamp_ms: None,
        }
    }

    pub fn state(&self) -> SupervisorState {
        self.state
    }

    pub fn is_quarantined(&self) -> bool {
        matches!(self.state, SupervisorState::Quarantined)
    }

    pub fn is_running(&self) -> bool {
        matches!(self.state, SupervisorState::Running)
    }

    pub fn last_running_timestamp_ms(&self) -> Option<u64> {
        self.last_running_timestamp_ms
    }

    pub fn failure_count(&self) -> usize {
        self.failure_timestamps_ms.len()
    }

    pub fn mark_running(&mut self, now_ms: u64) {
        self.state = SupervisorState::Running;
        self.last_running_timestamp_ms = Some(now_ms);
    }

    /// Forces a transition to `Starting`, e.g. when a port explicitly begins
    /// a new connection attempt outside the normal `Backoff -> Starting`
    /// tick. Deliberately does not touch `failure_timestamps_ms`: the
    /// rolling failure window must survive a restart-triggered re-entry into
    /// `Starting`, or repeated rapid restarts could never accumulate enough
    /// failures to trip quarantine.
    pub fn mark_starting(&mut self) {
        self.state = SupervisorState::Starting;
    }

    pub fn record_failure(&mut self, now_ms: u64) -> SupervisorState {
        self.last_running_timestamp_ms = None;
        self.failure_timestamps_ms
            .retain(|&ts| now_ms.saturating_sub(ts) <= FAILURE_WINDOW_MS);
        self.failure_timestamps_ms.push(now_ms);
        let failure_count = self.failure_timestamps_ms.len();

        let new_state = if failure_count >= QUARANTINE_FAILURES {
            SupervisorState::Quarantined
        } else {
            let attempt = failure_count as u32;
            let delay = reconnect_backoff_ms(attempt);
            SupervisorState::Backoff {
                attempt,
                retry_at_ms: now_ms.saturating_add(delay),
            }
        };
        self.state = new_state;
        new_state
    }

    pub fn tick(&mut self, now_ms: u64) {
        match self.state {
            SupervisorState::Backoff { retry_at_ms, .. } => {
                if now_ms >= retry_at_ms {
                    self.state = SupervisorState::Starting;
                }
            }
            SupervisorState::Running => {
                if let Some(start_ts) = self.last_running_timestamp_ms
                    && now_ms.saturating_sub(start_ts) >= STABLE_RESET_MS
                {
                    self.failure_timestamps_ms.clear();
                }
            }
            _ => {}
        }
    }

    pub fn reset_quarantine(&mut self) -> bool {
        if matches!(self.state, SupervisorState::Quarantined) {
            self.failure_timestamps_ms.clear();
            self.last_running_timestamp_ms = None;
            self.state = SupervisorState::Starting;
            true
        } else {
            false
        }
    }

    pub fn enter_stopping(&mut self) {
        self.state = SupervisorState::Stopping;
        self.last_running_timestamp_ms = None;
    }

    pub fn enter_stopped(&mut self) {
        self.state = SupervisorState::Stopped;
        self.last_running_timestamp_ms = None;
    }
}

impl Default for DomainSupervisor {
    fn default() -> Self {
        Self::new()
    }
}

/// Error returned when publishing a stale or conflicting snapshot update.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum StaleUpdateError {
    StaleVersion {
        current: DomainVersion,
        attempted: DomainVersion,
    },
    ConflictingSnapshot {
        version: DomainVersion,
    },
    UninstalledGeneration {
        installed: u64,
        attempted: u64,
    },
}

/// Reasons why a domain command was cancelled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CancellationReason {
    Shutdown,
    Reconnect,
    OwnerReplaced,
    Superseded,
    User,
    Timeout,
}

impl std::fmt::Display for CancellationReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::User => write!(f, "cancelled by user or dropped ticket"),
            Self::Reconnect => write!(f, "compositor reconnected or changed state"),
            Self::Shutdown => write!(f, "broker shutdown"),
            Self::OwnerReplaced => write!(f, "owner generation replaced"),
            Self::Superseded => write!(f, "superseded by newer command"),
            Self::Timeout => write!(f, "command deadline elapsed"),
        }
    }
}

/// Bounded command mailbox policy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MailboxPolicy {
    Lossless,
    ReplaceLatest { key: String },
}

/// Why a send into a bounded adapter mailbox did not enqueue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MailboxError {
    /// The service is offline; there is no receiver at all.
    Unavailable,
    /// The mailbox is at capacity and the policy rejected the send.
    Overloaded,
}

impl std::fmt::Display for MailboxError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unavailable => write!(f, "service is offline or mailbox closed"),
            Self::Overloaded => write!(f, "mailbox is at capacity"),
        }
    }
}

impl std::error::Error for MailboxError {}

/// Telemetry metrics for a domain port's mailbox and supervisor state.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DomainPortTelemetry {
    pub owner_generation: u64,
    pub current_queue_depth: usize,
    pub queue_capacity: usize,
    pub overloads: u64,
    pub supersessions: u64,
    pub restarts: u64,
    pub stale_updates: u64,
    pub last_error: Option<String>,
}

/// Monotonic time source for supervision timing.
pub trait TimeSource: Send + Sync {
    fn now_ms(&self) -> u64;
}

/// Monotonic time source implementation based on `std::time::Instant`.
#[derive(Debug, Clone)]
pub struct MonotonicTimeSource {
    start: std::time::Instant,
}

impl MonotonicTimeSource {
    pub fn new() -> Self {
        Self {
            start: std::time::Instant::now(),
        }
    }
}

impl Default for MonotonicTimeSource {
    fn default() -> Self {
        Self::new()
    }
}

impl TimeSource for MonotonicTimeSource {
    fn now_ms(&self) -> u64 {
        self.start.elapsed().as_millis() as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_domain_version_zvariant_signature() {
        assert_eq!(<DomainVersion as zvariant::Type>::SIGNATURE, "(tt)");
    }

    #[test]
    fn test_domain_lifecycle_serde_roundtrip() {
        let cases = [
            (DomainLifecycle::Unavailable, "\"unavailable\""),
            (DomainLifecycle::Connecting, "\"connecting\""),
            (DomainLifecycle::Ready, "\"ready\""),
            (DomainLifecycle::Reconnecting, "\"reconnecting\""),
            (DomainLifecycle::Degraded, "\"degraded\""),
        ];
        for (state, json) in cases {
            assert_eq!(serde_json::to_string(&state).unwrap(), json);
            assert_eq!(
                serde_json::from_str::<DomainLifecycle>(json).unwrap(),
                state
            );
        }
    }

    #[test]
    fn test_supervisor_state_serde_roundtrip() {
        let cases = [
            (SupervisorState::Starting, "\"starting\""),
            (SupervisorState::Running, "\"running\""),
            (
                SupervisorState::Backoff {
                    attempt: 2,
                    retry_at_ms: 1500,
                },
                "{\"backoff\":{\"attempt\":2,\"retry_at_ms\":1500}}",
            ),
            (SupervisorState::Quarantined, "\"quarantined\""),
            (SupervisorState::Stopping, "\"stopping\""),
            (SupervisorState::Stopped, "\"stopped\""),
        ];
        for (state, json) in cases {
            assert_eq!(serde_json::to_string(&state).unwrap(), json);
            assert_eq!(
                serde_json::from_str::<SupervisorState>(json).unwrap(),
                state
            );
        }
    }

    #[test]
    fn test_cancellation_reason_serde_roundtrip() {
        let cases = [
            (CancellationReason::Shutdown, "\"shutdown\""),
            (CancellationReason::Reconnect, "\"reconnect\""),
            (CancellationReason::OwnerReplaced, "\"owner_replaced\""),
            (CancellationReason::Superseded, "\"superseded\""),
            (CancellationReason::User, "\"user\""),
            (CancellationReason::Timeout, "\"timeout\""),
        ];
        for (reason, json) in cases {
            assert_eq!(serde_json::to_string(&reason).unwrap(), json);
            assert_eq!(
                serde_json::from_str::<CancellationReason>(json).unwrap(),
                reason
            );
        }
    }

    #[test]
    fn test_stale_update_error_serde_roundtrip() {
        let err1 = StaleUpdateError::StaleVersion {
            current: DomainVersion::new(1, 2),
            attempted: DomainVersion::new(1, 1),
        };
        let err2 = StaleUpdateError::ConflictingSnapshot {
            version: DomainVersion::new(1, 2),
        };
        let err3 = StaleUpdateError::UninstalledGeneration {
            installed: 1,
            attempted: 2,
        };

        for err in [err1, err2, err3] {
            let json = serde_json::to_string(&err).unwrap();
            let parsed: StaleUpdateError = serde_json::from_str(&json).unwrap();
            assert_eq!(err, parsed);
        }
    }

    #[test]
    fn test_domain_version_display_and_ordering() {
        let v1 = DomainVersion::new(1, 0);
        let v2 = DomainVersion::new(1, 5);
        let v3 = DomainVersion::new(2, 0);

        assert_eq!(format!("{v1}"), "g1.r0");
        assert_eq!(format!("{v2}"), "g1.r5");
        assert!(v1 < v2);
        assert!(v2 < v3);
    }

    #[test]
    fn test_mailbox_policy_serde_roundtrip() {
        let p1 = MailboxPolicy::Lossless;
        let p2 = MailboxPolicy::ReplaceLatest {
            key: "test_key".into(),
        };

        for p in [p1, p2] {
            let json = serde_json::to_string(&p).unwrap();
            let parsed: MailboxPolicy = serde_json::from_str(&json).unwrap();
            assert_eq!(p, parsed);
        }
    }

    #[test]
    fn test_domain_port_telemetry_serde_roundtrip() {
        let telem = DomainPortTelemetry {
            owner_generation: 1,
            current_queue_depth: 3,
            queue_capacity: 10,
            overloads: 2,
            supersessions: 1,
            restarts: 4,
            stale_updates: 0,
            last_error: Some("test error".into()),
        };

        let json = serde_json::to_string(&telem).unwrap();
        let parsed: DomainPortTelemetry = serde_json::from_str(&json).unwrap();
        assert_eq!(telem, parsed);
    }

    #[test]
    fn test_domain_lifecycle_helpers() {
        assert_eq!(DomainLifecycle::default(), DomainLifecycle::Unavailable);
        assert!(!DomainLifecycle::Unavailable.is_ready());
        assert!(!DomainLifecycle::Connecting.is_ready());
        assert!(DomainLifecycle::Ready.is_ready());
        assert!(!DomainLifecycle::Reconnecting.is_ready());
        assert!(!DomainLifecycle::Degraded.is_ready());

        assert_eq!(DomainLifecycle::Unavailable.state_name(), "unavailable");
        assert_eq!(DomainLifecycle::Connecting.state_name(), "connecting");
        assert_eq!(DomainLifecycle::Ready.state_name(), "ready");
        assert_eq!(DomainLifecycle::Reconnecting.state_name(), "reconnecting");
        assert_eq!(DomainLifecycle::Degraded.state_name(), "degraded");
    }

    #[test]
    fn test_cancellation_reason_display() {
        assert_eq!(
            format!("{}", CancellationReason::User),
            "cancelled by user or dropped ticket"
        );
        assert_eq!(
            format!("{}", CancellationReason::Reconnect),
            "compositor reconnected or changed state"
        );
        assert_eq!(
            format!("{}", CancellationReason::Shutdown),
            "broker shutdown"
        );
        assert_eq!(
            format!("{}", CancellationReason::OwnerReplaced),
            "owner generation replaced"
        );
        assert_eq!(
            format!("{}", CancellationReason::Superseded),
            "superseded by newer command"
        );
        assert_eq!(
            format!("{}", CancellationReason::Timeout),
            "command deadline elapsed"
        );
    }

    #[test]
    fn test_supervision_constants() {
        assert_eq!(INITIAL_BACKOFF_MS, 250);
        assert_eq!(MAX_BACKOFF_MS, 30_000);
        assert_eq!(FAILURE_WINDOW_MS, 60_000);
        assert_eq!(STABLE_RESET_MS, 300_000);
        assert_eq!(QUARANTINE_FAILURES, 5);
    }

    #[test]
    fn test_monotonic_time_source() {
        let ts = MonotonicTimeSource::new();
        let t1 = ts.now_ms();
        std::thread::sleep(std::time::Duration::from_millis(2));
        let t2 = ts.now_ms();
        assert!(t2 >= t1);
    }

    #[test]
    fn test_mailbox_error() {
        assert_eq!(
            format!("{}", MailboxError::Unavailable),
            "service is offline or mailbox closed"
        );
        assert_eq!(
            format!("{}", MailboxError::Overloaded),
            "mailbox is at capacity"
        );
        assert_ne!(MailboxError::Unavailable, MailboxError::Overloaded);
    }

    #[test]
    fn test_reconnect_backoff_ms() {
        assert_eq!(reconnect_backoff_ms(0), 250);
        assert_eq!(reconnect_backoff_ms(1), 250);
        assert_eq!(reconnect_backoff_ms(2), 500);
        assert_eq!(reconnect_backoff_ms(4), 2000);
        assert_eq!(reconnect_backoff_ms(7), 16000);
        assert_eq!(reconnect_backoff_ms(8), 30000);
        assert_eq!(reconnect_backoff_ms(32), 30000);
    }

    #[test]
    fn test_supervisor_initial_state_and_mark_running() {
        let mut sup = DomainSupervisor::new();
        assert_eq!(sup.state(), SupervisorState::Starting);
        assert!(!sup.is_running());
        assert!(!sup.is_quarantined());
        assert_eq!(sup.last_running_timestamp_ms(), None);
        assert_eq!(sup.failure_count(), 0);

        sup.mark_running(10_000);
        assert_eq!(sup.state(), SupervisorState::Running);
        assert!(sup.is_running());
        assert_eq!(sup.last_running_timestamp_ms(), Some(10_000));
    }

    #[test]
    fn test_supervisor_rolling_window_trims_correctly_at_boundary() {
        let mut sup = DomainSupervisor::new();
        // 4 failures inside window
        sup.record_failure(0);
        sup.record_failure(10_000);
        sup.record_failure(20_000);
        sup.record_failure(30_000);
        assert_eq!(sup.failure_count(), 4);

        // 5th failure at exactly boundary (60_000 - 0 <= 60_000) -> 5 failures, trips quarantine
        let state = sup.record_failure(60_000);
        assert_eq!(state, SupervisorState::Quarantined);
        assert_eq!(sup.failure_count(), 5);

        // Now test trimming outside boundary
        let mut sup2 = DomainSupervisor::new();
        sup2.record_failure(0);
        assert_eq!(sup2.failure_count(), 1);

        // Failure at 60_001 trims the failure at 0 (60_001 - 0 > 60_000)
        let state2 = sup2.record_failure(60_001);
        assert_eq!(
            state2,
            SupervisorState::Backoff {
                attempt: 1,
                retry_at_ms: 60_001 + 250,
            }
        );
        assert_eq!(sup2.failure_count(), 1);
    }

    #[test]
    fn test_supervisor_quarantine_trips_at_exact_5th_failure() {
        let mut sup = DomainSupervisor::new();
        for i in 1..=4 {
            let state = sup.record_failure(i * 1_000);
            assert_eq!(
                state,
                SupervisorState::Backoff {
                    attempt: i as u32,
                    retry_at_ms: (i * 1_000) + reconnect_backoff_ms(i as u32),
                }
            );
            assert_eq!(sup.failure_count(), i as usize);
            assert!(!sup.is_quarantined());
        }

        let state5 = sup.record_failure(5_000);
        assert_eq!(state5, SupervisorState::Quarantined);
        assert!(sup.is_quarantined());
        assert_eq!(sup.failure_count(), 5);
    }

    #[test]
    fn test_supervisor_tick_expires_backoff_and_stable_reset() {
        let mut sup = DomainSupervisor::new();
        sup.record_failure(1_000);
        assert_eq!(
            sup.state(),
            SupervisorState::Backoff {
                attempt: 1,
                retry_at_ms: 1_250,
            }
        );

        // Tick before deadline -> stays in Backoff
        sup.tick(1_249);
        assert_eq!(
            sup.state(),
            SupervisorState::Backoff {
                attempt: 1,
                retry_at_ms: 1_250,
            }
        );

        // Tick at/after deadline -> transitions to Starting
        sup.tick(1_250);
        assert_eq!(sup.state(), SupervisorState::Starting);

        // Mark running and test stable reset
        sup.mark_running(2_000);
        assert_eq!(sup.failure_count(), 1);

        // Tick before stable reset duration (300_000 ms)
        sup.tick(2_000 + STABLE_RESET_MS - 1);
        assert_eq!(sup.failure_count(), 1);

        // Tick after stable reset duration
        sup.tick(2_000 + STABLE_RESET_MS);
        assert_eq!(sup.failure_count(), 0);
    }

    #[test]
    fn test_supervisor_mark_starting_preserves_failure_history() {
        let mut sup = DomainSupervisor::new();
        sup.record_failure(1_000);
        sup.record_failure(2_000);
        assert_eq!(sup.failure_count(), 2);
        assert!(matches!(sup.state(), SupervisorState::Backoff { .. }));

        // A restart-triggered re-entry into Starting must not reset the
        // rolling failure window, or repeated rapid restarts could never
        // accumulate enough failures to trip quarantine.
        sup.mark_starting();
        assert_eq!(sup.state(), SupervisorState::Starting);
        assert_eq!(sup.failure_count(), 2);

        // The next failure builds on the preserved history, not from zero.
        let state = sup.record_failure(3_000);
        assert_eq!(sup.failure_count(), 3);
        assert_eq!(
            state,
            SupervisorState::Backoff {
                attempt: 3,
                retry_at_ms: 3_000 + reconnect_backoff_ms(3),
            }
        );
    }

    #[test]
    fn test_supervisor_reset_quarantine_guarded() {
        let mut sup = DomainSupervisor::new();
        // In Starting: no-op
        assert!(!sup.reset_quarantine());
        assert_eq!(sup.state(), SupervisorState::Starting);

        // In Running: no-op
        sup.mark_running(1_000);
        assert!(!sup.reset_quarantine());
        assert_eq!(sup.state(), SupervisorState::Running);

        // In Backoff: no-op
        sup.record_failure(2_000);
        assert!(!sup.reset_quarantine());
        assert!(matches!(sup.state(), SupervisorState::Backoff { .. }));

        // Trip to Quarantined
        for t in [3_000, 4_000, 5_000, 6_000] {
            sup.record_failure(t);
        }
        assert!(sup.is_quarantined());
        assert_eq!(sup.failure_count(), 5);

        // Reset quarantine when Quarantined: succeeds and resets to Starting
        assert!(sup.reset_quarantine());
        assert_eq!(sup.state(), SupervisorState::Starting);
        assert_eq!(sup.failure_count(), 0);
        assert_eq!(sup.last_running_timestamp_ms(), None);
    }

    #[test]
    fn test_supervisor_enter_stopping_and_stopped() {
        let mut sup = DomainSupervisor::new();
        sup.mark_running(1_000);
        sup.enter_stopping();
        assert_eq!(sup.state(), SupervisorState::Stopping);
        assert_eq!(sup.last_running_timestamp_ms(), None);

        sup.enter_stopped();
        assert_eq!(sup.state(), SupervisorState::Stopped);
        assert_eq!(sup.last_running_timestamp_ms(), None);

        // Idempotent calls
        sup.enter_stopped();
        assert_eq!(sup.state(), SupervisorState::Stopped);
    }
}
