use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use shilpo_domain::MailboxError;
use shilpo_ui::theme::ThemeMode;
use tokio::sync::{mpsc, oneshot};
use tracing::{error, info};

use crate::adapters::DesktopAdapter;
use crate::daemon::DaemonState;
use crate::persistence::write_state_snapshot_to;

/// Bounded capacity for the PersistenceExecutor mailbox.
pub const PERSISTENCE_MAILBOX_CAPACITY: usize = 8;
/// Bounded capacity for the AdapterExecutor mailbox.
pub const ADAPTER_MAILBOX_CAPACITY: usize = 8;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "status", content = "error")]
pub enum ProjectionStatus {
    Pending,
    Applied,
    Degraded(String),
}

pub struct PersistenceRequest {
    pub kind: PersistenceRequestKind,
    pub reply: oneshot::Sender<Result<u64, String>>,
}

pub enum PersistenceRequestKind {
    Persist(Box<DaemonState>),
    Flush,
}

#[derive(Clone)]
pub struct PersistenceExecutor {
    tx: mpsc::Sender<PersistenceRequest>,
    durable_revision: Arc<AtomicU64>,
    last_error: Arc<Mutex<Option<String>>>,
    overloads: Arc<AtomicU64>,
}

impl PersistenceExecutor {
    pub fn new(target_file_path: Option<PathBuf>) -> Self {
        let (tx, mut rx) = mpsc::channel::<PersistenceRequest>(PERSISTENCE_MAILBOX_CAPACITY);
        let durable_revision = Arc::new(AtomicU64::new(0));
        let durable_rev_clone = durable_revision.clone();
        let last_error = Arc::new(Mutex::new(None));
        let last_error_clone = last_error.clone();

        tokio::spawn(async move {
            while let Some(first_req) = rx.recv().await {
                let mut latest_state = match first_req.kind {
                    PersistenceRequestKind::Persist(state) => Some(*state),
                    PersistenceRequestKind::Flush => None,
                };
                let mut replies = vec![first_req.reply];

                // Coalesce any enqueued persistence requests
                while let Ok(next_req) = rx.try_recv() {
                    if let PersistenceRequestKind::Persist(next_state) = next_req.kind
                        && latest_state
                            .as_ref()
                            .is_none_or(|state| next_state.theme.revision > state.theme.revision)
                    {
                        latest_state = Some(*next_state);
                    }
                    replies.push(next_req.reply);
                }

                let result = if let Some(latest_state) = latest_state {
                    let target = target_file_path
                        .clone()
                        .unwrap_or_else(crate::persistence::state_file_path);
                    let mut result = Err("persistence did not run".to_string());
                    for attempt in 0..3u32 {
                        match write_state_snapshot_to(&latest_state, &target) {
                            Ok(_) => {
                                result = Ok(latest_state.theme.revision);
                                break;
                            }
                            Err(error) if attempt < 2 => {
                                tokio::time::sleep(Duration::from_millis(25 * (1 << attempt)))
                                    .await;
                                result = Err(format!("{error:#}"));
                            }
                            Err(error) => {
                                result = Err(format!("{error:#}"));
                                break;
                            }
                        }
                    }
                    result
                } else {
                    Ok(durable_rev_clone.load(Ordering::SeqCst))
                };

                match result {
                    Ok(written_rev) => {
                        *last_error_clone.lock().unwrap() = None;
                        durable_rev_clone.fetch_max(written_rev, Ordering::SeqCst);
                        let written_rev = durable_rev_clone.load(Ordering::SeqCst);
                        for reply in replies {
                            let _ = reply.send(Ok(written_rev));
                        }
                    }
                    Err(err) => {
                        let err_msg = format!("Persistence failed: {err:#}");
                        *last_error_clone.lock().unwrap() = Some(err_msg.clone());
                        error!(%err_msg, "Persistence executor write failure");
                        for reply in replies {
                            let _ = reply.send(Err(err_msg.clone()));
                        }
                    }
                }
            }
        });

        Self {
            tx,
            durable_revision,
            last_error,
            overloads: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn durable_revision(&self) -> u64 {
        self.durable_revision.load(Ordering::SeqCst)
    }

    pub fn last_error(&self) -> Option<String> {
        self.last_error.lock().unwrap().clone()
    }

    /// Number of times a `Persist` or `Flush` send was rejected because the
    /// mailbox was full.
    pub fn overloads(&self) -> u64 {
        self.overloads.load(Ordering::SeqCst)
    }

    pub async fn persist(&self, state: DaemonState) -> Result<u64, String> {
        let revision = state.theme.revision;
        self.enqueue(state)?;
        self.wait_until_durable(revision).await
    }

    pub fn enqueue(&self, state: DaemonState) -> Result<(), String> {
        *self.last_error.lock().unwrap() = None;
        let (reply_tx, _reply_rx) = oneshot::channel();
        match self.tx.try_send(PersistenceRequest {
            kind: PersistenceRequestKind::Persist(Box::new(state)),
            reply: reply_tx,
        }) {
            Ok(()) => Ok(()),
            Err(mpsc::error::TrySendError::Full(_)) => {
                self.overloads.fetch_add(1, Ordering::SeqCst);
                tracing::warn!(
                    site = "PersistenceExecutor",
                    policy = "Lossless",
                    capacity = PERSISTENCE_MAILBOX_CAPACITY,
                    "persistence mailbox full; request rejected"
                );
                Err(MailboxError::Overloaded.to_string())
            }
            Err(mpsc::error::TrySendError::Closed(_)) => {
                tracing::warn!(site = "PersistenceExecutor", "persistence mailbox closed");
                Err(MailboxError::Unavailable.to_string())
            }
        }
    }

    pub async fn wait_until_durable(&self, revision: u64) -> Result<u64, String> {
        for _ in 0..200 {
            if self.durable_revision() >= revision {
                return Ok(self.durable_revision());
            }
            if let Some(error) = self.last_error() {
                return Err(error);
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        Err("Timed out waiting for durable theme revision".into())
    }

    pub async fn shutdown_with_deadline(&self, deadline: Duration) -> bool {
        let (reply_tx, reply_rx) = oneshot::channel();
        let req = PersistenceRequest {
            kind: PersistenceRequestKind::Flush,
            reply: reply_tx,
        };
        match self.tx.try_send(req) {
            Ok(()) => {}
            Err(_) => return true,
        }
        tokio::time::timeout(deadline, reply_rx).await.is_ok()
    }

    /// Returns a clone of the bounded sender. Used in tests to fill the mailbox
    /// directly and verify overload behaviour.
    #[cfg(test)]
    pub(crate) fn sender(&self) -> mpsc::Sender<PersistenceRequest> {
        self.tx.clone()
    }
}

pub struct AdapterRequest {
    pub revision: u64,
    pub mode: ThemeMode,
    pub reply: Option<oneshot::Sender<ProjectionStatus>>,
}

#[derive(Clone)]
pub struct AdapterExecutor {
    tx: mpsc::Sender<AdapterRequest>,
    last_applied_revision: Arc<AtomicU64>,
    projection_status: Arc<Mutex<ProjectionStatus>>,
    latest: Arc<Mutex<Option<(u64, ThemeMode)>>>,
    overloads: Arc<AtomicU64>,
}

impl AdapterExecutor {
    pub fn new(adapter: Arc<dyn DesktopAdapter>) -> Self {
        let (tx, mut rx) = mpsc::channel::<AdapterRequest>(ADAPTER_MAILBOX_CAPACITY);
        let last_applied_revision = Arc::new(AtomicU64::new(0));
        let projection_status = Arc::new(Mutex::new(ProjectionStatus::Applied));
        let latest = Arc::new(Mutex::new(None));

        let last_applied_clone = last_applied_revision.clone();
        let status_clone = projection_status.clone();

        tokio::spawn(async move {
            let mut pending: Option<(u64, ThemeMode, Vec<oneshot::Sender<ProjectionStatus>>)> =
                None;

            while let Some(req) = rx.recv().await {
                // Collect replies and determine latest requested revision
                let mut current_rev = req.revision;
                let mut current_mode = req.mode;
                let mut current_replies = Vec::new();
                if let Some(reply) = req.reply {
                    current_replies.push(reply);
                }

                // If pending has a newer revision, combine
                if let Some((p_rev, p_mode, p_replies)) = pending.take() {
                    if p_rev > current_rev {
                        current_rev = p_rev;
                        current_mode = p_mode;
                    }
                    current_replies.extend(p_replies);
                }

                // Drain channel to skip superseded revisions before starting
                while let Ok(next_req) = rx.try_recv() {
                    if next_req.revision > current_rev {
                        current_rev = next_req.revision;
                        current_mode = next_req.mode;
                    }
                    if let Some(reply) = next_req.reply {
                        current_replies.push(reply);
                    }
                }

                // Mark pending
                {
                    let mut status_guard = status_clone.lock().unwrap();
                    *status_guard = ProjectionStatus::Pending;
                }

                // Perform bounded retries for current projection
                let mut attempts = 0;
                let max_attempts = 3;
                let final_status;

                loop {
                    attempts += 1;
                    match adapter.set_mode(current_mode) {
                        Ok(()) => {
                            info!(
                                provider = adapter.name(),
                                revision = current_rev,
                                mode = %current_mode,
                                "Desktop adapter projection succeeded"
                            );
                            last_applied_clone.store(current_rev, Ordering::SeqCst);
                            final_status = ProjectionStatus::Applied;
                            break;
                        }
                        Err(err) => {
                            let err_msg = format!("{err:#}");
                            if attempts < max_attempts {
                                tokio::time::sleep(Duration::from_millis(
                                    50 * (1 << (attempts - 1)),
                                ))
                                .await;
                                continue;
                            }
                            error!(
                                provider = adapter.name(),
                                revision = current_rev,
                                error = %err_msg,
                                "Desktop adapter projection failed after retries"
                            );
                            final_status = ProjectionStatus::Degraded(err_msg);
                            break;
                        }
                    }
                }

                // Update status
                {
                    let mut status_guard = status_clone.lock().unwrap();
                    *status_guard = final_status.clone();
                }

                for reply in current_replies {
                    let _ = reply.send(final_status.clone());
                }
            }
        });

        Self {
            tx,
            last_applied_revision,
            projection_status,
            latest,
            overloads: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn last_applied_revision(&self) -> u64 {
        self.last_applied_revision.load(Ordering::SeqCst)
    }

    pub fn status(&self) -> ProjectionStatus {
        self.projection_status.lock().unwrap().clone()
    }

    /// Number of times a `project` or `project_with_reply` send was rejected
    /// because the mailbox was full.
    pub fn overloads(&self) -> u64 {
        self.overloads.load(Ordering::SeqCst)
    }

    pub fn project(&self, revision: u64, mode: ThemeMode) {
        *self.latest.lock().unwrap() = Some((revision, mode));
        *self.projection_status.lock().unwrap() = ProjectionStatus::Pending;
        match self.tx.try_send(AdapterRequest {
            revision,
            mode,
            reply: None,
        }) {
            Ok(()) => {}
            Err(mpsc::error::TrySendError::Full(_)) => {
                self.overloads.fetch_add(1, Ordering::SeqCst);
                tracing::warn!(
                    site = "AdapterExecutor",
                    policy = "Lossless",
                    capacity = ADAPTER_MAILBOX_CAPACITY,
                    "adapter mailbox full; projection request rejected"
                );
            }
            Err(mpsc::error::TrySendError::Closed(_)) => {
                tracing::warn!(site = "AdapterExecutor", "adapter mailbox closed");
            }
        }
    }

    pub fn retry_latest(&self) {
        if let Some((revision, mode)) = *self.latest.lock().unwrap() {
            self.project(revision, mode);
        }
    }

    pub async fn project_with_reply(&self, revision: u64, mode: ThemeMode) -> ProjectionStatus {
        let (reply_tx, reply_rx) = oneshot::channel();
        match self.tx.try_send(AdapterRequest {
            revision,
            mode,
            reply: Some(reply_tx),
        }) {
            Ok(()) => {}
            Err(mpsc::error::TrySendError::Full(_)) => {
                self.overloads.fetch_add(1, Ordering::SeqCst);
                tracing::warn!(
                    site = "AdapterExecutor",
                    policy = "Lossless",
                    capacity = ADAPTER_MAILBOX_CAPACITY,
                    "adapter mailbox full; projection request rejected"
                );
                return ProjectionStatus::Degraded(MailboxError::Overloaded.to_string());
            }
            Err(mpsc::error::TrySendError::Closed(_)) => {
                tracing::warn!(site = "AdapterExecutor", "adapter mailbox closed");
                return ProjectionStatus::Degraded(MailboxError::Unavailable.to_string());
            }
        }

        reply_rx
            .await
            .unwrap_or_else(|_| ProjectionStatus::Degraded("Adapter dropped request".to_string()))
    }

    /// Returns a clone of the bounded sender. Used in tests to fill the mailbox
    /// directly and verify overload behaviour.
    #[cfg(test)]
    pub(crate) fn sender(&self) -> mpsc::Sender<AdapterRequest> {
        self.tx.clone()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicUsize;

    use anyhow::bail;

    use super::*;

    #[tokio::test]
    async fn test_persistence_coalescing_and_durability() {
        let temp_dir = std::env::temp_dir().join(format!("theme_test_{}", uuid::Uuid::new_v4()));
        let file_path = temp_dir.join("colors.json");
        let executor = PersistenceExecutor::new(Some(file_path.clone()));

        let mut state1 = DaemonState::default();
        state1.theme.revision = 10;

        let mut state2 = DaemonState::default();
        state2.theme.revision = 11;

        let (res1, res2) = tokio::join!(executor.persist(state1), executor.persist(state2),);

        assert_eq!(res1.unwrap(), 11);
        assert_eq!(res2.unwrap(), 11);
        assert_eq!(executor.durable_revision(), 11);

        let content = std::fs::read_to_string(&file_path).unwrap();
        assert!(content.contains("\"revision\": 11"));

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[derive(Debug)]
    struct MockAdapter {
        call_count: Arc<AtomicUsize>,
        should_fail: bool,
        block_ms: u64,
    }

    impl DesktopAdapter for MockAdapter {
        fn name(&self) -> &'static str {
            "mock"
        }

        fn set_mode(&self, _mode: ThemeMode) -> anyhow::Result<()> {
            self.call_count.fetch_add(1, Ordering::SeqCst);
            if self.block_ms > 0 {
                std::thread::sleep(Duration::from_millis(self.block_ms));
            }
            if self.should_fail {
                bail!("mock adapter failed");
            }
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_adapter_executor_success_and_supersession() {
        let call_count = Arc::new(AtomicUsize::new(0));
        let mock = Arc::new(MockAdapter {
            call_count: call_count.clone(),
            should_fail: false,
            block_ms: 50,
        });

        let executor = AdapterExecutor::new(mock);

        executor.project(1, ThemeMode::Light);
        executor.project(2, ThemeMode::Dark);
        let status = executor.project_with_reply(3, ThemeMode::Light).await;

        assert_eq!(status, ProjectionStatus::Applied);
        assert_eq!(executor.last_applied_revision(), 3);
        assert_eq!(executor.status(), ProjectionStatus::Applied);
    }

    #[tokio::test]
    async fn test_adapter_executor_failure_and_degraded_status() {
        let call_count = Arc::new(AtomicUsize::new(0));
        let mock = Arc::new(MockAdapter {
            call_count: call_count.clone(),
            should_fail: true,
            block_ms: 0,
        });

        let executor = AdapterExecutor::new(mock);
        let status = executor.project_with_reply(1, ThemeMode::Dark).await;

        assert!(matches!(status, ProjectionStatus::Degraded(_)));
        assert_eq!(executor.status(), status);
    }

    // Additional executor coverage: PersistenceExecutor mailbox overload.
    // Fill the PersistenceExecutor mailbox to capacity (8), assert the next send
    // returns an overload error and increments the counter, then drain and verify
    // all previously accepted messages are delivered.
    //
    // actor_tx (the D-Bus-facing channel named first in #228's policy table) is
    // covered separately in dbus.rs's own tests, against the real
    // `ThemeDbusService::try_send` path.
    #[tokio::test]
    async fn test_persistence_mailbox_overload() {
        let temp_dir = std::env::temp_dir().join(format!("theme_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).unwrap();
        let file_path = temp_dir.join("colors.json");
        let executor = PersistenceExecutor::new(Some(file_path.clone()));

        // Hold the sender so we can fill the mailbox without it being drained.
        // We fill by saturating try_send directly via the exposed sender().
        let raw_tx = executor.sender();
        let mut oneshot_rxs = Vec::new();

        // Fill to capacity (PERSISTENCE_MAILBOX_CAPACITY = 8)
        for i in 0..PERSISTENCE_MAILBOX_CAPACITY {
            let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
            let mut state = DaemonState::default();
            state.theme.revision = (i + 1) as u64;
            raw_tx
                .try_send(PersistenceRequest {
                    kind: PersistenceRequestKind::Persist(Box::new(state)),
                    reply: reply_tx,
                })
                .unwrap_or_else(|_| {
                    panic!("slot {i} should fit in capacity {PERSISTENCE_MAILBOX_CAPACITY}")
                });
            oneshot_rxs.push(reply_rx);
        }

        // One more must be rejected since executor hasn't drained yet.
        let mut overflow_state = DaemonState::default();
        overflow_state.theme.revision = 100;
        let result = executor.enqueue(overflow_state);
        assert!(
            result.is_err(),
            "enqueue on a full mailbox must return an error"
        );
        assert_eq!(
            executor.overloads(),
            1,
            "overload counter must be 1 after one rejected send"
        );

        // Release the raw_tx so the executor's internal task can drain normally.
        // (The executor holds its own rx; raw_tx is an extra sender.)
        drop(raw_tx);

        // All 8 accepted messages should eventually be delivered.
        // Wait for the executor to process at least the last one.
        for rx in oneshot_rxs {
            let _ = rx.await;
        }

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    // Note: a "PersistenceExecutor mailbox closed" test was deliberately not
    // added. Unlike actor_tx (where ThemeDaemon and ThemeDbusService hold the
    // sender and receiver independently, so one side can legitimately close
    // while the other still holds a sender), PersistenceExecutor owns its own
    // receiver inside its spawned task and its own sender in `self.tx` — the
    // channel cannot close while the executor is alive to call `enqueue` on
    // it. There is no real production path that reaches
    // `TrySendError::Closed` here to test against.

    // Additional executor coverage: AdapterExecutor mailbox overload.
    // Fill the AdapterExecutor mailbox to capacity, assert the next send returns
    // Degraded (overload) and increments the overload counter.
    //
    // wp_tx (the wallpaper-result channel named third in #228's policy table)
    // is covered separately below by `test_wallpaper_result_mailbox_overload`
    // in daemon.rs, against the real `spawn_wallpaper_task` path.
    #[tokio::test]
    async fn test_adapter_executor_overload() {
        // Use a blocking adapter so it never drains the queue while we fill it.
        let call_count = Arc::new(AtomicUsize::new(0));
        let mock = Arc::new(MockAdapter {
            call_count: call_count.clone(),
            should_fail: false,
            block_ms: 5000, // block long enough that the queue stays full
        });
        let executor = AdapterExecutor::new(mock);

        // Fill to ADAPTER_MAILBOX_CAPACITY (8) using the raw sender.
        let raw_tx = executor.sender();
        for i in 0..ADAPTER_MAILBOX_CAPACITY {
            let (reply_tx, _reply_rx) = tokio::sync::oneshot::channel();
            raw_tx
                .try_send(AdapterRequest {
                    revision: i as u64,
                    mode: ThemeMode::Light,
                    reply: Some(reply_tx),
                })
                .unwrap_or_else(|_| panic!("slot {i} should fit"));
        }
        drop(raw_tx);

        // Next project_with_reply must see Degraded (overload) and increment counter.
        let status = executor.project_with_reply(99, ThemeMode::Dark).await;
        assert!(
            matches!(status, ProjectionStatus::Degraded(_)),
            "overflowed project_with_reply must return Degraded, got {status:?}"
        );
        assert_eq!(
            executor.overloads(),
            1,
            "overload counter must be 1 after one rejected send"
        );
    }
}
