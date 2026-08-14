use crate::worker::protocol::ExtensionRuntimeKind;
use serde::{Deserialize, Serialize};
use shilpo_ext_api::ExtensionId;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

const MAX_DIAGNOSTICS: usize = 256;
const MAX_PENDING_NOTICES: usize = 256;

pub trait MonotonicClock: Send + Sync {
    fn now(&self) -> Instant;
}

#[derive(Clone, Default)]
pub struct SystemMonotonicClock;

impl MonotonicClock for SystemMonotonicClock {
    fn now(&self) -> Instant {
        Instant::now()
    }
}

#[derive(Clone)]
pub struct FakeMonotonicClock {
    now: Arc<std::sync::Mutex<Instant>>,
}

impl FakeMonotonicClock {
    pub fn new(start: Instant) -> Self {
        Self {
            now: Arc::new(std::sync::Mutex::new(start)),
        }
    }

    pub fn advance(&self, duration: Duration) {
        let mut lock = self.now.lock().unwrap();
        *lock += duration;
    }
}

impl MonotonicClock for FakeMonotonicClock {
    fn now(&self) -> Instant {
        *self.now.lock().unwrap()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticLevel {
    Info,
    Warning,
    Error,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticCode {
    RuntimeLoad,
    RuntimeTrap,
    RuntimeTimeout,
    FuelExhausted,
    MemoryLimit,
    InvalidOutput,
    InvalidView,
    CapabilityDenied,
    CircuitOpen,
    CircuitHalfOpen,
    CircuitRecovered,
    CircuitReset,
    CircuitPermanentlyDisabled,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtensionDiagnostic {
    pub level: DiagnosticLevel,
    pub code: DiagnosticCode,
    pub extension_id: ExtensionId,
    pub message: String,
    pub occurred_at: SystemTime,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CircuitStateKind {
    Closed,
    Open,
    HalfOpen,
    PermanentlyDisabled,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CircuitNoticeKind {
    Opened {
        trip_count: u32,
        retry_after_ms: u64,
    },
    Recovered {
        trip_count: u32,
    },
    PermanentlyDisabled {
        trip_count: u32,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CircuitNotice {
    pub extension_id: ExtensionId,
    pub kind: CircuitNoticeKind,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WasmExtensionStatus {
    pub id: ExtensionId,
    #[serde(default)]
    pub runtime_kind: ExtensionRuntimeKind,
    pub state: CircuitStateKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub consecutive_failures: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub consecutive_successes: Option<u32>,
    pub trip_count: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_after_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest_diagnostic: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CircuitBreakerPolicy {
    pub failure_threshold: u32,
    pub success_threshold: u32,
    pub retry_delays: Vec<Duration>,
    pub max_trip_cycles: u32,
}

impl Default for CircuitBreakerPolicy {
    fn default() -> Self {
        Self {
            failure_threshold: 3,
            success_threshold: 3,
            retry_delays: vec![
                Duration::from_secs(30),
                Duration::from_secs(60),
                Duration::from_secs(120),
                Duration::from_secs(300),
            ],
            max_trip_cycles: 4,
        }
    }
}

impl CircuitBreakerPolicy {
    pub fn retry_delay_for_trip(&self, trip_count: u32) -> Duration {
        if self.retry_delays.is_empty() {
            return Duration::from_secs(30);
        }
        let index = (trip_count.saturating_sub(1)) as usize;
        if index < self.retry_delays.len() {
            self.retry_delays[index]
        } else {
            *self.retry_delays.last().unwrap()
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum State {
    Closed {
        consecutive_failures: u32,
    },
    Open {
        trip_count: u32,
        retry_at: Instant,
        retry_delay: Duration,
    },
    HalfOpen {
        trip_count: u32,
        consecutive_successes: u32,
        probe_in_flight: bool,
    },
    PermanentlyDisabled {
        trip_count: u32,
    },
}

enum TransitionOutcome {
    Opened {
        trip_count: u32,
        retry_delay: Duration,
    },
    PermanentlyDisabled {
        trip_count: u32,
    },
}

#[derive(Debug)]
pub struct ProbePermit {
    extension_id: Option<ExtensionId>,
}

impl ProbePermit {
    pub fn none() -> Self {
        Self { extension_id: None }
    }

    pub fn probe(id: ExtensionId) -> Self {
        Self {
            extension_id: Some(id),
        }
    }

    pub fn is_probe(&self) -> bool {
        self.extension_id.is_some()
    }
}

pub struct CircuitBreaker {
    policy: CircuitBreakerPolicy,
    clock: Arc<dyn MonotonicClock>,
    states: HashMap<ExtensionId, State>,
    diagnostics: Vec<ExtensionDiagnostic>,
    pending_notices: Vec<CircuitNotice>,
    visible_changed: bool,
}

impl Default for CircuitBreaker {
    fn default() -> Self {
        Self::new(
            CircuitBreakerPolicy::default(),
            Arc::new(SystemMonotonicClock),
        )
    }
}

impl CircuitBreaker {
    pub fn new(policy: CircuitBreakerPolicy, clock: Arc<dyn MonotonicClock>) -> Self {
        Self {
            policy,
            clock,
            states: HashMap::new(),
            diagnostics: Vec::new(),
            pending_notices: Vec::new(),
            visible_changed: false,
        }
    }

    pub fn new_with_threshold(max_failures: u32) -> Self {
        let policy = CircuitBreakerPolicy {
            failure_threshold: max_failures.max(1),
            ..Default::default()
        };
        Self::new(policy, Arc::new(SystemMonotonicClock))
    }

    pub fn policy(&self) -> &CircuitBreakerPolicy {
        &self.policy
    }

    pub fn with_policy(mut self, policy: CircuitBreakerPolicy) -> Self {
        self.policy = policy;
        self
    }

    pub fn with_clock(mut self, clock: Arc<dyn MonotonicClock>) -> Self {
        self.clock = clock;
        self
    }

    pub fn acquire_permit(
        &mut self,
        id: &ExtensionId,
    ) -> Result<ProbePermit, crate::adapter::HostError> {
        let now = self.clock.now();
        let mut half_open_diag = None;

        if let Some(State::Open {
            trip_count,
            retry_at,
            ..
        }) = self.states.get(id)
            && now >= *retry_at
        {
            let tc = *trip_count;
            self.states.insert(
                id.clone(),
                State::HalfOpen {
                    trip_count: tc,
                    consecutive_successes: 0,
                    probe_in_flight: false,
                },
            );
            half_open_diag = Some(tc);
            self.visible_changed = true;
        }

        if let Some(tc) = half_open_diag {
            self.push_diagnostic(ExtensionDiagnostic {
                level: DiagnosticLevel::Info,
                code: DiagnosticCode::CircuitHalfOpen,
                extension_id: id.clone(),
                message: format!("circuit entered half-open probe state (trip cycle {tc})"),
                occurred_at: SystemTime::now(),
            });
        }

        let state = self.states.entry(id.clone()).or_insert(State::Closed {
            consecutive_failures: 0,
        });

        match state {
            State::Closed { .. } => Ok(ProbePermit::none()),
            State::HalfOpen {
                probe_in_flight, ..
            } => {
                if *probe_in_flight {
                    Err(crate::adapter::HostError::Disabled(id.clone()))
                } else {
                    *probe_in_flight = true;
                    Ok(ProbePermit::probe(id.clone()))
                }
            }
            State::Open { .. } | State::PermanentlyDisabled { .. } => {
                Err(crate::adapter::HostError::Disabled(id.clone()))
            }
        }
    }

    pub fn release_probe(&mut self, id: &ExtensionId) {
        if let Some(State::HalfOpen {
            probe_in_flight, ..
        }) = self.states.get_mut(id)
        {
            *probe_in_flight = false;
        }
    }

    pub fn record_success(&mut self, id: &ExtensionId) {
        let mut recovered = None;
        if let Some(state) = self.states.get_mut(id) {
            match state {
                State::Closed {
                    consecutive_failures,
                } => {
                    *consecutive_failures = 0;
                }
                State::HalfOpen {
                    trip_count,
                    consecutive_successes,
                    probe_in_flight,
                } => {
                    *probe_in_flight = false;
                    *consecutive_successes += 1;
                    if *consecutive_successes >= self.policy.success_threshold {
                        let prev_trips = *trip_count;
                        *state = State::Closed {
                            consecutive_failures: 0,
                        };
                        recovered = Some(prev_trips);
                    }
                }
                State::Open { .. } | State::PermanentlyDisabled { .. } => {}
            }
        } else {
            self.states.insert(
                id.clone(),
                State::Closed {
                    consecutive_failures: 0,
                },
            );
        }

        if let Some(prev_trips) = recovered {
            self.push_diagnostic(ExtensionDiagnostic {
                level: DiagnosticLevel::Info,
                code: DiagnosticCode::CircuitRecovered,
                extension_id: id.clone(),
                message: format!("circuit recovered and closed after {prev_trips} trip cycle(s)"),
                occurred_at: SystemTime::now(),
            });
            self.push_notice(CircuitNotice {
                extension_id: id.clone(),
                kind: CircuitNoticeKind::Recovered {
                    trip_count: prev_trips,
                },
                message: format!("Extension '{id}' recovered after {prev_trips} trip cycle(s)"),
            });
            self.visible_changed = true;
        }
    }

    pub fn record_failure(
        &mut self,
        id: &ExtensionId,
        code: DiagnosticCode,
        message: impl Into<String>,
    ) -> bool {
        let msg = message.into();
        self.push_diagnostic(ExtensionDiagnostic {
            level: DiagnosticLevel::Error,
            code,
            extension_id: id.clone(),
            message: msg,
            occurred_at: SystemTime::now(),
        });

        let now = self.clock.now();
        let mut transition = None;

        if let Some(state) = self.states.get_mut(id) {
            match state {
                State::Closed {
                    consecutive_failures,
                } => {
                    *consecutive_failures += 1;
                    if *consecutive_failures >= self.policy.failure_threshold {
                        let trip_count = 1;
                        let retry_delay = self.policy.retry_delay_for_trip(trip_count);
                        let retry_at = now + retry_delay;
                        *state = State::Open {
                            trip_count,
                            retry_at,
                            retry_delay,
                        };
                        transition = Some(TransitionOutcome::Opened {
                            trip_count,
                            retry_delay,
                        });
                    }
                }
                State::HalfOpen {
                    trip_count,
                    probe_in_flight,
                    ..
                } => {
                    *probe_in_flight = false;
                    let next_trip = *trip_count + 1;
                    if next_trip > self.policy.max_trip_cycles {
                        let tc = *trip_count;
                        *state = State::PermanentlyDisabled { trip_count: tc };
                        transition =
                            Some(TransitionOutcome::PermanentlyDisabled { trip_count: tc });
                    } else {
                        let retry_delay = self.policy.retry_delay_for_trip(next_trip);
                        let retry_at = now + retry_delay;
                        *state = State::Open {
                            trip_count: next_trip,
                            retry_at,
                            retry_delay,
                        };
                        transition = Some(TransitionOutcome::Opened {
                            trip_count: next_trip,
                            retry_delay,
                        });
                    }
                }
                State::Open { .. } | State::PermanentlyDisabled { .. } => {}
            }
        } else if self.policy.failure_threshold <= 1 {
            let trip_count = 1;
            let retry_delay = self.policy.retry_delay_for_trip(trip_count);
            let retry_at = now + retry_delay;
            self.states.insert(
                id.clone(),
                State::Open {
                    trip_count,
                    retry_at,
                    retry_delay,
                },
            );
            transition = Some(TransitionOutcome::Opened {
                trip_count,
                retry_delay,
            });
        } else {
            self.states.insert(
                id.clone(),
                State::Closed {
                    consecutive_failures: 1,
                },
            );
        }

        match transition {
            Some(TransitionOutcome::Opened {
                trip_count,
                retry_delay,
            }) => {
                self.push_diagnostic(ExtensionDiagnostic {
                    level: DiagnosticLevel::Error,
                    code: DiagnosticCode::CircuitOpen,
                    extension_id: id.clone(),
                    message: format!(
                        "circuit opened (trip {trip_count}): disabled temporarily, retrying in {:?}",
                        retry_delay
                    ),
                    occurred_at: SystemTime::now(),
                });
                self.push_notice(CircuitNotice {
                    extension_id: id.clone(),
                    kind: CircuitNoticeKind::Opened {
                        trip_count,
                        retry_after_ms: retry_delay.as_millis() as u64,
                    },
                    message: format!("Extension '{id}' temporarily disabled (trip {trip_count})"),
                });
                self.visible_changed = true;
                true
            }
            Some(TransitionOutcome::PermanentlyDisabled { trip_count }) => {
                self.push_diagnostic(ExtensionDiagnostic {
                    level: DiagnosticLevel::Error,
                    code: DiagnosticCode::CircuitPermanentlyDisabled,
                    extension_id: id.clone(),
                    message: format!(
                        "circuit permanently disabled for session after {} failed trip cycles",
                        self.policy.max_trip_cycles
                    ),
                    occurred_at: SystemTime::now(),
                });
                self.push_notice(CircuitNotice {
                    extension_id: id.clone(),
                    kind: CircuitNoticeKind::PermanentlyDisabled { trip_count },
                    message: format!(
                        "Extension '{id}' permanently disabled for session after {} failed trip cycles",
                        self.policy.max_trip_cycles
                    ),
                });
                self.visible_changed = true;
                true
            }
            None => false,
        }
    }

    pub fn advance_time(&mut self, now: Instant) -> bool {
        let mut transitions = Vec::new();
        for (id, state) in &mut self.states {
            if let State::Open {
                trip_count,
                retry_at,
                ..
            } = state
                && now >= *retry_at
            {
                let tc = *trip_count;
                *state = State::HalfOpen {
                    trip_count: tc,
                    consecutive_successes: 0,
                    probe_in_flight: false,
                };
                transitions.push((id.clone(), tc));
            }
        }
        let transitioned = !transitions.is_empty();
        if transitioned {
            self.visible_changed = true;
            for (id, tc) in transitions {
                self.push_diagnostic(ExtensionDiagnostic {
                    level: DiagnosticLevel::Info,
                    code: DiagnosticCode::CircuitHalfOpen,
                    extension_id: id,
                    message: format!("circuit entered half-open probe state (trip cycle {tc})"),
                    occurred_at: SystemTime::now(),
                });
            }
        }
        transitioned
    }

    /// Advances using the same injected clock used for admission and deadlines.
    pub fn advance_clock(&mut self) -> bool {
        self.advance_time(self.clock.now())
    }

    pub fn next_retry_deadline(&self) -> Option<Duration> {
        let now = self.clock.now();
        let mut nearest: Option<Duration> = None;
        for state in self.states.values() {
            if let State::Open { retry_at, .. } = *state {
                let remaining = retry_at.saturating_duration_since(now);
                nearest = Some(match nearest {
                    Some(cur) => cur.min(remaining),
                    None => remaining,
                });
            }
        }
        nearest
    }

    pub fn status(&self, id: &ExtensionId) -> WasmExtensionStatus {
        let now = self.clock.now();
        let state = self.states.get(id).unwrap_or(&State::Closed {
            consecutive_failures: 0,
        });
        let latest_diagnostic = self
            .diagnostics
            .iter()
            .rev()
            .find(|d| d.extension_id == *id && d.level == DiagnosticLevel::Error)
            .map(|d| d.message.clone());

        match state {
            State::Closed {
                consecutive_failures,
            } => WasmExtensionStatus {
                id: id.clone(),
                runtime_kind: ExtensionRuntimeKind::Wasm,
                state: CircuitStateKind::Closed,
                consecutive_failures: Some(*consecutive_failures),
                consecutive_successes: None,
                trip_count: 0,
                retry_after_ms: None,
                latest_diagnostic,
            },
            State::Open {
                trip_count,
                retry_at,
                ..
            } => {
                let retry_after_ms = if *retry_at > now {
                    (*retry_at - now).as_millis() as u64
                } else {
                    0
                };
                WasmExtensionStatus {
                    id: id.clone(),
                    runtime_kind: ExtensionRuntimeKind::Wasm,
                    state: CircuitStateKind::Open,
                    consecutive_failures: None,
                    consecutive_successes: None,
                    trip_count: *trip_count,
                    retry_after_ms: Some(retry_after_ms),
                    latest_diagnostic,
                }
            }
            State::HalfOpen {
                trip_count,
                consecutive_successes,
                ..
            } => WasmExtensionStatus {
                id: id.clone(),
                runtime_kind: ExtensionRuntimeKind::Wasm,
                state: CircuitStateKind::HalfOpen,
                consecutive_failures: None,
                consecutive_successes: Some(*consecutive_successes),
                trip_count: *trip_count,
                retry_after_ms: None,
                latest_diagnostic,
            },
            State::PermanentlyDisabled { trip_count } => WasmExtensionStatus {
                id: id.clone(),
                runtime_kind: ExtensionRuntimeKind::Wasm,
                state: CircuitStateKind::PermanentlyDisabled,
                consecutive_failures: None,
                consecutive_successes: None,
                trip_count: *trip_count,
                retry_after_ms: None,
                latest_diagnostic,
            },
        }
    }

    pub fn is_tripped(&self, id: &ExtensionId) -> bool {
        matches!(
            self.states.get(id),
            Some(State::Open { .. } | State::PermanentlyDisabled { .. })
        )
    }

    pub fn is_disabled(&self, id: &ExtensionId) -> bool {
        self.is_tripped(id)
    }

    pub fn reset(&mut self, id: &ExtensionId) {
        self.states.insert(
            id.clone(),
            State::Closed {
                consecutive_failures: 0,
            },
        );
        self.pending_notices
            .retain(|notice| notice.extension_id != *id);
        self.push_diagnostic(ExtensionDiagnostic {
            level: DiagnosticLevel::Info,
            code: DiagnosticCode::CircuitReset,
            extension_id: id.clone(),
            message: "circuit breaker manually reset".to_string(),
            occurred_at: SystemTime::now(),
        });
        self.visible_changed = true;
    }

    pub fn remove(&mut self, id: &ExtensionId) {
        self.states.remove(id);
        self.pending_notices.retain(|n| n.extension_id != *id);
        self.visible_changed = true;
    }

    pub fn clear(&mut self) {
        self.states.clear();
        self.pending_notices.clear();
        self.visible_changed = true;
    }

    pub fn take_pending_notices(&mut self) -> Vec<CircuitNotice> {
        std::mem::take(&mut self.pending_notices)
    }

    fn push_notice(&mut self, notice: CircuitNotice) {
        if self.pending_notices.len() == MAX_PENDING_NOTICES {
            self.pending_notices.remove(0);
        }
        self.pending_notices.push(notice);
    }

    pub fn take_visible_changed(&mut self) -> bool {
        let changed = self.visible_changed;
        self.visible_changed = false;
        changed
    }

    pub fn diagnostics(&self) -> &[ExtensionDiagnostic] {
        &self.diagnostics
    }

    pub fn clear_diagnostics(&mut self) {
        self.diagnostics.clear();
    }

    fn push_diagnostic(&mut self, diagnostic: ExtensionDiagnostic) {
        if self.diagnostics.len() == MAX_DIAGNOSTICS {
            self.diagnostics.remove(0);
        }
        self.diagnostics.push(diagnostic);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_id(name: &str) -> ExtensionId {
        ExtensionId::new(format!("io.github.test.{name}")).unwrap()
    }

    #[test]
    fn test_closed_failure_counting_and_success_reset() {
        let clock = Arc::new(FakeMonotonicClock::new(Instant::now()));
        let mut breaker = CircuitBreaker::new(CircuitBreakerPolicy::default(), clock);
        let ext = test_id("weather");

        assert!(!breaker.is_disabled(&ext));
        let status = breaker.status(&ext);
        assert_eq!(status.state, CircuitStateKind::Closed);
        assert_eq!(status.consecutive_failures, Some(0));

        // 1st failure
        assert!(!breaker.record_failure(&ext, DiagnosticCode::RuntimeTrap, "trap 1"));
        assert!(!breaker.is_disabled(&ext));
        assert_eq!(breaker.status(&ext).consecutive_failures, Some(1));

        // 2nd failure
        assert!(!breaker.record_failure(&ext, DiagnosticCode::RuntimeTimeout, "timeout 2"));
        assert!(!breaker.is_disabled(&ext));
        assert_eq!(breaker.status(&ext).consecutive_failures, Some(2));

        // Success clears failure count
        breaker.record_success(&ext);
        assert_eq!(breaker.status(&ext).consecutive_failures, Some(0));
        assert!(!breaker.is_disabled(&ext));
    }

    #[test]
    fn test_complete_retry_delays_schedule_and_cap() {
        let start = Instant::now();
        let clock = Arc::new(FakeMonotonicClock::new(start));
        let mut breaker = CircuitBreaker::new(CircuitBreakerPolicy::default(), clock.clone());
        let ext = test_id("schedule");

        // Cycle 1: 3 failures -> Open for 30s
        breaker.record_failure(&ext, DiagnosticCode::RuntimeTrap, "f1");
        breaker.record_failure(&ext, DiagnosticCode::RuntimeTrap, "f2");
        assert!(breaker.record_failure(&ext, DiagnosticCode::RuntimeTrap, "f3"));
        assert!(breaker.is_disabled(&ext));
        let status = breaker.status(&ext);
        assert_eq!(status.state, CircuitStateKind::Open);
        assert_eq!(status.trip_count, 1);
        assert_eq!(status.retry_after_ms, Some(30_000));

        // Before deadline rejection
        clock.advance(Duration::from_secs(10));
        assert!(breaker.acquire_permit(&ext).is_err());
        assert_eq!(breaker.status(&ext).retry_after_ms, Some(20_000));

        // Exact deadline: 30s total -> HalfOpen
        clock.advance(Duration::from_secs(20));
        let permit = breaker.acquire_permit(&ext).expect("should admit at 30s");
        assert!(permit.is_probe());
        assert_eq!(breaker.status(&ext).state, CircuitStateKind::HalfOpen);

        // Cycle 2: Probe fails -> Open for 60s (trip 2)
        assert!(breaker.record_failure(&ext, DiagnosticCode::RuntimeTrap, "probe f1"));
        assert_eq!(breaker.status(&ext).state, CircuitStateKind::Open);
        assert_eq!(breaker.status(&ext).trip_count, 2);
        assert_eq!(breaker.status(&ext).retry_after_ms, Some(60_000));

        // Advance 60s -> HalfOpen
        clock.advance(Duration::from_secs(60));
        let permit = breaker.acquire_permit(&ext).expect("should admit at 60s");
        assert!(permit.is_probe());

        // Cycle 3: Probe fails -> Open for 120s (trip 3)
        assert!(breaker.record_failure(&ext, DiagnosticCode::RuntimeTrap, "probe f2"));
        assert_eq!(breaker.status(&ext).state, CircuitStateKind::Open);
        assert_eq!(breaker.status(&ext).trip_count, 3);
        assert_eq!(breaker.status(&ext).retry_after_ms, Some(120_000));

        // Advance 120s -> HalfOpen
        clock.advance(Duration::from_secs(120));
        let permit = breaker.acquire_permit(&ext).expect("should admit at 120s");
        assert!(permit.is_probe());

        // Cycle 4: Probe fails -> Open for 300s (trip 4)
        assert!(breaker.record_failure(&ext, DiagnosticCode::RuntimeTrap, "probe f3"));
        assert_eq!(breaker.status(&ext).state, CircuitStateKind::Open);
        assert_eq!(breaker.status(&ext).trip_count, 4);
        assert_eq!(breaker.status(&ext).retry_after_ms, Some(300_000));

        // Advance 300s -> HalfOpen
        clock.advance(Duration::from_secs(300));
        let permit = breaker.acquire_permit(&ext).expect("should admit at 300s");
        assert!(permit.is_probe());

        // 4 failed cycles completed -> Probe fails -> PermanentlyDisabled
        assert!(breaker.record_failure(&ext, DiagnosticCode::RuntimeTrap, "probe f4"));
        assert_eq!(
            breaker.status(&ext).state,
            CircuitStateKind::PermanentlyDisabled
        );
        assert_eq!(breaker.status(&ext).trip_count, 4);

        // Future calls rejected permanently
        clock.advance(Duration::from_secs(10_000));
        assert!(breaker.acquire_permit(&ext).is_err());
        assert_eq!(
            breaker.status(&ext).state,
            CircuitStateKind::PermanentlyDisabled
        );
    }

    #[test]
    fn test_one_probe_admission_under_concurrent_attempts() {
        let start = Instant::now();
        let clock = Arc::new(FakeMonotonicClock::new(start));
        let mut breaker = CircuitBreaker::new(CircuitBreakerPolicy::default(), clock.clone());
        let ext = test_id("concurrent");

        // Trip the circuit
        breaker.record_failure(&ext, DiagnosticCode::RuntimeTrap, "f1");
        breaker.record_failure(&ext, DiagnosticCode::RuntimeTrap, "f2");
        breaker.record_failure(&ext, DiagnosticCode::RuntimeTrap, "f3");

        // Advance to retry deadline
        clock.advance(Duration::from_secs(30));

        // 1st caller acquires the half-open probe
        let permit1 = breaker
            .acquire_permit(&ext)
            .expect("first probe should be admitted");
        assert!(permit1.is_probe());

        // Concurrent caller is rejected without incrementing failures
        let permit2 = breaker.acquire_permit(&ext);
        assert!(permit2.is_err());

        // Status reflects half-open with probe in flight
        assert_eq!(breaker.status(&ext).state, CircuitStateKind::HalfOpen);

        // Success 1
        breaker.record_success(&ext);
        assert_eq!(breaker.status(&ext).consecutive_successes, Some(1));

        // Now next call can execute as probe 2
        let permit3 = breaker.acquire_permit(&ext).expect("next probe admitted");
        assert!(permit3.is_probe());

        // Concurrent rejected again
        assert!(breaker.acquire_permit(&ext).is_err());

        // Success 2
        breaker.record_success(&ext);
        assert_eq!(breaker.status(&ext).consecutive_successes, Some(2));

        // Probe 3
        let permit4 = breaker.acquire_permit(&ext).expect("third probe admitted");
        assert!(permit4.is_probe());

        // Success 3 -> Closes the circuit!
        breaker.record_success(&ext);
        assert_eq!(breaker.status(&ext).state, CircuitStateKind::Closed);
        assert_eq!(breaker.status(&ext).trip_count, 0);
        assert_eq!(breaker.status(&ext).consecutive_failures, Some(0));

        // Now normal closed calls can proceed
        let normal_permit = breaker.acquire_permit(&ext).expect("closed call admitted");
        assert!(!normal_permit.is_probe());
    }

    #[test]
    fn test_isolation_between_two_extension_ids() {
        let clock = Arc::new(FakeMonotonicClock::new(Instant::now()));
        let mut breaker = CircuitBreaker::new(CircuitBreakerPolicy::default(), clock);
        let ext1 = test_id("alice");
        let ext2 = test_id("bob");

        // Trip ext1
        breaker.record_failure(&ext1, DiagnosticCode::RuntimeTrap, "f1");
        breaker.record_failure(&ext1, DiagnosticCode::RuntimeTrap, "f2");
        breaker.record_failure(&ext1, DiagnosticCode::RuntimeTrap, "f3");

        assert!(breaker.is_disabled(&ext1));
        assert!(!breaker.is_disabled(&ext2));

        assert!(breaker.acquire_permit(&ext1).is_err());
        assert!(breaker.acquire_permit(&ext2).is_ok());
    }

    #[test]
    fn test_manual_reset_and_unload_cleanup() {
        let clock = Arc::new(FakeMonotonicClock::new(Instant::now()));
        let mut breaker = CircuitBreaker::new(CircuitBreakerPolicy::default(), clock);
        let ext = test_id("reset-test");

        // Trip ext
        breaker.record_failure(&ext, DiagnosticCode::RuntimeTrap, "f1");
        breaker.record_failure(&ext, DiagnosticCode::RuntimeTrap, "f2");
        breaker.record_failure(&ext, DiagnosticCode::RuntimeTrap, "f3");
        assert!(breaker.is_disabled(&ext));

        // Reset
        breaker.reset(&ext);
        assert!(!breaker.is_disabled(&ext));
        assert_eq!(breaker.status(&ext).state, CircuitStateKind::Closed);
        assert_eq!(breaker.status(&ext).trip_count, 0);

        // Remove
        breaker.remove(&ext);
        assert!(!breaker.is_disabled(&ext));
        assert_eq!(breaker.status(&ext).consecutive_failures, Some(0));
    }

    #[test]
    fn test_three_success_recovery_and_immediate_half_open_retrip() {
        let start = Instant::now();
        let clock = Arc::new(FakeMonotonicClock::new(start));
        let mut breaker = CircuitBreaker::new(CircuitBreakerPolicy::default(), clock.clone());
        let ext = test_id("retrip");

        // Trip 1
        breaker.record_failure(&ext, DiagnosticCode::RuntimeTrap, "f1");
        breaker.record_failure(&ext, DiagnosticCode::RuntimeTrap, "f2");
        breaker.record_failure(&ext, DiagnosticCode::RuntimeTrap, "f3");
        assert_eq!(breaker.status(&ext).state, CircuitStateKind::Open);
        assert_eq!(breaker.status(&ext).trip_count, 1);

        // Advance 30s -> HalfOpen
        clock.advance(Duration::from_secs(30));
        let _ = breaker.acquire_permit(&ext).unwrap();
        // Success 1
        breaker.record_success(&ext);
        assert_eq!(breaker.status(&ext).state, CircuitStateKind::HalfOpen);
        assert_eq!(breaker.status(&ext).consecutive_successes, Some(1));

        // Next probe
        let _ = breaker.acquire_permit(&ext).unwrap();
        // Success 2
        breaker.record_success(&ext);
        assert_eq!(breaker.status(&ext).state, CircuitStateKind::HalfOpen);
        assert_eq!(breaker.status(&ext).consecutive_successes, Some(2));

        // Next probe fails -> immediate retrip to Open with trip_count 2 and 60s delay!
        let _ = breaker.acquire_permit(&ext).unwrap();
        assert!(breaker.record_failure(&ext, DiagnosticCode::RuntimeTrap, "probe failure"));
        assert_eq!(breaker.status(&ext).state, CircuitStateKind::Open);
        assert_eq!(breaker.status(&ext).trip_count, 2);
        assert_eq!(breaker.status(&ext).retry_after_ms, Some(60_000));
    }

    #[test]
    fn test_bounded_diagnostics_and_no_spam_on_repeated_blocked_calls() {
        let clock = Arc::new(FakeMonotonicClock::new(Instant::now()));
        let mut breaker = CircuitBreaker::new(CircuitBreakerPolicy::default(), clock);
        let ext = test_id("spam");

        // Trip
        breaker.record_failure(&ext, DiagnosticCode::RuntimeTrap, "f1");
        breaker.record_failure(&ext, DiagnosticCode::RuntimeTrap, "f2");
        breaker.record_failure(&ext, DiagnosticCode::RuntimeTrap, "f3");
        assert_eq!(breaker.status(&ext).state, CircuitStateKind::Open);

        let initial_notices = breaker.take_pending_notices();
        assert_eq!(initial_notices.len(), 1);
        assert!(matches!(
            initial_notices[0].kind,
            CircuitNoticeKind::Opened { trip_count: 1, .. }
        ));

        // 100 repeated blocked calls
        for _ in 0..100 {
            assert!(breaker.acquire_permit(&ext).is_err());
        }

        // No new notices generated by blocked calls
        assert!(breaker.take_pending_notices().is_empty());

        // Generate 300 diagnostics to verify MAX_DIAGNOSTICS bound
        for i in 0..300 {
            breaker.record_failure(&ext, DiagnosticCode::RuntimeTrap, format!("err {i}"));
        }
        assert!(breaker.diagnostics().len() <= MAX_DIAGNOSTICS);
    }

    #[test]
    fn pending_transition_notices_are_bounded() {
        let clock = Arc::new(FakeMonotonicClock::new(Instant::now()));
        let policy = CircuitBreakerPolicy {
            failure_threshold: 1,
            ..Default::default()
        };
        let mut breaker = CircuitBreaker::new(policy, clock);

        for index in 0..(MAX_PENDING_NOTICES + 64) {
            let id = test_id(&format!("notice-{index}"));
            breaker.record_failure(&id, DiagnosticCode::RuntimeTrap, "trip");
        }

        assert_eq!(breaker.pending_notices.len(), MAX_PENDING_NOTICES);
    }

    #[test]
    fn test_probe_permit_release_on_unexecuted_call() {
        let start = Instant::now();
        let clock = Arc::new(FakeMonotonicClock::new(start));
        let mut breaker = CircuitBreaker::new(CircuitBreakerPolicy::default(), clock.clone());
        let ext = test_id("release-probe");

        // Trip
        breaker.record_failure(&ext, DiagnosticCode::RuntimeTrap, "f1");
        breaker.record_failure(&ext, DiagnosticCode::RuntimeTrap, "f2");
        breaker.record_failure(&ext, DiagnosticCode::RuntimeTrap, "f3");

        clock.advance(Duration::from_secs(30));
        let permit = breaker.acquire_permit(&ext).unwrap();
        assert!(permit.is_probe());

        // Concurrent call is blocked while permit in flight
        assert!(breaker.acquire_permit(&ext).is_err());

        // Call did not execute guest; release probe
        breaker.release_probe(&ext);

        // Next call can acquire probe
        let permit2 = breaker.acquire_permit(&ext).unwrap();
        assert!(permit2.is_probe());
    }
}
