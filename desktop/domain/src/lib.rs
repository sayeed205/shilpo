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
}
