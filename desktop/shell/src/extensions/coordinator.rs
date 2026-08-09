use shilpo_ext::{AuthorizedHostEffect, ViewTree};
use shilpo_ext_types::{CanonicalId, ExtensionId};
use std::{
    collections::BTreeMap,
    path::{Component, Path, PathBuf},
    sync::{Arc, Mutex, RwLock, mpsc},
    time::Duration,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ContributionSurface {
    Bar,
    Desktop,
    Settings,
    SidePanel,
    Launcher,
    Action,
    Background,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContributionDescriptor {
    pub id: CanonicalId,
    pub extension_name: String,
    pub name: String,
    pub surface: ContributionSurface,
    pub settings_schema: Option<String>,
    pub default_size: Option<(u32, u32)>,
    pub minimum_size: Option<(u32, u32)>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ContributionInstance {
    pub id: String,
    pub contribution: CanonicalId,
    pub output: Option<String>,
    pub width: f32,
    pub height: f32,
    pub settings: serde_json::Value,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ExtensionGeneration(pub u64);

impl ExtensionGeneration {
    pub fn next(self) -> Self {
        Self(self.0 + 1)
    }
}

#[derive(Clone, Debug, Default)]
pub struct ExtensionSnapshot {
    pub generation: ExtensionGeneration,
    pub descriptors: Arc<[ContributionDescriptor]>,
    pub views: Arc<BTreeMap<CanonicalId, ViewTree>>,
    pub diagnostics: Arc<[String]>,
    pub catalog_changed_at: Option<ExtensionGeneration>,
    pub settings_schemas: Arc<BTreeMap<CanonicalId, serde_json::Value>>,
    pub prevalidated_asset_roots: Arc<BTreeMap<ExtensionId, PathBuf>>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ExtensionChanges {
    pub effects: Vec<(ExtensionId, AuthorizedHostEffect)>,
    pub invalidated_views: Vec<CanonicalId>,
    pub catalog_changed: bool,
}

impl ExtensionChanges {
    pub fn merge(&mut self, mut other: Self) {
        self.effects.append(&mut other.effects);
        self.invalidated_views.append(&mut other.invalidated_views);
        self.catalog_changed |= other.catalog_changed;
    }
}

pub enum ReplaceableEvent {
    Power {
        percentage: Option<f32>,
        charging: bool,
    },
    Network {
        connected: bool,
    },
    Media {
        title: Option<String>,
        artist: Option<String>,
        playing: bool,
    },
    TimerFired(String),
}

pub enum ExtensionCommand {
    Lifecycle {
        expected: ExtensionGeneration,
        event: shilpo_ext::ExtensionEvent,
    },
    Input {
        expected: ExtensionGeneration,
        contribution: CanonicalId,
        instance_id: Option<String>,
        event_id: String,
        value: Option<serde_json::Value>,
    },
    Response {
        expected: ExtensionGeneration,
        extension_id: ExtensionId,
        event: shilpo_ext::ExtensionEvent,
    },
    Replaceable(ReplaceableEvent),
    ReconcileInstances {
        expected: ExtensionGeneration,
        desired: Vec<ContributionInstance>,
    },
    SourcesChanged,
    Shutdown {
        reply: mpsc::Sender<()>,
    },
}

#[derive(Debug)]
pub struct ExtensionUpdate {
    pub generation: ExtensionGeneration,
    pub snapshot: Option<ExtensionSnapshot>,
    pub effects: Vec<(ExtensionId, AuthorizedHostEffect)>,
    pub invalidated_views: Vec<CanonicalId>,
}

pub struct ExtensionCoordinator {
    snapshot: Arc<RwLock<ExtensionSnapshot>>,
    command_tx: mpsc::SyncSender<ExtensionCommand>,
    update_rx: Arc<Mutex<mpsc::Receiver<ExtensionUpdate>>>,
    pending_commands: Arc<Mutex<std::collections::VecDeque<ExtensionCommand>>>,
    _worker_task: Option<gpui::Task<()>>,
    _command_retry_task: Option<gpui::Task<()>>,
    _watcher: Option<super::watcher::ExtensionWatcher>,
    _fallback_scan: Option<gpui::Task<()>>,
}

impl ExtensionCoordinator {
    pub fn init(executor: gpui::BackgroundExecutor) -> Option<Self> {
        let paths = shilpo_ext::CatalogPaths::platform_default();
        Self::init_with_paths(executor, paths)
    }

    pub fn init_with_paths(
        executor: gpui::BackgroundExecutor,
        paths: shilpo_ext::CatalogPaths,
    ) -> Option<Self> {
        match shilpo_ext::WasmRuntime::new() {
            Ok(runtime) => match super::engine::ExtensionEngine::new(runtime, paths.clone()) {
                Ok(engine) => {
                    let (command_tx, command_rx) = std::sync::mpsc::sync_channel(64);
                    let (update_tx, update_rx) = std::sync::mpsc::sync_channel(64);
                    let snapshot =
                        std::sync::Arc::new(std::sync::RwLock::new(ExtensionSnapshot::default()));

                    let watch_paths = vec![
                        shilpo_ext::default_extension_state_dir().join("dev"),
                        paths.data_dir.join("installed"),
                        paths.data_dir.join("activated"),
                    ];
                    let mut watcher = None;
                    let mut fallback_scan = None;
                    match super::watcher::ExtensionWatcher::new(command_tx.clone(), watch_paths) {
                        Ok(w) => watcher = Some(w),
                        Err(error) => {
                            tracing::warn!(%error, "ExtensionWatcher failed, falling back to 30s background scan");
                            fallback_scan = spawn_fallback_scan(
                                &executor,
                                command_tx.clone(),
                                Duration::from_secs(30),
                            );
                        }
                    }

                    let worker_task = engine.run_worker_loop(
                        executor.clone(),
                        command_rx,
                        update_tx,
                        snapshot.clone(),
                    );

                    Some(Self::new_with_executor(
                        Some(executor.clone()),
                        snapshot,
                        command_tx,
                        update_rx,
                        Some(worker_task),
                        watcher,
                        fallback_scan,
                    ))
                }
                Err(error) => {
                    tracing::warn!(error = %error, "extension engine load failed");
                    None
                }
            },
            Err(error) => {
                tracing::warn!(error = %error, "extension runtime is unavailable");
                None
            }
        }
    }

    pub fn new(
        snapshot: Arc<RwLock<ExtensionSnapshot>>,
        command_tx: mpsc::SyncSender<ExtensionCommand>,
        update_rx: mpsc::Receiver<ExtensionUpdate>,
        worker_task: Option<gpui::Task<()>>,
        watcher: Option<super::watcher::ExtensionWatcher>,
        fallback_scan: Option<gpui::Task<()>>,
    ) -> Self {
        Self::new_with_executor(
            None,
            snapshot,
            command_tx,
            update_rx,
            worker_task,
            watcher,
            fallback_scan,
        )
    }

    pub fn new_with_executor(
        executor: Option<gpui::BackgroundExecutor>,
        snapshot: Arc<RwLock<ExtensionSnapshot>>,
        command_tx: mpsc::SyncSender<ExtensionCommand>,
        update_rx: mpsc::Receiver<ExtensionUpdate>,
        worker_task: Option<gpui::Task<()>>,
        watcher: Option<super::watcher::ExtensionWatcher>,
        fallback_scan: Option<gpui::Task<()>>,
    ) -> Self {
        let pending_commands = Arc::new(Mutex::new(std::collections::VecDeque::new()));
        let command_retry_task = executor.map(|executor| {
            let retry_queue = pending_commands.clone();
            let retry_tx = command_tx.clone();
            let retry_executor = executor.clone();
            executor.spawn(async move {
                loop {
                    let command = retry_queue.lock().unwrap().pop_front();
                    let Some(command) = command else {
                        retry_executor.timer(Duration::from_millis(25)).await;
                        continue;
                    };
                    match retry_tx.try_send(command) {
                        Ok(()) => {}
                        Err(mpsc::TrySendError::Full(command)) => {
                            retry_queue.lock().unwrap().push_front(command);
                            retry_executor.timer(Duration::from_millis(25)).await;
                        }
                        Err(mpsc::TrySendError::Disconnected(_)) => break,
                    }
                }
            })
        });
        Self {
            snapshot,
            command_tx,
            update_rx: Arc::new(Mutex::new(update_rx)),
            pending_commands,
            _worker_task: worker_task,
            _command_retry_task: command_retry_task,
            _watcher: watcher,
            _fallback_scan: fallback_scan,
        }
    }

    pub fn snapshot(&self) -> ExtensionSnapshot {
        self.snapshot.read().unwrap().clone()
    }

    pub fn generation(&self) -> ExtensionGeneration {
        self.snapshot.read().unwrap().generation
    }

    pub fn descriptors(&self) -> Vec<ContributionDescriptor> {
        self.snapshot.read().unwrap().descriptors.to_vec()
    }

    pub fn descriptors_for(&self, surface: ContributionSurface) -> Vec<ContributionDescriptor> {
        self.descriptors()
            .into_iter()
            .filter(|descriptor| descriptor.surface == surface)
            .collect()
    }

    pub fn diagnostics(&self) -> Arc<[String]> {
        self.snapshot.read().unwrap().diagnostics.clone()
    }

    pub fn view(&self, id: &CanonicalId) -> Option<ViewTree> {
        self.snapshot.read().unwrap().views.get(id).cloned()
    }

    pub fn settings_schema(&self, id: &CanonicalId) -> Result<Option<serde_json::Value>, String> {
        Ok(self
            .snapshot
            .read()
            .unwrap()
            .settings_schemas
            .get(id)
            .cloned())
    }

    pub fn asset_path(&self, id: &CanonicalId, relative: &str) -> Result<PathBuf, String> {
        let snapshot = self.snapshot.read().unwrap();
        let root = snapshot
            .prevalidated_asset_roots
            .get(&id.extension_id)
            .ok_or_else(|| format!("extension '{}' has no active asset root", id.extension_id))?;
        let path = safe_child(&root.join("assets"), relative)?;
        if path.is_file() {
            Ok(path)
        } else {
            Err(format!("extension asset {} is unavailable", path.display()))
        }
    }

    pub fn send_command(&self, command: ExtensionCommand) -> Result<(), String> {
        const MAX_PENDING_COMMANDS: usize = 256;
        let mut pending = self.pending_commands.lock().unwrap();
        if !pending.is_empty() {
            if pending.len() >= MAX_PENDING_COMMANDS {
                return Err("extension command queue full".into());
            }
            pending.push_back(command);
            return Ok(());
        }
        match self.command_tx.try_send(command) {
            Ok(()) => Ok(()),
            Err(mpsc::TrySendError::Full(command)) => {
                pending.push_back(command);
                Ok(())
            }
            Err(mpsc::TrySendError::Disconnected(_)) => Err("extension engine disconnected".into()),
        }
    }

    pub fn drain_updates(&self) -> Vec<ExtensionUpdate> {
        let mut updates = Vec::new();
        if let Ok(rx) = self.update_rx.lock() {
            while let Ok(update) = rx.try_recv() {
                if let Some(ref new_snapshot) = update.snapshot {
                    let mut lock = self.snapshot.write().unwrap();
                    if new_snapshot.generation >= lock.generation {
                        *lock = new_snapshot.clone();
                    }
                }
                updates.push(update);
            }
        }
        updates
    }

    pub fn shutdown(
        &self,
        executor: gpui::BackgroundExecutor,
        timeout: Duration,
    ) -> gpui::Task<bool> {
        let (reply_tx, reply_rx) = mpsc::channel();
        let cmd_res = self.send_command(ExtensionCommand::Shutdown { reply: reply_tx });
        executor.spawn(async move {
            if cmd_res.is_err() {
                return false;
            }
            reply_rx.recv_timeout(timeout).is_ok()
        })
    }
}

pub fn safe_child(base: &Path, relative: &str) -> Result<PathBuf, String> {
    let rel_path = Path::new(relative);
    if rel_path.is_absolute() {
        return Err("path must be relative".into());
    }
    for component in rel_path.components() {
        match component {
            Component::Normal(_) | Component::CurDir => {}
            Component::ParentDir | Component::RootDir => {
                return Err("path contains parent or root directory traversal".into());
            }
            Component::Prefix(_) => return Err("path contains invalid component".into()),
        }
    }
    Ok(base.join(rel_path))
}

fn spawn_fallback_scan(
    executor: &gpui::BackgroundExecutor,
    command_tx: mpsc::SyncSender<ExtensionCommand>,
    interval: Duration,
) -> Option<gpui::Task<()>> {
    let fallback_tx = command_tx.clone();
    let executor_inner = executor.clone();
    Some(executor.clone().spawn(async move {
        loop {
            executor_inner.timer(interval).await;
            if fallback_tx.send(ExtensionCommand::SourcesChanged).is_err() {
                break;
            }
        }
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use shilpo_ext_types::ContributionId;

    #[test]
    fn test_extension_generation_increment() {
        let gen0 = ExtensionGeneration::default();
        assert_eq!(gen0.0, 0);
        let gen1 = gen0.next();
        assert_eq!(gen1.0, 1);
        assert!(gen1 > gen0);
    }

    #[test]
    fn test_safe_child_traversal_prevention() {
        let base = Path::new("/tmp/test_ext");
        assert!(safe_child(base, "../etc/passwd").is_err());
        assert!(safe_child(base, "/etc/passwd").is_err());
        let valid = safe_child(base, "assets/logo.png").unwrap();
        assert_eq!(valid, PathBuf::from("/tmp/test_ext/assets/logo.png"));
    }

    #[test]
    fn test_coordinator_snapshot_reads() {
        let ext_id = ExtensionId::new("org.test.ext").unwrap();
        let contrib_id = ContributionId::new("bar_widget").unwrap();
        let snapshot = Arc::new(RwLock::new(ExtensionSnapshot {
            generation: ExtensionGeneration(1),
            descriptors: vec![ContributionDescriptor {
                id: CanonicalId {
                    extension_id: ext_id,
                    contribution_id: contrib_id,
                },
                extension_name: "Test Extension".into(),
                name: "Bar Widget".into(),
                surface: ContributionSurface::Bar,
                settings_schema: None,
                default_size: None,
                minimum_size: None,
            }]
            .into(),
            views: Arc::new(BTreeMap::new()),
            diagnostics: vec!["no errors".to_string()].into(),
            catalog_changed_at: Some(ExtensionGeneration(1)),
            settings_schemas: Arc::new(BTreeMap::new()),
            prevalidated_asset_roots: Arc::new(BTreeMap::new()),
        }));

        let (cmd_tx, _cmd_rx) = mpsc::sync_channel(16);
        let (_upd_tx, upd_rx) = mpsc::sync_channel(16);

        let coordinator = ExtensionCoordinator::new(snapshot, cmd_tx, upd_rx, None, None, None);

        assert_eq!(coordinator.generation(), ExtensionGeneration(1));
        assert_eq!(coordinator.descriptors().len(), 1);
        assert_eq!(
            coordinator.descriptors_for(ContributionSurface::Bar).len(),
            1
        );
        assert_eq!(
            coordinator
                .descriptors_for(ContributionSurface::Launcher)
                .len(),
            0
        );
        assert_eq!(
            coordinator.diagnostics().as_ref(),
            &["no errors".to_string()]
        );
    }

    #[test]
    fn test_coordinator_init_with_paths_creates_runtime() {
        let temp_base =
            std::env::temp_dir().join(format!("shilpo_ext_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(temp_base.join("data")).unwrap();
        std::fs::create_dir_all(temp_base.join("config")).unwrap();
        let paths = shilpo_ext::CatalogPaths::new(temp_base.join("data"), temp_base.join("config"));

        let executor = gpui::TestAppContext::single().executor().clone();
        let coordinator = ExtensionCoordinator::init_with_paths(executor, paths);
        assert!(coordinator.is_some());

        drop(coordinator);
        let _ = std::fs::remove_dir_all(temp_base);
    }

    #[test]
    fn test_fallback_scan_emits_sources_changed() {
        let cx = gpui::TestAppContext::single();
        let executor = cx.executor().clone();
        let (command_tx, command_rx) = mpsc::sync_channel(16);
        let task = spawn_fallback_scan(&executor, command_tx, Duration::from_millis(10));
        assert!(task.is_some());

        // The fallback scan fires SourcesChanged on its interval.
        executor.advance_clock(Duration::from_millis(10));
        executor.tick();
        executor.tick();
        assert!(matches!(
            command_rx.try_recv(),
            Ok(ExtensionCommand::SourcesChanged)
        ));

        // Disconnecting the receiver terminates the scan loop.
        drop(command_rx);
        executor.advance_clock(Duration::from_millis(10));
        executor.tick();
        executor.tick();
        drop(task);
    }
}
