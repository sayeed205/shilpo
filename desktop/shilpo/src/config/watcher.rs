//! Configuration file watching, path classification, and debouncing.
//!
//! Provides directory-based watching for Shilpo's layered declarative
//! configuration sources (`config.toml`, `overrides.toml`, and immediate `conf.d/*.toml` fragments)
//! with trailing-edge debouncing at 100 ms.

use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};
use std::{
    fmt,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

/// Logical path classification for filesystem events.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClassifiedPath {
    Primary,
    Overrides,
    Fragment,
    ConfDir,
    Irrelevant,
}

/// Classify a path relative to the base `config_dir`.
pub fn classify_path(config_dir: &Path, path: &Path) -> ClassifiedPath {
    let Ok(rel) = path.strip_prefix(config_dir) else {
        return ClassifiedPath::Irrelevant;
    };

    let components: Vec<_> = rel.components().map(|c| c.as_os_str()).collect();
    if components.is_empty() {
        return ClassifiedPath::Irrelevant;
    }

    if components.len() == 1 {
        let name = components[0].to_string_lossy();
        if name == "config.toml" {
            return ClassifiedPath::Primary;
        } else if name == "overrides.toml" {
            return ClassifiedPath::Overrides;
        } else if name == "conf.d" {
            return ClassifiedPath::ConfDir;
        } else {
            return ClassifiedPath::Irrelevant;
        }
    }

    if components.len() == 2 && components[0] == "conf.d" {
        let filename = components[1].to_string_lossy();
        if is_valid_toml_fragment(&filename) {
            return ClassifiedPath::Fragment;
        }
    }

    ClassifiedPath::Irrelevant
}

/// Returns `true` if `path` is a relevant configuration source under `config_dir`.
pub fn is_relevant_path(config_dir: &Path, path: &Path) -> bool {
    classify_path(config_dir, path) != ClassifiedPath::Irrelevant
}

fn is_valid_toml_fragment(filename: &str) -> bool {
    if !filename.ends_with(".toml") {
        return false;
    }
    if filename.starts_with('.') {
        return false;
    }
    if filename.ends_with('~') || filename.ends_with(".swp") {
        return false;
    }
    true
}

/// State of the trailing-edge debounce state machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DebounceState {
    Idle,
    Debouncing {
        deadline: Instant,
        burst_count: usize,
    },
    Reloading {
        pending_event: bool,
        burst_count: usize,
    },
}

/// Action to take after ticking the debounce state machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DebounceAction {
    WaitUntil(Instant),
    TriggerReload { burst_size: usize },
    Nothing,
}

/// Pure trailing-edge debounce state machine.
#[derive(Debug, Clone)]
pub struct DebounceStateMachine {
    state: DebounceState,
    debounce_duration: Duration,
}

impl DebounceStateMachine {
    pub fn new(debounce_duration: Duration) -> Self {
        Self {
            state: DebounceState::Idle,
            debounce_duration,
        }
    }

    pub fn state(&self) -> DebounceState {
        self.state
    }

    pub fn on_event(&mut self, now: Instant) {
        match self.state {
            DebounceState::Idle => {
                self.state = DebounceState::Debouncing {
                    deadline: now + self.debounce_duration,
                    burst_count: 1,
                };
            }
            DebounceState::Debouncing { burst_count, .. } => {
                self.state = DebounceState::Debouncing {
                    deadline: now + self.debounce_duration,
                    burst_count: burst_count + 1,
                };
            }
            DebounceState::Reloading { burst_count, .. } => {
                self.state = DebounceState::Reloading {
                    pending_event: true,
                    burst_count: burst_count + 1,
                };
            }
        }
    }

    pub fn tick(&mut self, now: Instant) -> DebounceAction {
        match self.state {
            DebounceState::Idle => DebounceAction::Nothing,
            DebounceState::Debouncing {
                deadline,
                burst_count,
            } => {
                if now >= deadline {
                    self.state = DebounceState::Reloading {
                        pending_event: false,
                        burst_count,
                    };
                    DebounceAction::TriggerReload {
                        burst_size: burst_count,
                    }
                } else {
                    DebounceAction::WaitUntil(deadline)
                }
            }
            DebounceState::Reloading { .. } => DebounceAction::Nothing,
        }
    }

    pub fn on_reload_complete(&mut self, now: Instant) {
        if let DebounceState::Reloading { pending_event, .. } = self.state {
            if pending_event {
                self.state = DebounceState::Debouncing {
                    deadline: now + self.debounce_duration,
                    burst_count: 1,
                };
            } else {
                self.state = DebounceState::Idle;
            }
        }
    }
}

/// Event emitted by the filesystem watcher channel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigWatchEvent {
    FilesystemChanged { paths: Vec<PathBuf> },
    RuntimeError(String),
}

/// Structured error for configuration file watcher operations.
#[derive(Debug)]
pub enum ConfigWatchError {
    Creation(notify::Error),
    WatchPath {
        path: PathBuf,
        source: notify::Error,
    },
}

impl fmt::Display for ConfigWatchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Creation(e) => write!(f, "failed to create notify watcher: {e}"),
            Self::WatchPath { path, source } => {
                write!(f, "failed to watch path {}: {source}", path.display())
            }
        }
    }
}

impl std::error::Error for ConfigWatchError {}

/// File watcher owning a [`notify::RecommendedWatcher`].
pub struct ConfigWatcher {
    watcher: RecommendedWatcher,
    config_dir: PathBuf,
    watching_conf_d: bool,
    pending_flag: Arc<AtomicBool>,
}

impl ConfigWatcher {
    pub fn new(
        config_dir: PathBuf,
        event_tx: mpsc::SyncSender<ConfigWatchEvent>,
    ) -> Result<Self, ConfigWatchError> {
        let dir_for_callback = config_dir.clone();
        let pending_flag = Arc::new(AtomicBool::new(false));
        let pending_flag_cb = pending_flag.clone();

        let mut watcher = RecommendedWatcher::new(
            move |res: Result<Event, notify::Error>| match res {
                Ok(event) => {
                    let is_relevant = event
                        .paths
                        .iter()
                        .any(|p| is_relevant_path(&dir_for_callback, p));
                    if is_relevant
                        && event_tx
                            .try_send(ConfigWatchEvent::FilesystemChanged {
                                paths: event.paths.clone(),
                            })
                            .is_err()
                    {
                        pending_flag_cb.store(true, Ordering::SeqCst);
                    }
                }
                Err(err) => {
                    let _ = event_tx.try_send(ConfigWatchEvent::RuntimeError(err.to_string()));
                }
            },
            notify::Config::default(),
        )
        .map_err(ConfigWatchError::Creation)?;

        watcher
            .watch(&config_dir, RecursiveMode::NonRecursive)
            .map_err(|source| ConfigWatchError::WatchPath {
                path: config_dir.clone(),
                source,
            })?;

        let conf_d = config_dir.join("conf.d");
        let watching_conf_d = if conf_d.is_dir() {
            match watcher.watch(&conf_d, RecursiveMode::NonRecursive) {
                Ok(()) => true,
                Err(err) => {
                    return Err(ConfigWatchError::WatchPath {
                        path: conf_d,
                        source: err,
                    });
                }
            }
        } else {
            false
        };

        Ok(Self {
            watcher,
            config_dir,
            watching_conf_d,
            pending_flag,
        })
    }

    /// Refresh directory watches if `conf.d` was created or deleted.
    pub fn refresh_watches(&mut self) {
        let conf_d = self.config_dir.join("conf.d");
        if conf_d.is_dir() && !self.watching_conf_d {
            if self
                .watcher
                .watch(&conf_d, RecursiveMode::NonRecursive)
                .is_ok()
            {
                self.watching_conf_d = true;
            }
        } else if !conf_d.exists() && self.watching_conf_d {
            let _ = self.watcher.unwatch(&conf_d);
            self.watching_conf_d = false;
        }
    }

    /// Check and clear any pending overflow signal.
    pub fn take_pending(&self) -> bool {
        self.pending_flag.swap(false, Ordering::SeqCst)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn path_classification_rules() {
        let dir = Path::new("/home/user/.config/shilpo");

        // Relevant primary & overrides
        assert_eq!(
            classify_path(dir, &dir.join("config.toml")),
            ClassifiedPath::Primary
        );
        assert_eq!(
            classify_path(dir, &dir.join("overrides.toml")),
            ClassifiedPath::Overrides
        );
        assert_eq!(
            classify_path(dir, &dir.join("conf.d")),
            ClassifiedPath::ConfDir
        );

        // Immediate fragment
        assert_eq!(
            classify_path(dir, &dir.join("conf.d/01-bar.toml")),
            ClassifiedPath::Fragment
        );

        // Nested fragment under conf.d -> Ignored
        assert_eq!(
            classify_path(dir, &dir.join("conf.d/sub/01-bar.toml")),
            ClassifiedPath::Irrelevant
        );

        // Backup files -> Ignored
        assert_eq!(
            classify_path(dir, &dir.join("config.toml.bak.20260812")),
            ClassifiedPath::Irrelevant
        );
        assert_eq!(
            classify_path(dir, &dir.join("conf.d/01-bar.toml.bak")),
            ClassifiedPath::Irrelevant
        );
        assert_eq!(
            classify_path(dir, &dir.join("conf.d/foo.bak.toml")),
            ClassifiedPath::Fragment
        );

        // Migration temp files -> Ignored
        assert_eq!(
            classify_path(dir, &dir.join("config.toml.1234.uuid.tmp")),
            ClassifiedPath::Irrelevant
        );

        // Non-TOML files -> Ignored
        assert_eq!(
            classify_path(dir, &dir.join("conf.d/notes.txt")),
            ClassifiedPath::Irrelevant
        );

        // Hidden files -> Ignored
        assert_eq!(
            classify_path(dir, &dir.join("conf.d/.DS_Store")),
            ClassifiedPath::Irrelevant
        );

        // Session & operational files -> Ignored
        assert_eq!(
            classify_path(dir, &dir.join("session.json")),
            ClassifiedPath::Irrelevant
        );
    }

    #[test]
    fn debounce_one_event_triggers_after_duration() {
        let duration = Duration::from_millis(100);
        let mut state_machine = DebounceStateMachine::new(duration);
        let t0 = Instant::now();

        assert_eq!(state_machine.tick(t0), DebounceAction::Nothing);

        state_machine.on_event(t0);
        assert_eq!(
            state_machine.tick(t0 + Duration::from_millis(50)),
            DebounceAction::WaitUntil(t0 + duration)
        );

        assert_eq!(
            state_machine.tick(t0 + duration),
            DebounceAction::TriggerReload { burst_size: 1 }
        );
    }

    #[test]
    fn debounce_rapid_events_extend_trailing_deadline() {
        let duration = Duration::from_millis(100);
        let mut state_machine = DebounceStateMachine::new(duration);
        let t0 = Instant::now();

        state_machine.on_event(t0);
        state_machine.on_event(t0 + Duration::from_millis(40));
        state_machine.on_event(t0 + Duration::from_millis(80));

        // At t0 + 100ms, should wait for extended deadline (t0 + 80ms + 100ms = t0 + 180ms)
        assert_eq!(
            state_machine.tick(t0 + Duration::from_millis(100)),
            DebounceAction::WaitUntil(t0 + Duration::from_millis(180))
        );

        assert_eq!(
            state_machine.tick(t0 + Duration::from_millis(180)),
            DebounceAction::TriggerReload { burst_size: 3 }
        );
    }

    #[test]
    fn debounce_event_during_reload_schedules_follow_up() {
        let duration = Duration::from_millis(100);
        let mut state_machine = DebounceStateMachine::new(duration);
        let t0 = Instant::now();

        state_machine.on_event(t0);
        assert_eq!(
            state_machine.tick(t0 + duration),
            DebounceAction::TriggerReload { burst_size: 1 }
        );

        // Event arrives while reload is running at t0 + 120ms
        state_machine.on_event(t0 + Duration::from_millis(120));

        // Reload completes at t0 + 150ms
        let t_complete = t0 + Duration::from_millis(150);
        state_machine.on_reload_complete(t_complete);

        // Now in Debouncing state with new deadline t_complete + 100ms = t0 + 250ms
        assert_eq!(
            state_machine.tick(t0 + Duration::from_millis(200)),
            DebounceAction::WaitUntil(t_complete + duration)
        );

        assert_eq!(
            state_machine.tick(t_complete + duration),
            DebounceAction::TriggerReload { burst_size: 1 }
        );
    }

    #[test]
    fn real_notify_watcher_detects_file_changes_in_temp_dir() {
        let temp = TempDir::new().unwrap();
        let config_dir = temp.path().to_path_buf();
        let primary = config_dir.join("config.toml");

        let (tx, rx) = mpsc::sync_channel(16);
        let mut watcher = ConfigWatcher::new(config_dir.clone(), tx).unwrap();

        std::fs::write(&primary, "version = 1\n").unwrap();

        // Wait bounded time for event
        let mut received = false;
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(3) {
            watcher.refresh_watches();
            if rx.recv_timeout(Duration::from_millis(100)).is_ok() || watcher.take_pending() {
                received = true;
                break;
            }
        }
        assert!(received, "notify watcher failed to detect primary edit");
    }
}
