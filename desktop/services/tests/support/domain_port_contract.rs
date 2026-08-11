use std::{
    collections::VecDeque,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};

/// DomainVersion tuple containing owner_generation and revision with strict lexicographical ordering.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
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

/// Consumer-facing lifecycle state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum DomainLifecycle {
    #[default]
    Unavailable,
    Connecting,
    Ready,
    Reconnecting,
    Degraded,
}

/// Supervisor operational state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SupervisorState {
    Starting,
    Running,
    Backoff { attempt: u32, retry_at_ms: u64 },
    Quarantined,
    Stopping,
    Stopped,
}

/// Domain snapshot containing version, lifecycle, payload, and error diagnostics.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DomainSnapshot<P> {
    pub version: DomainVersion,
    pub lifecycle: DomainLifecycle,
    pub payload: P,
    pub last_error: Option<String>,
}

/// Unique identifier for commands.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct CommandId(pub String);

/// Terminal outcome for accepted commands.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum CommandOutcome {
    Applied {
        version: DomainVersion,
    },
    ReconciledApplied {
        version: DomainVersion,
    },
    Rejected {
        reason: RejectionReason,
    },
    TimedOut {
        last_observed_version: DomainVersion,
    },
    Cancelled {
        reason: CancellationReason,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum RejectionReason {
    Unavailable,
    Overloaded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum CancellationReason {
    Shutdown,
    Reconnect,
    OwnerReplaced,
    Superseded,
}

/// Bounded mailbox overload policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MailboxPolicy {
    Lossless,
    ReplaceLatest { key: String },
}

/// Telemetry metrics for domain port observability.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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

/// Controllable manual clock for deterministic time advancement in contract tests.
#[derive(Debug, Clone, Default)]
pub struct ManualClock {
    now_ms: Arc<AtomicU64>,
}

impl ManualClock {
    pub fn new() -> Self {
        Self {
            now_ms: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn now_ms(&self) -> u64 {
        self.now_ms.load(Ordering::SeqCst)
    }

    pub fn advance_ms(&self, ms: u64) {
        self.now_ms.fetch_add(ms, Ordering::SeqCst);
    }

    #[allow(dead_code)]
    pub fn advance_secs(&self, secs: u64) {
        self.advance_ms(secs * 1000);
    }
}

/// Reference Audio-like domain payload for test harness.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TestPayload {
    pub volume: u8,
    pub is_muted: bool,
    pub device_name: String,
}

impl Default for TestPayload {
    fn default() -> Self {
        Self {
            volume: 50,
            is_muted: false,
            device_name: "Default Speakers".to_string(),
        }
    }
}

/// Reference typed command for test harness.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestCommand {
    pub id: CommandId,
    pub action: TestAction,
    pub policy: MailboxPolicy,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TestAction {
    SetVolume(u8),
    SetMuted(bool),
}

/// Error returned when publishing a stale or conflicting snapshot update.
#[derive(Debug, Clone, PartialEq, Eq)]
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

/// Handle / Ticket returned to caller when a command is submitted.
#[derive(Debug, Clone)]
pub struct CommandTicket {
    outcome: Arc<Mutex<Option<CommandOutcome>>>,
    completion_attempts: Arc<AtomicU64>,
}

impl CommandTicket {
    pub fn new() -> (Self, CommandResolver) {
        let outcome = Arc::new(Mutex::new(None));
        let completion_attempts = Arc::new(AtomicU64::new(0));
        let ticket = Self {
            outcome: outcome.clone(),
            completion_attempts: completion_attempts.clone(),
        };
        let resolver = CommandResolver {
            outcome,
            completion_attempts,
        };
        (ticket, resolver)
    }

    pub fn outcome(&self) -> Option<CommandOutcome> {
        self.outcome.lock().unwrap().clone()
    }

    pub fn is_completed(&self) -> bool {
        self.outcome.lock().unwrap().is_some()
    }

    pub fn completion_attempts(&self) -> u64 {
        self.completion_attempts.load(Ordering::SeqCst)
    }
}

#[derive(Debug, Clone)]
pub struct CommandResolver {
    outcome: Arc<Mutex<Option<CommandOutcome>>>,
    completion_attempts: Arc<AtomicU64>,
}

impl CommandResolver {
    pub fn resolve(&self, outcome: CommandOutcome) -> bool {
        self.completion_attempts.fetch_add(1, Ordering::SeqCst);
        let mut guard = self.outcome.lock().unwrap();
        if guard.is_none() {
            *guard = Some(outcome);
            true
        } else {
            false
        }
    }
}

#[derive(Debug, Clone)]
pub struct SnapshotSubscription {
    latest: Arc<Mutex<DomainSnapshot<TestPayload>>>,
}

impl SnapshotSubscription {
    pub fn latest(&self) -> DomainSnapshot<TestPayload> {
        self.latest.lock().unwrap().clone()
    }
}

/// Test driver trait exposed by support module so scenarios can be run against any driver.
pub trait DomainPortDriver {
    fn snapshot(&self) -> DomainSnapshot<TestPayload>;
    fn subscribe(&self) -> SnapshotSubscription;
    fn supervisor_state(&self) -> SupervisorState;
    fn telemetry(&self) -> DomainPortTelemetry;
    fn clock(&self) -> &ManualClock;
    fn advance_clock_ms(&self, ms: u64);
    fn advance_clock_secs(&self, secs: u64);

    fn begin_start(&self);
    fn mark_ready(&self);
    fn report_owner_failure(&self, error: String);
    fn publish_update(&self, revision: u64, payload: TestPayload) -> Result<(), StaleUpdateError>;
    fn publish_raw_update(
        &self,
        version: DomainVersion,
        lifecycle: DomainLifecycle,
        payload: TestPayload,
        error: Option<String>,
    ) -> Result<(), StaleUpdateError>;

    fn submit_command(&self, command: TestCommand) -> Result<CommandTicket, CommandOutcome>;
    fn ack_command_without_snapshot(&self) -> Option<CommandId>;
    fn process_pending_commands_and_converge(&self);
    fn reconcile_front_command(&self);
    fn timeout_front_command(&self);
    fn reset_quarantine(&self);
    fn restart_containing_process(&self);
    fn backoff_delay_ms(&self, attempt: u32) -> u64;
    #[allow(dead_code)]
    fn tick(&self);
}

struct PendingCommandItem {
    command: TestCommand,
    generation: u64,
    resolver: CommandResolver,
    backend_acked: bool,
}

struct ReferenceState {
    supervisor_state: SupervisorState,
    lifecycle: DomainLifecycle,
    owner_generation: u64,
    revision: u64,
    payload: TestPayload,
    last_error: Option<String>,
    had_prior_readiness: bool,
    last_running_timestamp_ms: Option<u64>,
    failure_timestamps_ms: Vec<u64>,
    backoff_attempt: u32,
    queue: VecDeque<PendingCommandItem>,
    in_flight: VecDeque<PendingCommandItem>,
    subscribers: Vec<Arc<Mutex<DomainSnapshot<TestPayload>>>>,
    overloads: u64,
    supersessions: u64,
    restarts: u64,
    stale_updates: u64,
}

pub struct ReferenceDomainPort {
    clock: ManualClock,
    capacity: usize,
    state: Mutex<ReferenceState>,
}

impl ReferenceDomainPort {
    pub fn new(capacity: usize) -> Self {
        assert!(
            capacity > 0,
            "domain command mailbox capacity must be positive"
        );
        Self {
            clock: ManualClock::new(),
            capacity,
            state: Mutex::new(ReferenceState {
                supervisor_state: SupervisorState::Starting,
                lifecycle: DomainLifecycle::Unavailable,
                owner_generation: 0,
                revision: 0,
                payload: TestPayload::default(),
                last_error: None,
                had_prior_readiness: false,
                last_running_timestamp_ms: None,
                failure_timestamps_ms: Vec::new(),
                backoff_attempt: 0,
                queue: VecDeque::new(),
                in_flight: VecDeque::new(),
                subscribers: Vec::new(),
                overloads: 0,
                supersessions: 0,
                restarts: 0,
                stale_updates: 0,
            }),
        }
    }

    fn snapshot_from_state(state: &ReferenceState) -> DomainSnapshot<TestPayload> {
        DomainSnapshot {
            version: DomainVersion::new(state.owner_generation, state.revision),
            lifecycle: state.lifecycle,
            payload: state.payload.clone(),
            last_error: state.last_error.clone(),
        }
    }

    fn notify_subscribers(state: &ReferenceState) {
        let snapshot = Self::snapshot_from_state(state);
        for subscriber in &state.subscribers {
            *subscriber.lock().unwrap() = snapshot.clone();
        }
    }

    fn cancel_generation_commands(state: &mut ReferenceState, reason: CancellationReason) {
        for item in std::mem::take(&mut state.queue) {
            item.resolver.resolve(CommandOutcome::Cancelled { reason });
        }
        for item in std::mem::take(&mut state.in_flight) {
            item.resolver.resolve(CommandOutcome::Cancelled { reason });
        }
    }

    fn backoff_delay_for_attempt(attempt: u32) -> u64 {
        let multiplier = 2u64.saturating_pow(attempt.saturating_sub(1));
        250u64.saturating_mul(multiplier).min(30_000)
    }

    fn apply_update(
        state: &mut ReferenceState,
        version: DomainVersion,
        lifecycle: DomainLifecycle,
        payload: TestPayload,
        last_error: Option<String>,
    ) -> Result<(), StaleUpdateError> {
        let current_version = DomainVersion::new(state.owner_generation, state.revision);
        if version.owner_generation > state.owner_generation {
            state.stale_updates += 1;
            return Err(StaleUpdateError::UninstalledGeneration {
                installed: state.owner_generation,
                attempted: version.owner_generation,
            });
        }
        if version < current_version {
            state.stale_updates += 1;
            return Err(StaleUpdateError::StaleVersion {
                current: current_version,
                attempted: version,
            });
        }
        if version == current_version {
            if state.lifecycle == lifecycle
                && state.payload == payload
                && state.last_error == last_error
            {
                return Ok(());
            }
            state.stale_updates += 1;
            return Err(StaleUpdateError::ConflictingSnapshot {
                version: current_version,
            });
        }

        state.owner_generation = version.owner_generation;
        state.revision = version.revision;
        state.lifecycle = lifecycle;
        state.payload = payload;
        state.last_error = last_error;
        Self::notify_subscribers(state);
        Ok(())
    }

    fn update_backoff_and_quarantine(state: &mut ReferenceState, now_ms: u64, error: String) {
        state.last_error = Some(error);
        state.failure_timestamps_ms.push(now_ms);
        state
            .failure_timestamps_ms
            .retain(|&ts| now_ms.saturating_sub(ts) <= 60_000);

        if state.failure_timestamps_ms.len() >= 5 {
            state.supervisor_state = SupervisorState::Quarantined;
            state.lifecycle = DomainLifecycle::Unavailable;
        } else {
            state.backoff_attempt += 1;
            let backoff_ms = Self::backoff_delay_for_attempt(state.backoff_attempt);
            let retry_at_ms = now_ms + backoff_ms;
            state.supervisor_state = SupervisorState::Backoff {
                attempt: state.backoff_attempt,
                retry_at_ms,
            };
            if state.had_prior_readiness {
                state.lifecycle = DomainLifecycle::Reconnecting;
            } else {
                state.lifecycle = DomainLifecycle::Unavailable;
            }
        }
    }

    fn check_clock_state(state: &mut ReferenceState, now_ms: u64) {
        match state.supervisor_state {
            SupervisorState::Backoff { retry_at_ms, .. } => {
                if now_ms >= retry_at_ms {
                    state.owner_generation += 1;
                    state.revision = 0;
                    state.restarts += 1;
                    // Cancel pending commands from prior generation
                    Self::cancel_generation_commands(state, CancellationReason::OwnerReplaced);

                    state.supervisor_state = SupervisorState::Starting;
                    state.lifecycle = if state.had_prior_readiness {
                        DomainLifecycle::Reconnecting
                    } else {
                        DomainLifecycle::Connecting
                    };
                    state.last_running_timestamp_ms = None;
                    Self::notify_subscribers(state);
                }
            }
            SupervisorState::Running => {
                if let Some(start_ts) = state.last_running_timestamp_ms
                    && now_ms.saturating_sub(start_ts) >= 300_000
                {
                    // 5 minutes stable reset
                    state.failure_timestamps_ms.clear();
                    state.backoff_attempt = 0;
                }
            }
            _ => {}
        }
    }
}

impl DomainPortDriver for ReferenceDomainPort {
    fn snapshot(&self) -> DomainSnapshot<TestPayload> {
        let mut state = self.state.lock().unwrap();
        Self::check_clock_state(&mut state, self.clock.now_ms());
        Self::snapshot_from_state(&state)
    }

    fn subscribe(&self) -> SnapshotSubscription {
        let mut state = self.state.lock().unwrap();
        Self::check_clock_state(&mut state, self.clock.now_ms());
        let latest = Arc::new(Mutex::new(Self::snapshot_from_state(&state)));
        state.subscribers.push(latest.clone());
        SnapshotSubscription { latest }
    }

    fn supervisor_state(&self) -> SupervisorState {
        let mut state = self.state.lock().unwrap();
        Self::check_clock_state(&mut state, self.clock.now_ms());
        state.supervisor_state
    }

    fn telemetry(&self) -> DomainPortTelemetry {
        let mut state = self.state.lock().unwrap();
        Self::check_clock_state(&mut state, self.clock.now_ms());
        DomainPortTelemetry {
            owner_generation: state.owner_generation,
            current_queue_depth: state.queue.len(),
            queue_capacity: self.capacity,
            overloads: state.overloads,
            supersessions: state.supersessions,
            restarts: state.restarts,
            stale_updates: state.stale_updates,
            last_error: state.last_error.clone(),
        }
    }

    fn clock(&self) -> &ManualClock {
        &self.clock
    }

    fn advance_clock_ms(&self, ms: u64) {
        self.clock.advance_ms(ms);
        let mut state = self.state.lock().unwrap();
        Self::check_clock_state(&mut state, self.clock.now_ms());
    }

    fn advance_clock_secs(&self, secs: u64) {
        self.advance_clock_ms(secs * 1000);
    }

    fn begin_start(&self) {
        let mut state = self.state.lock().unwrap();
        Self::cancel_generation_commands(&mut state, CancellationReason::OwnerReplaced);
        state.owner_generation = state.owner_generation.saturating_add(1);
        state.revision = 0;
        state.supervisor_state = SupervisorState::Starting;
        state.lifecycle = if state.had_prior_readiness {
            DomainLifecycle::Reconnecting
        } else {
            DomainLifecycle::Connecting
        };
        Self::notify_subscribers(&state);
    }

    fn mark_ready(&self) {
        let mut state = self.state.lock().unwrap();
        state.supervisor_state = SupervisorState::Running;
        state.lifecycle = DomainLifecycle::Ready;
        state.had_prior_readiness = true;
        state.last_running_timestamp_ms = Some(self.clock.now_ms());
        Self::notify_subscribers(&state);
    }

    fn report_owner_failure(&self, error: String) {
        let mut state = self.state.lock().unwrap();
        assert_eq!(
            state.supervisor_state,
            SupervisorState::Running,
            "only a running owner can report an unexpected failure"
        );
        Self::update_backoff_and_quarantine(&mut state, self.clock.now_ms(), error);
        Self::notify_subscribers(&state);
    }

    fn publish_update(&self, revision: u64, payload: TestPayload) -> Result<(), StaleUpdateError> {
        let mut state = self.state.lock().unwrap();
        Self::check_clock_state(&mut state, self.clock.now_ms());
        let version = DomainVersion::new(state.owner_generation, revision);
        let lifecycle = state.lifecycle;
        let last_error = state.last_error.clone();
        Self::apply_update(&mut state, version, lifecycle, payload, last_error)
    }

    fn publish_raw_update(
        &self,
        version: DomainVersion,
        lifecycle: DomainLifecycle,
        payload: TestPayload,
        error: Option<String>,
    ) -> Result<(), StaleUpdateError> {
        let mut state = self.state.lock().unwrap();
        Self::check_clock_state(&mut state, self.clock.now_ms());
        Self::apply_update(&mut state, version, lifecycle, payload, error)
    }

    fn submit_command(&self, command: TestCommand) -> Result<CommandTicket, CommandOutcome> {
        let mut state = self.state.lock().unwrap();
        Self::check_clock_state(&mut state, self.clock.now_ms());

        if matches!(state.lifecycle, DomainLifecycle::Unavailable)
            || matches!(
                state.supervisor_state,
                SupervisorState::Quarantined | SupervisorState::Stopped
            )
        {
            return Err(CommandOutcome::Rejected {
                reason: RejectionReason::Unavailable,
            });
        }

        match &command.policy {
            MailboxPolicy::Lossless => {
                if state.queue.len() >= self.capacity {
                    state.overloads += 1;
                    return Err(CommandOutcome::Rejected {
                        reason: RejectionReason::Overloaded,
                    });
                }
            }
            MailboxPolicy::ReplaceLatest { key } => {
                let mut replaced_idx = None;
                for (idx, item) in state.queue.iter().enumerate() {
                    if let MailboxPolicy::ReplaceLatest { key: existing_key } = &item.command.policy
                        && existing_key == key
                    {
                        replaced_idx = Some(idx);
                        break;
                    }
                }
                if let Some(idx) = replaced_idx {
                    let removed = state.queue.remove(idx).unwrap();
                    removed.resolver.resolve(CommandOutcome::Cancelled {
                        reason: CancellationReason::Superseded,
                    });
                    state.supersessions += 1;
                }
                if state.queue.len() >= self.capacity {
                    state.overloads += 1;
                    return Err(CommandOutcome::Rejected {
                        reason: RejectionReason::Overloaded,
                    });
                }
            }
        }

        let (ticket, resolver) = CommandTicket::new();
        let generation = state.owner_generation;
        state.queue.push_back(PendingCommandItem {
            command,
            generation,
            resolver,
            backend_acked: false,
        });

        Ok(ticket)
    }

    fn ack_command_without_snapshot(&self) -> Option<CommandId> {
        let mut state = self.state.lock().unwrap();
        if let Some(mut item) = state.queue.pop_front() {
            item.backend_acked = true;
            let command_id = item.command.id.clone();
            state.in_flight.push_back(item);
            Some(command_id)
        } else {
            None
        }
    }

    fn process_pending_commands_and_converge(&self) {
        let mut state = self.state.lock().unwrap();
        Self::check_clock_state(&mut state, self.clock.now_ms());

        let mut queue_items = std::mem::take(&mut state.in_flight);
        queue_items.append(&mut state.queue);
        for item in queue_items {
            if item.generation != state.owner_generation {
                item.resolver.resolve(CommandOutcome::Cancelled {
                    reason: CancellationReason::OwnerReplaced,
                });
                continue;
            }

            match item.command.action {
                TestAction::SetVolume(v) => state.payload.volume = v,
                TestAction::SetMuted(m) => state.payload.is_muted = m,
            }
            state.revision += 1;
            let version = DomainVersion {
                owner_generation: state.owner_generation,
                revision: state.revision,
            };
            item.resolver.resolve(CommandOutcome::Applied { version });
        }
        Self::notify_subscribers(&state);
    }

    fn reconcile_front_command(&self) {
        let mut state = self.state.lock().unwrap();
        if let Some(item) = state.queue.pop_front() {
            match item.command.action {
                TestAction::SetVolume(value) => state.payload.volume = value,
                TestAction::SetMuted(value) => state.payload.is_muted = value,
            }
            state.revision += 1;
            item.resolver.resolve(CommandOutcome::ReconciledApplied {
                version: DomainVersion::new(state.owner_generation, state.revision),
            });
            Self::notify_subscribers(&state);
        }
    }

    fn timeout_front_command(&self) {
        let mut state = self.state.lock().unwrap();
        if let Some(item) = state.queue.pop_front() {
            item.resolver.resolve(CommandOutcome::TimedOut {
                last_observed_version: DomainVersion::new(state.owner_generation, state.revision),
            });
        }
    }

    fn reset_quarantine(&self) {
        let mut state = self.state.lock().unwrap();
        if matches!(state.supervisor_state, SupervisorState::Quarantined) {
            state.failure_timestamps_ms.clear();
            state.backoff_attempt = 0;
            state.supervisor_state = SupervisorState::Starting;
            state.lifecycle = if state.had_prior_readiness {
                DomainLifecycle::Reconnecting
            } else {
                DomainLifecycle::Connecting
            };
            Self::notify_subscribers(&state);
        }
    }

    fn restart_containing_process(&self) {
        let mut state = self.state.lock().unwrap();
        Self::cancel_generation_commands(&mut state, CancellationReason::Shutdown);
        let subscribers = std::mem::take(&mut state.subscribers);
        *state = ReferenceState {
            supervisor_state: SupervisorState::Starting,
            lifecycle: DomainLifecycle::Unavailable,
            owner_generation: 0,
            revision: 0,
            payload: TestPayload::default(),
            last_error: None,
            had_prior_readiness: false,
            last_running_timestamp_ms: None,
            failure_timestamps_ms: Vec::new(),
            backoff_attempt: 0,
            queue: VecDeque::new(),
            in_flight: VecDeque::new(),
            subscribers,
            overloads: 0,
            supersessions: 0,
            restarts: 0,
            stale_updates: 0,
        };
        Self::notify_subscribers(&state);
    }

    fn backoff_delay_ms(&self, attempt: u32) -> u64 {
        Self::backoff_delay_for_attempt(attempt)
    }

    fn tick(&self) {
        let mut state = self.state.lock().unwrap();
        Self::check_clock_state(&mut state, self.clock.now_ms());
    }
}

// ---------------------------------------------------------------------------
// Reusable scenario functions exposing assertions for all 20 contract rules
// ---------------------------------------------------------------------------

fn start_ready(driver: &impl DomainPortDriver) {
    driver.begin_start();
    driver.mark_ready();
}

pub fn scenario_01_initial_projection_is_deterministic_and_unavailable(
    driver: &impl DomainPortDriver,
) {
    let snap = driver.snapshot();
    assert_eq!(snap.lifecycle, DomainLifecycle::Unavailable);
    assert_eq!(snap.version, DomainVersion::ZERO);
    assert_eq!(snap.payload, TestPayload::default());
    assert!(snap.last_error.is_none());
    assert_eq!(driver.supervisor_state(), SupervisorState::Starting);
}

pub fn scenario_02_initial_start_follows_unavailable_connecting_ready(
    driver: &impl DomainPortDriver,
) {
    assert_eq!(driver.snapshot().lifecycle, DomainLifecycle::Unavailable);
    let subscriber = driver.subscribe();
    driver.begin_start();
    assert_eq!(driver.snapshot().lifecycle, DomainLifecycle::Connecting);
    assert_eq!(subscriber.latest().lifecycle, DomainLifecycle::Connecting);
    driver.mark_ready();
    let snap = driver.snapshot();
    assert_eq!(snap.lifecycle, DomainLifecycle::Ready);
    assert_eq!(subscriber.latest().lifecycle, DomainLifecycle::Ready);
    assert_eq!(snap.version, DomainVersion::new(1, 0));
}

pub fn scenario_03_reconnect_retains_safe_payload_and_records_last_error(
    driver: &impl DomainPortDriver,
) {
    start_ready(driver);
    let custom_payload = TestPayload {
        volume: 85,
        is_muted: true,
        device_name: "Headset".to_string(),
    };
    driver.publish_update(1, custom_payload.clone()).unwrap();

    driver.report_owner_failure("connection reset by peer".to_string());
    let snap = driver.snapshot();
    assert_eq!(snap.lifecycle, DomainLifecycle::Reconnecting);
    assert_eq!(snap.payload, custom_payload);
    assert_eq!(
        snap.last_error,
        Some("connection reset by peer".to_string())
    );
}

pub fn scenario_04_strictly_newer_revision_in_same_generation_is_accepted(
    driver: &impl DomainPortDriver,
) {
    start_ready(driver);
    let p1 = TestPayload {
        volume: 60,
        ..Default::default()
    };
    let p2 = TestPayload {
        volume: 70,
        ..Default::default()
    };

    assert!(driver.publish_update(1, p1.clone()).is_ok());
    assert_eq!(driver.snapshot().version, DomainVersion::new(1, 1));
    assert_eq!(driver.snapshot().payload, p1);

    assert!(driver.publish_update(2, p2.clone()).is_ok());
    assert_eq!(driver.snapshot().version, DomainVersion::new(1, 2));
    assert_eq!(driver.snapshot().payload, p2);
}

pub fn scenario_05_stale_generation_is_rejected(driver: &impl DomainPortDriver) {
    start_ready(driver);
    driver.report_owner_failure("reconnect needed".to_string());
    driver.advance_clock_ms(300); // trigger backoff restart -> generation 2
    driver.mark_ready();

    assert_eq!(driver.snapshot().version.owner_generation, 2);

    let stale_payload = TestPayload {
        volume: 10,
        ..Default::default()
    };
    let res = driver.publish_raw_update(
        DomainVersion::new(1, 99),
        DomainLifecycle::Ready,
        stale_payload,
        None,
    );
    assert!(matches!(res, Err(StaleUpdateError::StaleVersion { .. })));

    let future = driver.publish_raw_update(
        DomainVersion::new(3, 0),
        DomainLifecycle::Ready,
        TestPayload::default(),
        None,
    );
    assert!(matches!(
        future,
        Err(StaleUpdateError::UninstalledGeneration {
            installed: 2,
            attempted: 3
        })
    ));
    assert_eq!(driver.telemetry().stale_updates, 2);
}

pub fn scenario_06_stale_revision_is_rejected(driver: &impl DomainPortDriver) {
    start_ready(driver);
    let p5 = TestPayload {
        volume: 50,
        ..Default::default()
    };
    driver.publish_update(5, p5).unwrap();

    let p3 = TestPayload {
        volume: 30,
        ..Default::default()
    };
    let res = driver.publish_update(3, p3);
    assert!(matches!(res, Err(StaleUpdateError::StaleVersion { .. })));
    assert_eq!(driver.telemetry().stale_updates, 1);
    assert_eq!(driver.snapshot().version, DomainVersion::new(1, 5));
}

pub fn scenario_07_conflicting_payload_at_same_version_is_rejected_and_diagnosed(
    driver: &impl DomainPortDriver,
) {
    start_ready(driver);
    let p_a = TestPayload {
        volume: 40,
        ..Default::default()
    };
    driver.publish_update(5, p_a.clone()).unwrap();

    let p_b = TestPayload {
        volume: 90,
        ..Default::default()
    };
    let res =
        driver.publish_raw_update(DomainVersion::new(1, 5), DomainLifecycle::Ready, p_b, None);
    assert!(matches!(
        res,
        Err(StaleUpdateError::ConflictingSnapshot { .. })
    ));
    assert_eq!(driver.telemetry().stale_updates, 1);
    assert_eq!(driver.snapshot().payload, p_a);
}

pub fn scenario_08_new_owner_generation_permits_revision_reset(driver: &impl DomainPortDriver) {
    start_ready(driver);
    let p1 = TestPayload {
        volume: 99,
        ..Default::default()
    };
    driver.publish_update(100, p1).unwrap();
    assert_eq!(driver.snapshot().version, DomainVersion::new(1, 100));

    driver.report_owner_failure("restarting".to_string());
    driver.advance_clock_ms(300); // generation 2 starts at (2, 0)
    driver.mark_ready();

    let p2 = TestPayload {
        volume: 20,
        ..Default::default()
    };
    let res = driver.publish_update(1, p2.clone());
    assert!(res.is_ok());
    assert_eq!(driver.snapshot().version, DomainVersion::new(2, 1));
    assert_eq!(driver.snapshot().payload, p2);
}

pub fn scenario_09_slow_subscriber_converges_to_latest_atomic_snapshot(
    driver: &impl DomainPortDriver,
) {
    start_ready(driver);
    let subscriber = driver.subscribe();
    for r in 1..=5 {
        let p = TestPayload {
            volume: r as u8 * 10,
            ..Default::default()
        };
        driver.publish_update(r, p).unwrap();
    }
    // A latest-value subscriber does not consume intermediate updates, but
    // still converges atomically to the newest snapshot.
    let snap = subscriber.latest();
    assert_eq!(snap.version, DomainVersion::new(1, 5));
    assert_eq!(snap.payload.volume, 50);
}

pub fn scenario_10_accepted_command_receives_exactly_one_terminal_outcome(
    driver: &impl DomainPortDriver,
) {
    start_ready(driver);
    let cmd = TestCommand {
        id: CommandId("cmd-10".to_string()),
        action: TestAction::SetVolume(75),
        policy: MailboxPolicy::Lossless,
    };
    let ticket = driver.submit_command(cmd).unwrap();
    assert_eq!(ticket.outcome(), None);

    driver.process_pending_commands_and_converge();
    let outcome = ticket.outcome();
    assert!(matches!(outcome, Some(CommandOutcome::Applied { .. })));
    assert_eq!(ticket.completion_attempts(), 1);

    // Processing again cannot emit another terminal completion.
    driver.process_pending_commands_and_converge();
    assert_eq!(ticket.outcome(), outcome);
    assert_eq!(ticket.completion_attempts(), 1);
}

pub fn scenario_11_backend_acknowledgement_alone_does_not_complete_convergence_command(
    driver: &impl DomainPortDriver,
) {
    start_ready(driver);
    let cmd = TestCommand {
        id: CommandId("cmd-11".to_string()),
        action: TestAction::SetVolume(33),
        policy: MailboxPolicy::Lossless,
    };
    let ticket = driver.submit_command(cmd).unwrap();

    // Backend ACK occurs
    let acked_id = driver.ack_command_without_snapshot();
    assert_eq!(acked_id, Some(CommandId("cmd-11".to_string())));
    // Command is NOT complete yet
    assert_eq!(ticket.outcome(), None);

    // State convergence snapshot update arrives
    driver.process_pending_commands_and_converge();
    assert!(matches!(
        ticket.outcome(),
        Some(CommandOutcome::Applied { .. })
    ));
}

pub fn scenario_12_lossless_mailbox_rejects_overflow_without_dropping_accepted_commands(
    driver: &impl DomainPortDriver,
) {
    start_ready(driver);
    let c1 = TestCommand {
        id: CommandId("c1".to_string()),
        action: TestAction::SetVolume(10),
        policy: MailboxPolicy::Lossless,
    };
    let c2 = TestCommand {
        id: CommandId("c2".to_string()),
        action: TestAction::SetVolume(20),
        policy: MailboxPolicy::Lossless,
    };
    let c3 = TestCommand {
        id: CommandId("c3".to_string()),
        action: TestAction::SetVolume(30),
        policy: MailboxPolicy::Lossless,
    };

    let t1 = driver.submit_command(c1).unwrap();
    let t2 = driver.submit_command(c2).unwrap();
    let overflow_res = driver.submit_command(c3);

    assert_eq!(
        overflow_res.unwrap_err(),
        CommandOutcome::Rejected {
            reason: RejectionReason::Overloaded
        }
    );
    assert_eq!(driver.telemetry().overloads, 1);

    // Accepted commands t1 and t2 remain active and pending in queue
    assert!(!t1.is_completed());
    assert!(!t2.is_completed());
    driver.process_pending_commands_and_converge();
    assert!(t1.is_completed());
    assert!(t2.is_completed());
}

pub fn scenario_13_replace_latest_supersedes_pending_command_with_same_key_and_emits_terminal_cancellation(
    driver: &impl DomainPortDriver,
) {
    start_ready(driver);
    let c1 = TestCommand {
        id: CommandId("c1".to_string()),
        action: TestAction::SetVolume(10),
        policy: MailboxPolicy::ReplaceLatest {
            key: "volume".to_string(),
        },
    };
    let c2 = TestCommand {
        id: CommandId("c2".to_string()),
        action: TestAction::SetVolume(20),
        policy: MailboxPolicy::ReplaceLatest {
            key: "volume".to_string(),
        },
    };

    let t1 = driver.submit_command(c1).unwrap();
    let t2 = driver.submit_command(c2).unwrap();

    assert_eq!(
        t1.outcome(),
        Some(CommandOutcome::Cancelled {
            reason: CancellationReason::Superseded
        })
    );
    assert_eq!(driver.telemetry().supersessions, 1);
    assert!(!t2.is_completed());

    driver.process_pending_commands_and_converge();
    assert!(matches!(t2.outcome(), Some(CommandOutcome::Applied { .. })));
}

pub fn scenario_14_different_replace_latest_keys_do_not_replace_each_other(
    driver: &impl DomainPortDriver,
) {
    start_ready(driver);
    let c1 = TestCommand {
        id: CommandId("c1".to_string()),
        action: TestAction::SetVolume(10),
        policy: MailboxPolicy::ReplaceLatest {
            key: "vol".to_string(),
        },
    };
    let c2 = TestCommand {
        id: CommandId("c2".to_string()),
        action: TestAction::SetMuted(true),
        policy: MailboxPolicy::ReplaceLatest {
            key: "mute".to_string(),
        },
    };

    let t1 = driver.submit_command(c1).unwrap();
    let t2 = driver.submit_command(c2).unwrap();

    assert!(!t1.is_completed());
    assert!(!t2.is_completed());
    assert_eq!(driver.telemetry().supersessions, 0);

    driver.process_pending_commands_and_converge();
    assert!(matches!(t1.outcome(), Some(CommandOutcome::Applied { .. })));
    assert!(matches!(t2.outcome(), Some(CommandOutcome::Applied { .. })));
}

pub fn scenario_15_owner_replacement_cancels_old_generation_pending_in_flight_commands(
    driver: &impl DomainPortDriver,
) {
    start_ready(driver); // Generation 1
    let in_flight = TestCommand {
        id: CommandId("in-flight".to_string()),
        action: TestAction::SetVolume(10),
        policy: MailboxPolicy::Lossless,
    };
    let pending = TestCommand {
        id: CommandId("pending".to_string()),
        action: TestAction::SetMuted(true),
        policy: MailboxPolicy::Lossless,
    };
    let in_flight_ticket = driver.submit_command(in_flight).unwrap();
    assert_eq!(
        driver.ack_command_without_snapshot(),
        Some(CommandId("in-flight".to_string()))
    );
    let pending_ticket = driver.submit_command(pending).unwrap();

    // Owner fails and restarts into generation 2
    driver.report_owner_failure("crashed".to_string());
    driver.advance_clock_ms(300);

    for ticket in [in_flight_ticket, pending_ticket] {
        assert_eq!(
            ticket.outcome(),
            Some(CommandOutcome::Cancelled {
                reason: CancellationReason::OwnerReplaced
            })
        );
        assert_eq!(ticket.completion_attempts(), 1);
    }
    assert_eq!(driver.snapshot().version.owner_generation, 2);
}

pub fn scenario_16_backoff_is_exponential_from_250_ms_and_capped_at_30_seconds(
    driver: &impl DomainPortDriver,
) {
    assert_eq!(driver.backoff_delay_ms(1), 250);
    assert_eq!(driver.backoff_delay_ms(2), 500);
    assert_eq!(driver.backoff_delay_ms(4), 2_000);
    assert_eq!(driver.backoff_delay_ms(7), 16_000);
    assert_eq!(driver.backoff_delay_ms(8), 30_000);
    assert_eq!(driver.backoff_delay_ms(32), 30_000);
}

pub fn scenario_17_five_failures_inside_60_seconds_enter_quarantine(
    driver: &impl DomainPortDriver,
) {
    start_ready(driver);

    // Four owners fail and restart inside the rolling window.
    for i in 1..=4 {
        driver.report_owner_failure(format!("f{i}"));
        driver.advance_clock_ms(driver.backoff_delay_ms(i));
        driver.mark_ready();
        assert_eq!(driver.supervisor_state(), SupervisorState::Running);
    }
    // The fifth running owner failure triggers quarantine.
    driver.report_owner_failure("f5".to_string());
    assert_eq!(driver.supervisor_state(), SupervisorState::Quarantined);
    assert_eq!(driver.snapshot().lifecycle, DomainLifecycle::Unavailable);
}

pub fn scenario_18_five_minutes_stable_clears_rolling_failure_window_but_preserves_session_restart_telemetry(
    driver: &impl DomainPortDriver,
) {
    start_ready(driver);

    // 3 failures
    for i in 1..=3 {
        driver.report_owner_failure(format!("f{}", i));
        driver.advance_clock_ms(1000);
        driver.mark_ready();
    }
    let total_restarts_before = driver.telemetry().restarts;
    assert_eq!(total_restarts_before, 3);

    // 5 minutes stable
    driver.advance_clock_secs(300);

    // Next failure should be attempt 1 (250ms backoff) because rolling window was cleared
    driver.report_owner_failure("f4_after_stable".to_string());
    let now = driver.clock().now_ms();
    assert_eq!(
        driver.supervisor_state(),
        SupervisorState::Backoff {
            attempt: 1,
            retry_at_ms: now + 250
        }
    );
    assert_eq!(driver.telemetry().restarts, 3); // Restarts preserved until next retry completes
}

pub fn scenario_19_quarantine_requires_explicit_reset_or_containing_process_restart(
    driver: &impl DomainPortDriver,
) {
    start_ready(driver);
    for i in 1..=4 {
        driver.report_owner_failure(format!("f{i}"));
        driver.advance_clock_ms(driver.backoff_delay_ms(i));
        driver.mark_ready();
    }
    driver.report_owner_failure("f5".to_string());
    assert_eq!(driver.supervisor_state(), SupervisorState::Quarantined);

    // Advance clock heavily -> remains quarantined
    driver.advance_clock_secs(600);
    assert_eq!(driver.supervisor_state(), SupervisorState::Quarantined);

    // Explicit reset
    driver.reset_quarantine();
    assert_eq!(driver.supervisor_state(), SupervisorState::Starting);
    assert_eq!(driver.snapshot().lifecycle, DomainLifecycle::Reconnecting);

    driver.begin_start();
    driver.mark_ready();
    assert_eq!(driver.supervisor_state(), SupervisorState::Running);
    assert_eq!(driver.snapshot().lifecycle, DomainLifecycle::Ready);

    // A containing-process restart starts a fresh session and also clears
    // quarantine without retaining session telemetry or generation.
    for i in 1..=4 {
        driver.report_owner_failure(format!("second-session-f{i}"));
        driver.advance_clock_ms(driver.backoff_delay_ms(i));
        driver.mark_ready();
    }
    driver.report_owner_failure("second-session-f5".to_string());
    assert_eq!(driver.supervisor_state(), SupervisorState::Quarantined);
    driver.restart_containing_process();
    assert_eq!(driver.supervisor_state(), SupervisorState::Starting);
    assert_eq!(driver.snapshot().lifecycle, DomainLifecycle::Unavailable);
    assert_eq!(driver.snapshot().version, DomainVersion::ZERO);
    assert_eq!(driver.telemetry().restarts, 0);
}

pub fn scenario_20_telemetry_reports_generation_queue_depth_capacity_overloads_supersessions_restarts_stale_updates_and_last_error(
    driver: &impl DomainPortDriver,
) {
    start_ready(driver); // gen 1

    // 1. Submit 2 commands, overload 1
    let c1 = TestCommand {
        id: CommandId("c1".to_string()),
        action: TestAction::SetVolume(10),
        policy: MailboxPolicy::Lossless,
    };
    let c2 = TestCommand {
        id: CommandId("c2".to_string()),
        action: TestAction::SetVolume(20),
        policy: MailboxPolicy::Lossless,
    };
    let c3 = TestCommand {
        id: CommandId("c3".to_string()),
        action: TestAction::SetVolume(30),
        policy: MailboxPolicy::Lossless,
    };
    let _ = driver.submit_command(c1);
    let _ = driver.submit_command(c2);
    let _ = driver.submit_command(c3); // overload
    let queued = driver.telemetry();
    assert_eq!(queued.current_queue_depth, 2);
    assert_eq!(queued.queue_capacity, 2);

    // 2. Supersede
    let c_rep1 = TestCommand {
        id: CommandId("c_rep1".to_string()),
        action: TestAction::SetMuted(true),
        policy: MailboxPolicy::ReplaceLatest {
            key: "mute".to_string(),
        },
    };
    let c_rep2 = TestCommand {
        id: CommandId("c_rep2".to_string()),
        action: TestAction::SetMuted(false),
        policy: MailboxPolicy::ReplaceLatest {
            key: "mute".to_string(),
        },
    };
    // c1 and c2 are in queue, queue length 2 (capacity 2). Replace c_rep1 with c_rep2 doesn't work if queue is full unless replacing existing mute key.
    // Let's clear c1 and c2 first to demonstrate replacement
    driver.process_pending_commands_and_converge();

    let _ = driver.submit_command(c_rep1);
    let _ = driver.submit_command(c_rep2); // supersedes c_rep1

    // 3. Stale update
    let _ = driver.publish_update(0, TestPayload::default());

    // 4. Restart
    driver.report_owner_failure("test error".to_string());
    driver.advance_clock_ms(300);

    let telem = driver.telemetry();
    assert_eq!(telem.owner_generation, 2);
    assert_eq!(telem.overloads, 1);
    assert_eq!(telem.supersessions, 1);
    assert_eq!(telem.restarts, 1);
    assert_eq!(telem.stale_updates, 1);
    assert_eq!(telem.last_error, Some("test error".to_string()));
    assert_eq!(telem.current_queue_depth, 0);
    assert_eq!(telem.queue_capacity, 2);
}

pub fn scenario_21_reconciled_and_timed_out_commands_have_typed_terminal_outcomes(
    driver: &impl DomainPortDriver,
) {
    start_ready(driver);
    let reconciled = driver
        .submit_command(TestCommand {
            id: CommandId("reconciled".to_string()),
            action: TestAction::SetVolume(88),
            policy: MailboxPolicy::Lossless,
        })
        .unwrap();
    driver.reconcile_front_command();
    assert!(matches!(
        reconciled.outcome(),
        Some(CommandOutcome::ReconciledApplied { .. })
    ));
    assert_eq!(reconciled.completion_attempts(), 1);

    let timed_out = driver
        .submit_command(TestCommand {
            id: CommandId("timed-out".to_string()),
            action: TestAction::SetMuted(true),
            policy: MailboxPolicy::Lossless,
        })
        .unwrap();
    let expected_version = driver.snapshot().version;
    driver.timeout_front_command();
    assert_eq!(
        timed_out.outcome(),
        Some(CommandOutcome::TimedOut {
            last_observed_version: expected_version
        })
    );
    assert_eq!(timed_out.completion_attempts(), 1);
}
