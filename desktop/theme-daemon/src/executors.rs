use crate::adapters::DesktopAdapter;
use crate::daemon::DaemonState;
use crate::persistence::write_state_snapshot_to;
use serde::{Deserialize, Serialize};
use shilpo_theme::ThemeMode;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};
use tracing::{error, info};

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
    tx: mpsc::UnboundedSender<PersistenceRequest>,
    durable_revision: Arc<AtomicU64>,
    last_error: Arc<Mutex<Option<String>>>,
}

impl PersistenceExecutor {
    pub fn new(target_file_path: Option<PathBuf>) -> Self {
        let (tx, mut rx) = mpsc::unbounded_channel::<PersistenceRequest>();
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
        }
    }

    pub fn durable_revision(&self) -> u64 {
        self.durable_revision.load(Ordering::SeqCst)
    }

    pub fn last_error(&self) -> Option<String> {
        self.last_error.lock().unwrap().clone()
    }

    pub async fn persist(&self, state: DaemonState) -> Result<u64, String> {
        let revision = state.theme.revision;
        self.enqueue(state)?;
        self.wait_until_durable(revision).await
    }

    pub fn enqueue(&self, state: DaemonState) -> Result<(), String> {
        *self.last_error.lock().unwrap() = None;
        let (reply_tx, _reply_rx) = oneshot::channel();
        self.tx
            .send(PersistenceRequest {
                kind: PersistenceRequestKind::Persist(Box::new(state)),
                reply: reply_tx,
            })
            .map_err(|_| "Persistence executor channel closed".to_string())
            .map(|_| ())
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
        if self.tx.send(req).is_err() {
            return true;
        }
        tokio::time::timeout(deadline, reply_rx).await.is_ok()
    }
}

pub struct AdapterRequest {
    pub revision: u64,
    pub mode: ThemeMode,
    pub reply: Option<oneshot::Sender<ProjectionStatus>>,
}

#[derive(Clone)]
pub struct AdapterExecutor {
    tx: mpsc::UnboundedSender<AdapterRequest>,
    last_applied_revision: Arc<AtomicU64>,
    projection_status: Arc<Mutex<ProjectionStatus>>,
    latest: Arc<Mutex<Option<(u64, ThemeMode)>>>,
}

impl AdapterExecutor {
    pub fn new(adapter: Arc<dyn DesktopAdapter>) -> Self {
        let (tx, mut rx) = mpsc::unbounded_channel::<AdapterRequest>();
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
        }
    }

    pub fn last_applied_revision(&self) -> u64 {
        self.last_applied_revision.load(Ordering::SeqCst)
    }

    pub fn status(&self) -> ProjectionStatus {
        self.projection_status.lock().unwrap().clone()
    }

    pub fn project(&self, revision: u64, mode: ThemeMode) {
        *self.latest.lock().unwrap() = Some((revision, mode));
        *self.projection_status.lock().unwrap() = ProjectionStatus::Pending;
        let _ = self.tx.send(AdapterRequest {
            revision,
            mode,
            reply: None,
        });
    }

    pub fn retry_latest(&self) {
        if let Some((revision, mode)) = *self.latest.lock().unwrap() {
            self.project(revision, mode);
        }
    }

    pub async fn project_with_reply(&self, revision: u64, mode: ThemeMode) -> ProjectionStatus {
        let (reply_tx, reply_rx) = oneshot::channel();
        if self
            .tx
            .send(AdapterRequest {
                revision,
                mode,
                reply: Some(reply_tx),
            })
            .is_err()
        {
            return ProjectionStatus::Degraded("Adapter executor channel closed".to_string());
        }

        reply_rx
            .await
            .unwrap_or_else(|_| ProjectionStatus::Degraded("Adapter dropped request".to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::bail;
    use std::sync::atomic::AtomicUsize;

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
}
