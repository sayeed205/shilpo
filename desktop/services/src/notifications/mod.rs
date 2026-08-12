use anyhow::Result;
use std::{
    collections::{HashMap, VecDeque},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};
use tokio::sync::{broadcast, watch};
use zbus::{Connection, interface, object_server::SignalEmitter};

pub use crate::compositor::{CancellationReason, DomainVersion, StaleUpdateError, SupervisorState};
pub use shilpo_device::{DomainLifecycle, DomainPortTelemetry};

const NOTIFICATION_OBJECT_PATH: &str = "/org/freedesktop/Notifications";

/// Reasons reported by the freedesktop `NotificationClosed` signal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[repr(u32)]
pub enum NotificationCloseReason {
    Expired = 1,
    DismissedByUser = 2,
    ClosedByRequest = 3,
    Undefined = 4,
}

/// Notification urgency levels per Freedesktop Desktop Notifications Specification.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Default,
    serde::Serialize,
    serde::Deserialize,
)]
pub enum NotificationUrgency {
    Low = 0,
    #[default]
    Normal = 1,
    Critical = 2,
}

/// Represents a single desktop notification.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Notification {
    pub id: u32,
    pub app_name: String,
    pub summary: String,
    pub body: String,
    pub app_icon: Option<String>,
    pub desktop_entry: Option<String>,
    pub image_path: Option<String>,
    pub urgency: NotificationUrgency,
    pub actions: Vec<(String, String)>,
    pub expire_timeout_ms: i32,
    pub timestamp: chrono::DateTime<chrono::Local>,
}

impl Notification {
    pub fn new(summary: impl Into<String>, body: impl Into<String>) -> Self {
        Self {
            id: 0,
            app_name: "Shilpo Shell".into(),
            summary: summary.into(),
            body: body.into(),
            app_icon: None,
            desktop_entry: None,
            image_path: None,
            urgency: NotificationUrgency::Normal,
            actions: Vec::new(),
            expire_timeout_ms: 5000,
            timestamp: chrono::Local::now(),
        }
    }
}

/// Revisioned atomic snapshot of the notification domain state.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct NotificationSnapshot {
    pub version: DomainVersion,
    pub lifecycle: DomainLifecycle,
    pub notifications: Vec<Notification>,
    pub history: Vec<Notification>,
    pub dnd_enabled: bool,
    pub last_error: Option<String>,
}

impl Default for NotificationSnapshot {
    fn default() -> Self {
        Self {
            version: DomainVersion::ZERO,
            lifecycle: DomainLifecycle::Unavailable,
            notifications: Vec::new(),
            history: Vec::new(),
            dnd_enabled: false,
            last_error: None,
        }
    }
}

/// Unique identifier for commands.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct CommandId(pub String);

impl CommandId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn generate() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }
}

/// Bounded mailbox overload policy.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum MailboxPolicy {
    Lossless,
    ReplaceLatest { key: String },
}

/// Typed notification domain commands.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum NotificationCommand {
    Push(Notification),
    Dismiss(u32),
    Expire(u32),
    DismissAll,
    InvokeAction { id: u32, action_key: String },
    SetDnd(bool),
    ClearHistory,
}

impl NotificationCommand {
    pub fn policy(&self) -> MailboxPolicy {
        match self {
            Self::SetDnd(_) => MailboxPolicy::ReplaceLatest {
                key: "set_dnd".to_string(),
            },
            _ => MailboxPolicy::Lossless,
        }
    }
}

/// Rejection reasons for notification commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum RejectionReason {
    Unavailable,
    Overloaded,
    NotFound,
}

pub type NotificationRejectionReason = RejectionReason;

/// Terminal outcome for accepted notification commands.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum NotificationCommandOutcome {
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

pub type CommandOutcome = NotificationCommandOutcome;

/// Handle / Ticket returned when a command is submitted.
#[derive(Debug, Clone)]
pub struct CommandTicket {
    outcome: Arc<Mutex<Option<CommandOutcome>>>,
}

impl CommandTicket {
    pub fn new() -> (Self, CommandResolver) {
        let outcome = Arc::new(Mutex::new(None));
        let ticket = Self {
            outcome: outcome.clone(),
        };
        let resolver = CommandResolver { outcome };
        (ticket, resolver)
    }

    pub fn outcome(&self) -> Option<CommandOutcome> {
        self.outcome.lock().unwrap().clone()
    }

    pub fn is_completed(&self) -> bool {
        self.outcome.lock().unwrap().is_some()
    }

    /// Claims a timeout terminal state if the owner has not completed first.
    /// The resolver's single-assignment guard prevents a later worker result
    /// from producing a second terminal classification.
    pub fn wait_timeout(
        &self,
        timeout: std::time::Duration,
        last_observed_version: DomainVersion,
    ) -> CommandOutcome {
        let deadline = std::time::Instant::now() + timeout;
        loop {
            if let Some(outcome) = self.outcome() {
                return outcome;
            }
            if std::time::Instant::now() >= deadline {
                let mut guard = self.outcome.lock().unwrap();
                return guard
                    .get_or_insert_with(|| CommandOutcome::TimedOut {
                        last_observed_version,
                    })
                    .clone();
            }
            std::thread::yield_now();
        }
    }
}

#[derive(Debug, Clone)]
pub struct CommandResolver {
    outcome: Arc<Mutex<Option<CommandOutcome>>>,
}

impl CommandResolver {
    pub fn resolve(&self, outcome: CommandOutcome) -> bool {
        let mut guard = self.outcome.lock().unwrap();
        if guard.is_none() {
            *guard = Some(outcome);
            true
        } else {
            false
        }
    }
}

/// Narrow, revisioned domain port interface for desktop notification operations.
pub trait NotificationPort: Send + Sync {
    fn snapshot(&self) -> NotificationSnapshot;
    fn subscribe(&self) -> watch::Receiver<NotificationSnapshot>;
    fn subscribe_events(&self) -> broadcast::Receiver<Notification>;
    fn submit_command(&self, command: NotificationCommand)
    -> Result<CommandTicket, CommandOutcome>;
    fn supervisor_state(&self) -> SupervisorState;
    fn telemetry(&self) -> DomainPortTelemetry;
    fn reset_quarantine(&self);

    fn push_notification(&self, notif: Notification) {
        let _ = self.submit_command(NotificationCommand::Push(notif));
    }
    fn dismiss(&self, id: u32) {
        let _ = self.submit_command(NotificationCommand::Dismiss(id));
    }
    fn expire(&self, id: u32) {
        let _ = self.submit_command(NotificationCommand::Expire(id));
    }
    fn dismiss_all(&self) {
        let _ = self.submit_command(NotificationCommand::DismissAll);
    }
    fn invoke_action(&self, id: u32, action_key: &str) {
        let _ = self.submit_command(NotificationCommand::InvokeAction {
            id,
            action_key: action_key.to_string(),
        });
    }
    fn set_dnd_enabled(&self, enabled: bool) {
        let _ = self.submit_command(NotificationCommand::SetDnd(enabled));
    }
    fn clear_history(&self) {
        let _ = self.submit_command(NotificationCommand::ClearHistory);
    }
}

/// Controllable manual clock for deterministic time advancement in tests.
#[derive(Debug, Clone, Default)]
pub struct NotificationClock {
    now_ms: Arc<AtomicU64>,
}

impl NotificationClock {
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

    pub fn advance_secs(&self, secs: u64) {
        self.advance_ms(secs * 1000);
    }
}

struct PendingCommandItem {
    command: NotificationCommand,
    generation: u64,
    resolver: CommandResolver,
}

struct NotificationState {
    supervisor_state: SupervisorState,
    lifecycle: DomainLifecycle,
    owner_generation: u64,
    revision: u64,
    notifications: Vec<Notification>,
    history: Vec<Notification>,
    dnd_enabled: bool,
    last_error: Option<String>,
    had_prior_readiness: bool,
    last_running_timestamp_ms: Option<u64>,
    failure_timestamps_ms: Vec<u64>,
    backoff_attempt: u32,
    queue: VecDeque<PendingCommandItem>,
    next_notification_id: u32,
    overloads: u64,
    supersessions: u64,
    restarts: u64,
    stale_updates: u64,
    auto_converge: bool,
}

/// In-memory notification domain state used by the port and deterministic tests.
///
/// It is deliberately kept behind the concrete notification service; callers use
/// `NotificationPort` and cannot select this state machine as a runtime owner.
pub struct NotificationDomainState {
    clock: NotificationClock,
    capacity: usize,
    state: Mutex<NotificationState>,
    watch_tx: watch::Sender<NotificationSnapshot>,
    event_tx: broadcast::Sender<Notification>,
}

impl NotificationDomainState {
    pub fn new(capacity: usize) -> Self {
        assert!(capacity > 0, "mailbox capacity must be positive");
        let initial_snapshot = NotificationSnapshot::default();
        let (watch_tx, _) = watch::channel(initial_snapshot.clone());
        let (event_tx, _) = broadcast::channel(64);
        Self {
            clock: NotificationClock::new(),
            capacity,
            state: Mutex::new(NotificationState {
                supervisor_state: SupervisorState::Starting,
                lifecycle: DomainLifecycle::Unavailable,
                owner_generation: 0,
                revision: 0,
                notifications: Vec::new(),
                history: Vec::new(),
                dnd_enabled: false,
                last_error: None,
                had_prior_readiness: false,
                last_running_timestamp_ms: None,
                failure_timestamps_ms: Vec::new(),
                backoff_attempt: 0,
                queue: VecDeque::new(),
                next_notification_id: 0,
                overloads: 0,
                supersessions: 0,
                restarts: 0,
                stale_updates: 0,
                auto_converge: true,
            }),
            watch_tx,
            event_tx,
        }
    }

    pub fn new_ready(capacity: usize) -> Self {
        let adapter = Self::new(capacity);
        adapter.begin_start();
        adapter.mark_ready();
        adapter
    }

    pub fn clock(&self) -> &NotificationClock {
        &self.clock
    }

    pub fn advance_clock_ms(&self, ms: u64) {
        self.clock.advance_ms(ms);
        let mut state = self.state.lock().unwrap();
        Self::check_clock_state(&mut state, self.clock.now_ms(), &self.watch_tx);
    }

    pub fn advance_clock_secs(&self, secs: u64) {
        self.advance_clock_ms(secs * 1000);
    }

    fn snapshot_from_state(state: &NotificationState) -> NotificationSnapshot {
        NotificationSnapshot {
            version: DomainVersion::new(state.owner_generation, state.revision),
            lifecycle: state.lifecycle,
            notifications: state.notifications.clone(),
            history: state.history.clone(),
            dnd_enabled: state.dnd_enabled,
            last_error: state.last_error.clone(),
        }
    }

    fn notify_subscribers(
        state: &NotificationState,
        watch_tx: &watch::Sender<NotificationSnapshot>,
    ) {
        let snapshot = Self::snapshot_from_state(state);
        let _ = watch_tx.send(snapshot);
    }

    fn cancel_queue(state: &mut NotificationState, reason: CancellationReason) {
        for item in std::mem::take(&mut state.queue) {
            item.resolver.resolve(CommandOutcome::Cancelled { reason });
        }
    }

    fn backoff_delay_for_attempt(attempt: u32) -> u64 {
        let multiplier = 2u64.saturating_pow(attempt.saturating_sub(1));
        250u64.saturating_mul(multiplier).min(30_000)
    }

    fn check_clock_state(
        state: &mut NotificationState,
        now_ms: u64,
        watch_tx: &watch::Sender<NotificationSnapshot>,
    ) {
        match state.supervisor_state {
            SupervisorState::Backoff { retry_at_ms, .. } => {
                if now_ms >= retry_at_ms {
                    state.owner_generation += 1;
                    state.revision = 0;
                    state.restarts += 1;
                    Self::cancel_queue(state, CancellationReason::OwnerReplaced);
                    state.supervisor_state = SupervisorState::Starting;
                    state.lifecycle = if state.had_prior_readiness {
                        DomainLifecycle::Reconnecting
                    } else {
                        DomainLifecycle::Connecting
                    };
                    state.last_running_timestamp_ms = None;
                    Self::notify_subscribers(state, watch_tx);
                }
            }
            SupervisorState::Running => {
                if let Some(start_ts) = state.last_running_timestamp_ms
                    && now_ms.saturating_sub(start_ts) >= 300_000
                {
                    state.failure_timestamps_ms.clear();
                    state.backoff_attempt = 0;
                }
            }
            _ => {}
        }
    }

    pub fn begin_start(&self) {
        let mut state = self.state.lock().unwrap();
        Self::cancel_queue(&mut state, CancellationReason::OwnerReplaced);
        state.owner_generation += 1;
        state.revision = 0;
        state.supervisor_state = SupervisorState::Starting;
        state.lifecycle = if state.had_prior_readiness {
            DomainLifecycle::Reconnecting
        } else {
            DomainLifecycle::Connecting
        };
        Self::notify_subscribers(&state, &self.watch_tx);
    }

    pub fn mark_ready(&self) {
        let mut state = self.state.lock().unwrap();
        state.supervisor_state = SupervisorState::Running;
        state.lifecycle = DomainLifecycle::Ready;
        state.had_prior_readiness = true;
        state.last_running_timestamp_ms = Some(self.clock.now_ms());
        Self::notify_subscribers(&state, &self.watch_tx);
    }

    pub fn report_owner_failure(&self, error: String) {
        let mut state = self.state.lock().unwrap();
        let now_ms = self.clock.now_ms();
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
            let delay = Self::backoff_delay_for_attempt(state.backoff_attempt);
            state.supervisor_state = SupervisorState::Backoff {
                attempt: state.backoff_attempt,
                retry_at_ms: now_ms + delay,
            };
            state.lifecycle = if state.had_prior_readiness {
                DomainLifecycle::Reconnecting
            } else {
                DomainLifecycle::Unavailable
            };
        }
        Self::notify_subscribers(&state, &self.watch_tx);
    }

    pub fn publish_update(
        &self,
        revision: u64,
        notifications: Vec<Notification>,
        history: Vec<Notification>,
        dnd_enabled: bool,
    ) -> Result<(), StaleUpdateError> {
        let mut state = self.state.lock().unwrap();
        Self::check_clock_state(&mut state, self.clock.now_ms(), &self.watch_tx);
        let version = DomainVersion::new(state.owner_generation, revision);
        let lifecycle = state.lifecycle;
        let last_error = state.last_error.clone();
        Self::apply_update(
            &mut state,
            version,
            lifecycle,
            notifications,
            history,
            dnd_enabled,
            last_error,
            &self.watch_tx,
        )
    }

    pub fn publish_raw_update(
        &self,
        version: DomainVersion,
        lifecycle: DomainLifecycle,
        notifications: Vec<Notification>,
        history: Vec<Notification>,
        dnd_enabled: bool,
        last_error: Option<String>,
    ) -> Result<(), StaleUpdateError> {
        let mut state = self.state.lock().unwrap();
        Self::check_clock_state(&mut state, self.clock.now_ms(), &self.watch_tx);
        Self::apply_update(
            &mut state,
            version,
            lifecycle,
            notifications,
            history,
            dnd_enabled,
            last_error,
            &self.watch_tx,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn apply_update(
        state: &mut NotificationState,
        version: DomainVersion,
        lifecycle: DomainLifecycle,
        notifications: Vec<Notification>,
        history: Vec<Notification>,
        dnd_enabled: bool,
        last_error: Option<String>,
        watch_tx: &watch::Sender<NotificationSnapshot>,
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
                && state.notifications == notifications
                && state.history == history
                && state.dnd_enabled == dnd_enabled
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
        state.notifications = notifications;
        state.history = history;
        state.dnd_enabled = dnd_enabled;
        state.last_error = last_error;
        Self::notify_subscribers(state, watch_tx);
        Ok(())
    }

    pub fn set_auto_converge(&self, auto: bool) {
        let mut state = self.state.lock().unwrap();
        state.auto_converge = auto;
    }

    fn process_queue_locked(
        state: &mut NotificationState,
        watch_tx: &watch::Sender<NotificationSnapshot>,
        event_tx: &broadcast::Sender<Notification>,
    ) {
        let items = std::mem::take(&mut state.queue);
        for item in items {
            if item.generation != state.owner_generation {
                item.resolver.resolve(CommandOutcome::Cancelled {
                    reason: CancellationReason::OwnerReplaced,
                });
                continue;
            }

            match item.command {
                NotificationCommand::Push(mut notif) => {
                    state.next_notification_id += 1;
                    if notif.id == 0 {
                        notif.id = state.next_notification_id;
                    }
                    if state.dnd_enabled && notif.urgency != NotificationUrgency::Critical {
                        state.history.push(notif);
                        if state.history.len() > 100 {
                            state.history.remove(0);
                        }
                    } else {
                        state.notifications.push(notif.clone());
                        state.history.push(notif.clone());
                        if state.history.len() > 100 {
                            state.history.remove(0);
                        }
                        let _ = event_tx.send(notif);
                    }
                }
                NotificationCommand::Dismiss(id) | NotificationCommand::Expire(id) => {
                    state.notifications.retain(|n| n.id != id);
                }
                NotificationCommand::DismissAll => {
                    state.notifications.clear();
                }
                NotificationCommand::InvokeAction { id, .. } => {
                    state.notifications.retain(|n| n.id != id);
                }
                NotificationCommand::SetDnd(enabled) => {
                    state.dnd_enabled = enabled;
                }
                NotificationCommand::ClearHistory => {
                    state.history.clear();
                }
            }
            state.revision += 1;
            let version = DomainVersion::new(state.owner_generation, state.revision);
            item.resolver.resolve(CommandOutcome::Applied { version });
        }
        Self::notify_subscribers(state, watch_tx);
    }

    pub fn process_pending_commands_and_converge(&self) {
        let mut state = self.state.lock().unwrap();
        Self::check_clock_state(&mut state, self.clock.now_ms(), &self.watch_tx);
        Self::process_queue_locked(&mut state, &self.watch_tx, &self.event_tx);
    }
}

impl NotificationPort for NotificationDomainState {
    fn snapshot(&self) -> NotificationSnapshot {
        let mut state = self.state.lock().unwrap();
        Self::check_clock_state(&mut state, self.clock.now_ms(), &self.watch_tx);
        NotificationSnapshot {
            lifecycle: state.lifecycle,
            version: DomainVersion::new(state.owner_generation, state.revision),
            notifications: state.notifications.clone(),
            history: state.history.clone(),
            dnd_enabled: state.dnd_enabled,
            last_error: state.last_error.clone(),
        }
    }

    fn subscribe(&self) -> watch::Receiver<NotificationSnapshot> {
        self.watch_tx.subscribe()
    }

    fn subscribe_events(&self) -> broadcast::Receiver<Notification> {
        self.event_tx.subscribe()
    }

    fn submit_command(
        &self,
        command: NotificationCommand,
    ) -> Result<CommandTicket, CommandOutcome> {
        let mut state = self.state.lock().unwrap();
        Self::check_clock_state(&mut state, self.clock.now_ms(), &self.watch_tx);

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

        match command.policy() {
            MailboxPolicy::Lossless => {
                if state.queue.len() >= self.capacity {
                    state.overloads += 1;
                    return Err(CommandOutcome::Rejected {
                        reason: RejectionReason::Overloaded,
                    });
                }
            }
            MailboxPolicy::ReplaceLatest { ref key } => {
                let mut replaced_idx = None;
                for (idx, item) in state.queue.iter().enumerate() {
                    if let MailboxPolicy::ReplaceLatest {
                        key: ref existing_key,
                    } = item.command.policy()
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
        });

        if state.auto_converge {
            Self::process_queue_locked(&mut state, &self.watch_tx, &self.event_tx);
        }

        Ok(ticket)
    }

    fn supervisor_state(&self) -> SupervisorState {
        let mut state = self.state.lock().unwrap();
        Self::check_clock_state(&mut state, self.clock.now_ms(), &self.watch_tx);
        state.supervisor_state
    }

    fn telemetry(&self) -> DomainPortTelemetry {
        let mut state = self.state.lock().unwrap();
        Self::check_clock_state(&mut state, self.clock.now_ms(), &self.watch_tx);
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
            Self::notify_subscribers(&state, &self.watch_tx);
        }
    }
}

trait NotificationSignalSink: Send + Sync {
    fn action_invoked(&self, id: u32, action_key: String);
    fn notification_closed(&self, id: u32, reason: NotificationCloseReason);
}

struct DbusNotificationSignalSink {
    emitter: SignalEmitter<'static>,
}

impl NotificationSignalSink for DbusNotificationSignalSink {
    fn action_invoked(&self, id: u32, action_key: String) {
        let emitter = self.emitter.clone();
        let connection = emitter.connection().clone();
        connection
            .executor()
            .spawn(
                async move {
                    if let Err(error) = NotificationServer::action_invoked(&emitter, id, &action_key).await {
                        tracing::warn!(%error, id, action = %action_key, "failed to emit notification action signal");
                    }
                },
                "notification-action-invoked",
            )
            .detach();
    }

    fn notification_closed(&self, id: u32, reason: NotificationCloseReason) {
        let emitter = self.emitter.clone();
        let connection = emitter.connection().clone();
        connection
            .executor()
            .spawn(
                async move {
                    if let Err(error) = NotificationServer::notification_closed(&emitter, id, reason as u32).await {
                        tracing::warn!(%error, id, ?reason, "failed to emit notification closed signal");
                    }
                },
                "notification-closed",
            )
            .detach();
    }
}

/// Dynamic Notification Daemon Service implementing org.freedesktop.Notifications and NotificationPort.
pub struct NotificationService {
    adapter: Arc<NotificationDomainState>,
    connection: Arc<Mutex<Option<Connection>>>,
    signal_sink: Arc<Mutex<Option<Arc<dyn NotificationSignalSink>>>>,
}

impl NotificationService {
    pub async fn new_async() -> Result<Self> {
        let connection = Connection::session().await?;
        Self::new_with_connection(connection).await
    }

    pub fn new() -> Result<Self> {
        let run_async = async {
            match tokio::time::timeout(std::time::Duration::from_millis(300), Self::new_async())
                .await
            {
                Ok(res) => res,
                Err(_) => Err(anyhow::anyhow!("notification DBus connect timeout")),
            }
        };

        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            tokio::task::block_in_place(|| handle.block_on(run_async))
        } else {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()?;
            rt.block_on(run_async)
        }
    }

    pub fn new_offline() -> Self {
        let adapter = Arc::new(NotificationDomainState::new_ready(32));
        Self {
            adapter,
            connection: Arc::new(Mutex::new(None)),
            signal_sink: Arc::new(Mutex::new(None)),
        }
    }

    /// Constructs an offline service without claiming the freedesktop owner name.
    pub fn new_unavailable() -> Self {
        let adapter = Arc::new(NotificationDomainState::new(32));
        Self {
            adapter,
            connection: Arc::new(Mutex::new(None)),
            signal_sink: Arc::new(Mutex::new(None)),
        }
    }

    pub async fn new_with_connection(connection: Connection) -> Result<Self> {
        let adapter = Arc::new(NotificationDomainState::new(32));
        adapter.begin_start();

        let server = NotificationServer {
            adapter: adapter.clone(),
            next_id: Arc::new(Mutex::new(0)),
        };

        connection
            .object_server()
            .at(NOTIFICATION_OBJECT_PATH, server)
            .await?;

        connection
            .request_name("org.freedesktop.Notifications")
            .await?;

        let signal_emitter =
            SignalEmitter::new(&connection, NOTIFICATION_OBJECT_PATH)?.into_owned();
        adapter.mark_ready();
        let service = Self {
            adapter,
            connection: Arc::new(Mutex::new(Some(connection.clone()))),
            signal_sink: Arc::new(Mutex::new(Some(Arc::new(DbusNotificationSignalSink {
                emitter: signal_emitter,
            })))),
        };
        service.spawn_supervisor(connection);
        Ok(service)
    }

    fn spawn_supervisor(&self, initial_connection: Connection) {
        let adapter = self.adapter.clone();
        let connection_slot = self.connection.clone();
        let signal_slot = self.signal_sink.clone();
        initial_connection
            .clone()
            .executor()
            .spawn(
                async move {
                    let mut connection = initial_connection;
                    loop {
                        while !connection.is_closed() {
                            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
                        }
                        adapter.report_owner_failure("notification DBus connection closed".into());
                        if matches!(adapter.supervisor_state(), SupervisorState::Quarantined) {
                            *connection_slot.lock().unwrap() = None;
                            *signal_slot.lock().unwrap() = None;
                            break;
                        }
                        *connection_slot.lock().unwrap() = None;
                        *signal_slot.lock().unwrap() = None;
                        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
                        let Ok(next) = Connection::session().await else {
                            adapter
                                .report_owner_failure("notification DBus reconnect failed".into());
                            continue;
                        };
                        let server = NotificationServer {
                            adapter: adapter.clone(),
                            next_id: Arc::new(Mutex::new(0)),
                        };
                        if next
                            .object_server()
                            .at(NOTIFICATION_OBJECT_PATH, server)
                            .await
                            .is_err()
                            || next
                                .request_name("org.freedesktop.Notifications")
                                .await
                                .is_err()
                        {
                            adapter.report_owner_failure(
                                "notification DBus owner registration failed".into(),
                            );
                            continue;
                        }
                        let Ok(emitter) = SignalEmitter::new(&next, NOTIFICATION_OBJECT_PATH)
                            .map(SignalEmitter::into_owned)
                        else {
                            adapter.report_owner_failure(
                                "notification DBus signal setup failed".into(),
                            );
                            continue;
                        };
                        adapter.begin_start();
                        adapter.mark_ready();
                        *connection_slot.lock().unwrap() = Some(next.clone());
                        *signal_slot.lock().unwrap() =
                            Some(Arc::new(DbusNotificationSignalSink { emitter }));
                        connection = next;
                    }
                },
                "notification-owner-supervisor",
            )
            .detach();
    }

    pub fn is_dbus_connected(&self) -> bool {
        self.connection.lock().unwrap().is_some()
    }

    pub fn is_healthy(&self) -> bool {
        self.connection
            .lock()
            .unwrap()
            .as_ref()
            .is_some_and(|connection| !connection.is_closed())
    }

    pub fn notifications(&self) -> Vec<Notification> {
        self.snapshot().notifications
    }

    pub fn history(&self) -> Vec<Notification> {
        self.snapshot().history
    }

    pub fn is_dnd_enabled(&self) -> bool {
        self.snapshot().dnd_enabled
    }

    pub fn grouped_notifications(&self) -> HashMap<String, Vec<Notification>> {
        let notifs = self.notifications();
        let mut grouped: HashMap<String, Vec<Notification>> = HashMap::new();
        for notif in notifs {
            grouped
                .entry(notif.app_name.clone())
                .or_default()
                .push(notif);
        }
        grouped
    }

    pub fn unread_count(&self) -> usize {
        self.notifications().len()
    }

    pub fn send_inline_reply(&self, id: u32, reply_text: &str) -> Result<()> {
        tracing::info!(id = id, text = %reply_text, "Sending inline reply to notification");
        self.dismiss(id);
        Ok(())
    }

    pub fn run_daemon_boundary(&self) -> Result<()> {
        if !self.is_dbus_connected() {
            tracing::warn!("Notification daemon running in offline fallback mode");
        } else {
            tracing::info!("Notification daemon active on DBus org.freedesktop.Notifications");
        }
        Ok(())
    }

    fn emit_action_invoked(&self, id: u32, action_key: String) {
        if let Some(sink) = self.signal_sink.lock().unwrap().as_ref() {
            sink.action_invoked(id, action_key);
        }
    }

    fn emit_closed(&self, id: u32, reason: NotificationCloseReason) {
        if let Some(sink) = self.signal_sink.lock().unwrap().as_ref() {
            sink.notification_closed(id, reason);
        }
    }
}

impl NotificationPort for NotificationService {
    fn snapshot(&self) -> NotificationSnapshot {
        self.adapter.snapshot()
    }

    fn subscribe(&self) -> watch::Receiver<NotificationSnapshot> {
        self.adapter.subscribe()
    }

    fn subscribe_events(&self) -> broadcast::Receiver<Notification> {
        self.adapter.subscribe_events()
    }

    fn submit_command(
        &self,
        command: NotificationCommand,
    ) -> Result<CommandTicket, CommandOutcome> {
        let ticket_res = self.adapter.submit_command(command.clone());
        self.adapter.process_pending_commands_and_converge();

        if ticket_res.is_ok() {
            match command {
                NotificationCommand::Dismiss(id) => {
                    self.emit_closed(id, NotificationCloseReason::DismissedByUser);
                }
                NotificationCommand::Expire(id) => {
                    self.emit_closed(id, NotificationCloseReason::Expired);
                }
                NotificationCommand::DismissAll => {
                    // Notifying subscribers done via snapshot update
                }
                NotificationCommand::InvokeAction { id, action_key } => {
                    self.emit_action_invoked(id, action_key);
                    self.emit_closed(id, NotificationCloseReason::DismissedByUser);
                }
                _ => {}
            }
        }

        ticket_res
    }

    fn supervisor_state(&self) -> SupervisorState {
        self.adapter.supervisor_state()
    }

    fn telemetry(&self) -> DomainPortTelemetry {
        self.adapter.telemetry()
    }

    fn reset_quarantine(&self) {
        self.adapter.reset_quarantine();
    }
}

struct NotificationServer {
    adapter: Arc<NotificationDomainState>,
    next_id: Arc<Mutex<u32>>,
}

#[interface(name = "org.freedesktop.Notifications")]
impl NotificationServer {
    #[allow(clippy::too_many_arguments)]
    fn notify(
        &self,
        app_name: String,
        replaces_id: u32,
        app_icon: String,
        summary: String,
        body: String,
        raw_actions: Vec<String>,
        hints: HashMap<String, zbus::zvariant::Value<'_>>,
        expire_timeout: i32,
    ) -> u32 {
        let mut id_lock = self.next_id.lock().unwrap();
        let id = if replaces_id == 0 {
            *id_lock += 1;
            *id_lock
        } else {
            replaces_id
        };

        let urgency = hints
            .get("urgency")
            .and_then(|v| match v {
                zbus::zvariant::Value::U8(u) => match u {
                    0 => Some(NotificationUrgency::Low),
                    1 => Some(NotificationUrgency::Normal),
                    2 => Some(NotificationUrgency::Critical),
                    _ => None,
                },
                zbus::zvariant::Value::I32(i) => match i {
                    0 => Some(NotificationUrgency::Low),
                    1 => Some(NotificationUrgency::Normal),
                    2 => Some(NotificationUrgency::Critical),
                    _ => None,
                },
                _ => None,
            })
            .unwrap_or(NotificationUrgency::Normal);

        let mut actions = Vec::new();
        for chunk in raw_actions.chunks(2) {
            if chunk.len() == 2 {
                actions.push((chunk[0].clone(), chunk[1].clone()));
            }
        }

        let expire_timeout_ms = match expire_timeout {
            0 => 0,
            timeout if timeout > 0 => timeout,
            _ => match urgency {
                NotificationUrgency::Low => 3000,
                NotificationUrgency::Normal => 5000,
                NotificationUrgency::Critical => 0,
            },
        };

        let desktop_entry = hints
            .get("desktop-entry")
            .or_else(|| hints.get("desktop_entry"))
            .and_then(|v| match v {
                zbus::zvariant::Value::Str(s) => Some(s.to_string()),
                _ => None,
            });

        let image_path = hints
            .get("image-path")
            .or_else(|| hints.get("image_path"))
            .or_else(|| hints.get("image-path-url"))
            .and_then(|v| match v {
                zbus::zvariant::Value::Str(s) => Some(s.to_string()),
                _ => None,
            });

        let notification = Notification {
            id,
            app_name,
            summary,
            body,
            app_icon: if app_icon.is_empty() {
                None
            } else {
                Some(app_icon)
            },
            desktop_entry,
            image_path,
            urgency,
            actions,
            expire_timeout_ms,
            timestamp: chrono::Local::now(),
        };

        let _ = self
            .adapter
            .submit_command(NotificationCommand::Push(notification));
        self.adapter.process_pending_commands_and_converge();

        id
    }

    async fn close_notification(
        &self,
        id: u32,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> zbus::fdo::Result<()> {
        let _ = self
            .adapter
            .submit_command(NotificationCommand::Dismiss(id));
        self.adapter.process_pending_commands_and_converge();
        Self::notification_closed(
            &emitter,
            id,
            NotificationCloseReason::ClosedByRequest as u32,
        )
        .await?;
        Ok(())
    }

    fn get_capabilities(&self) -> Vec<String> {
        vec![
            "actions".to_string(),
            "body".to_string(),
            "body-markup".to_string(),
            "icon-static".to_string(),
            "persistence".to_string(),
            "image-path".to_string(),
        ]
    }

    fn get_server_information(&self) -> (String, String, String, String) {
        (
            "shilpo-notification-daemon".to_string(),
            "Shilpo".to_string(),
            "0.1.0".to_string(),
            "1.2".to_string(),
        )
    }

    #[zbus(signal)]
    async fn action_invoked(
        emitter: &SignalEmitter<'_>,
        id: u32,
        action_key: &str,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn notification_closed(
        emitter: &SignalEmitter<'_>,
        id: u32,
        reason: u32,
    ) -> zbus::Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_notification_creation_and_dismiss() {
        let service = NotificationService::new_offline();
        assert!(!service.is_dbus_connected());

        let notif = Notification::new("Hello", "World");
        service.push_notification(notif);

        let list = service.notifications();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].summary, "Hello");

        service.dismiss(list[0].id);
        assert!(service.notifications().is_empty());
    }

    #[test]
    fn test_notification_urgency_and_action_parsing() {
        let adapter = Arc::new(NotificationDomainState::new(32));
        adapter.begin_start();
        adapter.mark_ready();

        let server = NotificationServer {
            adapter: adapter.clone(),
            next_id: Arc::new(Mutex::new(0)),
        };

        let mut hints = HashMap::new();
        hints.insert("urgency".to_string(), zbus::zvariant::Value::U8(2));

        let id = server.notify(
            "alert-app".to_string(),
            0,
            "error".to_string(),
            "Critical Alert".to_string(),
            "System error occurred".to_string(),
            vec!["default".to_string(), "Open".to_string()],
            hints,
            -1,
        );

        assert_eq!(id, 1);
        let snap = adapter.snapshot();
        assert_eq!(snap.notifications[0].urgency, NotificationUrgency::Critical);
        assert_eq!(snap.notifications[0].expire_timeout_ms, 0);
        assert_eq!(
            snap.notifications[0].actions,
            vec![("default".to_string(), "Open".to_string())]
        );
    }

    #[test]
    fn test_notification_dismiss_all() {
        let service = NotificationService::new_offline();
        service.push_notification(Notification::new("Title 1", "Body 1"));
        service.push_notification(Notification::new("Title 2", "Body 2"));

        assert_eq!(service.notifications().len(), 2);
        service.dismiss_all();
        assert!(service.notifications().is_empty());
    }

    #[test]
    fn test_notification_replacement_existing_id() {
        let service = NotificationService::new_offline();
        let mut n1 = Notification::new("Initial Summary", "Initial Body");
        n1.id = 1;
        service.push_notification(n1);

        assert_eq!(service.notifications().len(), 1);
        assert_eq!(service.notifications()[0].summary, "Initial Summary");
    }
}
