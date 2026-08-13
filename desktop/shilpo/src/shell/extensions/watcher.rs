use std::{path::PathBuf, sync::mpsc};

use notify::{Config, Event, RecommendedWatcher, RecursiveMode, Watcher};
use shilpo_ext_runtime::ExtensionCommand;

pub struct ExtensionWatcher {
    _watcher: RecommendedWatcher,
}

impl ExtensionWatcher {
    pub fn new(
        command_tx: mpsc::SyncSender<ExtensionCommand>,
        watch_paths: Vec<PathBuf>,
    ) -> Result<Self, String> {
        let watcher_tx = command_tx;
        let mut watcher = RecommendedWatcher::new(
            move |res: Result<Event, notify::Error>| {
                if let Ok(event) = res
                    && (event.kind.is_modify() || event.kind.is_create() || event.kind.is_remove())
                {
                    let _ = watcher_tx.try_send(ExtensionCommand::SourcesChanged);
                }
            },
            Config::default(),
        )
        .map_err(|e| format!("failed to create extension watcher: {e}"))?;

        let mut watched = 0;
        for path in watch_paths {
            if path.exists() {
                watcher
                    .watch(&path, RecursiveMode::Recursive)
                    .map_err(|e| format!("failed to watch {}: {e}", path.display()))?;
                watched += 1;
            }
        }

        if watched == 0 {
            return Err("no extension source paths were available to watch".into());
        }

        Ok(Self { _watcher: watcher })
    }
}
