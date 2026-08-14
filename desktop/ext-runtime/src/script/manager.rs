use std::{
    collections::{BTreeMap, HashMap},
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

use shilpo_ext_api::{CanonicalId, ExtensionId, ViewTree};

use crate::worker::protocol::{
    ContributionDescriptor, ContributionSurface, ExtensionRuntimeKind, ScriptExtensionStatus,
};
use crate::{CatalogPaths, CircuitBreaker, DiagnosticCode};

use super::{
    manifest::{ScriptManifest, ScriptMode},
    record::decode_and_validate_record,
    runner::{ProcessOutput, ProcessRunner, RealProcessRunner, ScriptProcessError},
};

const MAX_DIAGNOSTICS_PER_BUNDLE: usize = 64;
const MAX_SOURCE_DIAGNOSTICS: usize = 128;

pub trait ScriptClock: Send + Sync {
    fn now(&self) -> Instant;
}

#[derive(Default)]
pub struct SystemScriptClock;

impl ScriptClock for SystemScriptClock {
    fn now(&self) -> Instant {
        Instant::now()
    }
}

#[derive(Clone, Debug)]
pub struct ScriptBundleInfo {
    pub id: ExtensionId,
    pub name: String,
    pub version: String,
    pub path: PathBuf,
    pub mode: ScriptMode,
    pub contributions_count: usize,
    pub status: String,
    pub diagnostics: Vec<String>,
}

enum TaskEvent {
    PollFinished(Result<ProcessOutput, ScriptProcessError>),
    StreamRecord(Vec<u8>),
    StreamStopped(Result<String, ScriptProcessError>),
}

struct RunningTask {
    cancelled: Arc<AtomicBool>,
    events: mpsc::Receiver<TaskEvent>,
    join: Option<thread::JoinHandle<()>>,
}

impl RunningTask {
    fn cancel_and_join(&mut self) {
        self.cancelled.store(true, Ordering::Release);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

struct ScriptBundleState {
    manifest: ScriptManifest,
    bundle_root: PathBuf,
    views: HashMap<CanonicalId, ViewTree>,
    diagnostics: Vec<String>,
    task: Option<RunningTask>,
    next_run: Instant,
    pending_poll: bool,
    failure_count: u32,
}

impl ScriptBundleState {
    fn new(manifest: ScriptManifest, bundle_root: PathBuf, now: Instant) -> Self {
        Self {
            manifest,
            bundle_root,
            views: HashMap::new(),
            diagnostics: Vec::new(),
            task: None,
            next_run: now,
            pending_poll: false,
            failure_count: 0,
        }
    }

    fn stop(&mut self) {
        if let Some(task) = &mut self.task {
            task.cancel_and_join();
        }
        self.task = None;
    }

    fn push_diagnostic(&mut self, diagnostic: String) {
        if self.diagnostics.len() == MAX_DIAGNOSTICS_PER_BUNDLE {
            self.diagnostics.remove(0);
        }
        self.diagnostics.push(diagnostic);
    }
}

pub struct ScriptRuntime {
    paths: CatalogPaths,
    runner: Arc<dyn ProcessRunner>,
    clock: Arc<dyn ScriptClock>,
    bundles: BTreeMap<ExtensionId, ScriptBundleState>,
    source_diagnostics: Vec<String>,
    breaker: CircuitBreaker,
}

impl ScriptRuntime {
    pub fn new(paths: CatalogPaths) -> Self {
        Self::with_dependencies(
            paths,
            Arc::new(RealProcessRunner),
            Arc::new(SystemScriptClock),
        )
    }

    pub fn with_dependencies(
        paths: CatalogPaths,
        runner: Arc<dyn ProcessRunner>,
        clock: Arc<dyn ScriptClock>,
    ) -> Self {
        Self {
            paths,
            runner,
            clock,
            bundles: BTreeMap::new(),
            source_diagnostics: Vec::new(),
            breaker: CircuitBreaker::default(),
        }
    }

    pub fn shutdown(&mut self) {
        for state in self.bundles.values_mut() {
            state.stop();
        }
        self.bundles.clear();
    }

    pub fn reconcile(&mut self, active_wasm_ids: &[ExtensionId]) {
        self.source_diagnostics.clear();
        let scripts_dir = self.paths.config_dir.join("scripts");
        let mut candidates: BTreeMap<ExtensionId, Vec<(ScriptManifest, PathBuf)>> = BTreeMap::new();
        let mut invalid_roots = BTreeMap::<PathBuf, String>::new();

        if scripts_dir.is_dir() {
            match fs::read_dir(&scripts_dir) {
                Ok(entries) => {
                    let mut roots: Vec<PathBuf> = entries
                        .flatten()
                        .map(|entry| entry.path())
                        .filter(|path| path.is_dir())
                        .collect();
                    roots.sort();
                    for root in roots {
                        let manifest_path = root.join("manifest.toml");
                        if !manifest_path.is_file() {
                            continue;
                        }
                        match parse_and_validate_bundle(&manifest_path, &root) {
                            Ok(manifest) => candidates
                                .entry(manifest.id.clone())
                                .or_default()
                                .push((manifest, root)),
                            Err(error) => {
                                invalid_roots.insert(root.clone(), error.clone());
                                self.push_source_diagnostic(format!(
                                    "failed to load script bundle at '{}': {error}",
                                    root.display()
                                ));
                            }
                        }
                    }
                }
                Err(error) => self.push_source_diagnostic(format!(
                    "failed to read script source '{}': {error}",
                    scripts_dir.display()
                )),
            }
        }

        let now = self.clock.now();
        let existing_ids: Vec<ExtensionId> = self.bundles.keys().cloned().collect();
        for id in existing_ids {
            let Some(existing_root) = self.bundles.get(&id).map(|state| state.bundle_root.clone())
            else {
                continue;
            };
            let has_valid_candidate = candidates.contains_key(&id);
            let invalid_replacement = invalid_roots.contains_key(&existing_root);
            if !has_valid_candidate
                && !invalid_replacement
                && !existing_root.exists()
                && let Some(mut removed) = self.bundles.remove(&id)
            {
                removed.stop();
                self.breaker.reset(&id);
            }
        }

        for (id, mut entries) in candidates {
            if active_wasm_ids.contains(&id) {
                let paths = entries
                    .iter()
                    .map(|(_, path)| path.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ");
                self.push_source_diagnostic(format!(
                    "script extension '{id}' from [{paths}] conflicts with an active WASM/catalog source; the script source is disabled"
                ));
                if let Some(mut removed) = self.bundles.remove(&id) {
                    removed.stop();
                }
                continue;
            }
            if entries.len() > 1 {
                let paths = entries
                    .iter()
                    .map(|(_, path)| path.display().to_string())
                    .collect::<Vec<_>>()
                    .join(" and ");
                self.push_source_diagnostic(format!(
                    "duplicate script extension ID '{id}' declared by {paths}; both candidates are disabled"
                ));
                // Preserve an already-running last-valid source, but never choose a new winner.
                continue;
            }
            let (manifest, root) = entries.pop().expect("one candidate remains");
            match self.bundles.get_mut(&id) {
                Some(state) if state.manifest == manifest && state.bundle_root == root => {}
                Some(state) => {
                    // The replacement is fully parsed and validated. Keep the previous views until
                    // the new generation publishes its first valid record.
                    let retained_views = std::mem::take(&mut state.views);
                    state.stop();
                    let mut replacement = ScriptBundleState::new(manifest, root, now);
                    replacement.views = retained_views;
                    *state = replacement;
                    self.breaker.reset(&id);
                }
                None => {
                    self.bundles
                        .insert(id, ScriptBundleState::new(manifest, root, now));
                }
            }
        }
        self.tick();
    }

    /// Advances script scheduling without blocking on child process I/O.
    /// Returns true when a snapshot-visible view, status, or diagnostic changed.
    pub fn tick(&mut self) -> bool {
        let now = self.clock.now();
        let mut changed = false;
        let ids: Vec<ExtensionId> = self.bundles.keys().cloned().collect();
        for id in ids {
            changed |= self.drain_task_events(&id, now);
            let should_start = self.bundles.get(&id).is_some_and(|state| {
                state.task.is_none() && now >= state.next_run && !self.breaker.is_tripped(&id)
            });
            if should_start {
                self.start_task(&id, now);
                changed = true;
            } else if let Some(state) = self.bundles.get_mut(&id)
                && state.task.is_some()
                && state.manifest.runtime.mode == ScriptMode::Poll
                && now >= state.next_run
            {
                state.pending_poll = true;
            }
        }
        changed
    }

    fn drain_task_events(&mut self, id: &ExtensionId, now: Instant) -> bool {
        let mut events = Vec::new();
        if let Some(state) = self.bundles.get_mut(id)
            && let Some(task) = &mut state.task
        {
            while let Ok(event) = task.events.try_recv() {
                events.push(event);
            }
        }
        let mut changed = false;
        for event in events {
            changed = true;
            match event {
                TaskEvent::PollFinished(result) => {
                    self.finish_task(id);
                    match result {
                        Ok(output) if output.exit_code == 0 => {
                            match single_poll_record(&output.stdout) {
                                Ok(record) => self.accept_record(id, record),
                                Err(error) => self.record_failure(id, error, now),
                            }
                        }
                        Ok(output) => self.record_failure(
                            id,
                            format!(
                                "script exited with code {}: {}",
                                output.exit_code,
                                sanitize_excerpt(&output.stderr)
                            ),
                            now,
                        ),
                        Err(ScriptProcessError::Cancelled) => {}
                        Err(error) => self.record_failure(id, error.to_string(), now),
                    }
                    if let Some(state) = self.bundles.get_mut(id) {
                        let interval = Duration::from_millis(
                            state.manifest.runtime.interval_ms.unwrap_or(1_000),
                        );
                        state.next_run = if state.pending_poll {
                            now
                        } else {
                            now + interval
                        };
                        state.pending_poll = false;
                    }
                }
                TaskEvent::StreamRecord(record) => self.accept_record(id, &record),
                TaskEvent::StreamStopped(result) => {
                    self.finish_task(id);
                    match result {
                        Ok(stderr) => self.record_failure(
                            id,
                            format!(
                                "stream reached EOF: {}",
                                sanitize_excerpt(stderr.as_bytes())
                            ),
                            now,
                        ),
                        Err(ScriptProcessError::Cancelled) => {}
                        Err(error) => self.record_failure(id, error.to_string(), now),
                    }
                }
            }
        }
        changed
    }

    fn start_task(&mut self, id: &ExtensionId, now: Instant) {
        let Some(state) = self.bundles.get_mut(id) else {
            return;
        };
        let executable = state.bundle_root.join(&state.manifest.runtime.executable);
        let args = state.manifest.runtime.args.clone();
        let cwd = state.bundle_root.clone();
        let timeout = Duration::from_millis(state.manifest.runtime.timeout_ms);
        let mode = state.manifest.runtime.mode;
        let runner = self.runner.clone();
        let cancelled = Arc::new(AtomicBool::new(false));
        let task_cancelled = cancelled.clone();
        let (sender, receiver) = mpsc::sync_channel(64);
        let join = thread::spawn(move || match mode {
            ScriptMode::Poll => {
                let result = runner.run_poll(&executable, &args, &cwd, timeout, task_cancelled);
                let _ = sender.send(TaskEvent::PollFinished(result));
            }
            ScriptMode::Stream => match runner.spawn_stream(&executable, &args, &cwd) {
                Ok(mut process) => loop {
                    match process.next_line(timeout, &task_cancelled) {
                        Ok(Some(record)) => {
                            if sender.try_send(TaskEvent::StreamRecord(record)).is_err() {
                                let _ = process.kill_group();
                                break;
                            }
                        }
                        Ok(None) => {
                            let stderr = process.stderr_excerpt();
                            let _ = process.kill_group();
                            let _ = sender.send(TaskEvent::StreamStopped(Ok(stderr)));
                            break;
                        }
                        Err(error) => {
                            let _ = process.kill_group();
                            let _ = sender.send(TaskEvent::StreamStopped(Err(error)));
                            break;
                        }
                    }
                },
                Err(error) => {
                    let _ = sender.send(TaskEvent::StreamStopped(Err(error)));
                }
            },
        });
        state.task = Some(RunningTask {
            cancelled,
            events: receiver,
            join: Some(join),
        });
        state.next_run = match mode {
            ScriptMode::Poll => {
                now + Duration::from_millis(state.manifest.runtime.interval_ms.unwrap_or(1_000))
            }
            ScriptMode::Stream => now,
        };
    }

    fn finish_task(&mut self, id: &ExtensionId) {
        if let Some(state) = self.bundles.get_mut(id)
            && let Some(mut task) = state.task.take()
            && let Some(join) = task.join.take()
        {
            let _ = join.join();
        }
    }

    fn accept_record(&mut self, id: &ExtensionId, record: &[u8]) {
        let result = self.bundles.get(id).map(|state| {
            decode_and_validate_record(record, &state.manifest)
                .map(|(contribution, view)| (CanonicalId::new(id.clone(), contribution), view))
        });
        match result {
            Some(Ok((canonical, view))) => {
                if let Some(state) = self.bundles.get_mut(id) {
                    state.views.insert(canonical, view);
                    state.failure_count = 0;
                }
                self.breaker.record_success(id);
            }
            Some(Err(error)) => self.record_failure(id, error, self.clock.now()),
            None => {}
        }
    }

    fn record_failure(&mut self, id: &ExtensionId, message: String, now: Instant) {
        if let Some(state) = self.bundles.get_mut(id) {
            state.failure_count = state.failure_count.saturating_add(1);
            state.push_diagnostic(message.clone());
            state.next_run = now + failure_backoff(state.failure_count);
        }
        let tripped = self
            .breaker
            .record_failure(id, DiagnosticCode::InvalidOutput, message);
        if tripped && let Some(state) = self.bundles.get_mut(id) {
            state.stop();
        }
    }

    fn push_source_diagnostic(&mut self, diagnostic: String) {
        if self.source_diagnostics.len() == MAX_SOURCE_DIAGNOSTICS {
            self.source_diagnostics.remove(0);
        }
        self.source_diagnostics.push(diagnostic);
    }

    pub fn descriptors(&self) -> Vec<ContributionDescriptor> {
        self.bundles
            .iter()
            .flat_map(|(extension_id, state)| {
                state
                    .manifest
                    .contributions
                    .bar_widgets
                    .iter()
                    .map(move |widget| ContributionDescriptor {
                        id: CanonicalId::new(extension_id.clone(), widget.id.clone()),
                        extension_name: state.manifest.name.clone(),
                        name: widget.name.clone(),
                        surface: ContributionSurface::Bar,
                        runtime_kind: ExtensionRuntimeKind::TrustedLocalScript,
                        settings_schema: None,
                        default_size: None,
                        minimum_size: None,
                        bar_widget: None,
                        action: None,
                        default_binding: None,
                        wallpaper_modes: None,
                        wallpaper_targets: None,
                    })
            })
            .collect()
    }

    pub fn views(&self) -> HashMap<CanonicalId, ViewTree> {
        self.bundles
            .values()
            .flat_map(|state| state.views.clone())
            .collect()
    }

    pub fn diagnostics(&self) -> Vec<String> {
        let mut diagnostics = self.source_diagnostics.clone();
        for (id, state) in &self.bundles {
            diagnostics.extend(
                state
                    .diagnostics
                    .iter()
                    .map(|message| format!("script[{id}]: {message}")),
            );
        }
        diagnostics
    }

    pub fn statuses(&self) -> Vec<ScriptExtensionStatus> {
        self.bundles
            .iter()
            .map(|(id, state)| ScriptExtensionStatus {
                id: id.clone(),
                name: state.manifest.name.clone(),
                version: state.manifest.version.to_string(),
                source: "local".into(),
                status: if self.breaker.is_tripped(id) {
                    "disabled_for_session"
                } else if state.task.is_some() {
                    "running"
                } else if state.views.is_empty() {
                    "starting"
                } else {
                    "ready"
                }
                .into(),
                contributions_count: state.manifest.contributions.bar_widgets.len(),
                diagnostics: state.diagnostics.clone(),
            })
            .collect()
    }

    pub fn asset_roots(&self) -> BTreeMap<ExtensionId, PathBuf> {
        self.bundles
            .iter()
            .map(|(id, state)| (id.clone(), state.bundle_root.clone()))
            .collect()
    }
}

impl Drop for ScriptRuntime {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn parse_and_validate_bundle(
    manifest_path: &Path,
    bundle_root: &Path,
) -> Result<ScriptManifest, String> {
    let source = fs::read_to_string(manifest_path)
        .map_err(|error| format!("failed to read manifest: {error}"))?;
    let manifest = ScriptManifest::from_toml(&source).map_err(|error| error.to_string())?;
    manifest
        .validate(bundle_root)
        .map_err(|error| error.to_string())?;
    Ok(manifest)
}

fn single_poll_record(stdout: &[u8]) -> Result<&[u8], String> {
    if stdout.len() > super::record::MAX_RECORD_BYTES + 1 {
        return Err("poll output exceeds the 1 MiB record limit".into());
    }
    let records: Vec<&[u8]> = stdout
        .split(|byte| *byte == b'\n')
        .map(|record| record.trim_ascii())
        .filter(|record| !record.is_empty())
        .collect();
    match records.as_slice() {
        [record] => Ok(record),
        [] => Err("polling script produced no output record".into()),
        _ => Err("polling script produced multiple output records".into()),
    }
}

fn failure_backoff(failure_count: u32) -> Duration {
    match failure_count {
        1 => Duration::from_millis(250),
        2 => Duration::from_secs(1),
        _ => Duration::from_secs(4),
    }
}

fn sanitize_excerpt(bytes: &[u8]) -> String {
    let excerpt = &bytes[..bytes.len().min(super::runner::MAX_STDERR_BYTES)];
    String::from_utf8_lossy(excerpt)
        .chars()
        .map(|character| {
            if character.is_control() && !matches!(character, '\n' | '\t') {
                '\u{fffd}'
            } else {
                character
            }
        })
        .collect()
}

pub fn discover_script_bundles(paths: &CatalogPaths) -> Vec<ScriptBundleInfo> {
    let scripts_dir = paths.config_dir.join("scripts");
    let Ok(entries) = fs::read_dir(scripts_dir) else {
        return Vec::new();
    };
    let mut roots: Vec<PathBuf> = entries.flatten().map(|entry| entry.path()).collect();
    roots.sort();
    roots
        .into_iter()
        .filter(|path| path.is_dir())
        .filter_map(|path| {
            let manifest_path = path.join("manifest.toml");
            let source = fs::read_to_string(&manifest_path).ok()?;
            let manifest = ScriptManifest::from_toml(&source).ok()?;
            let diagnostics = manifest
                .validate(&path)
                .err()
                .map(|error| vec![error.to_string()])
                .unwrap_or_default();
            Some(ScriptBundleInfo {
                id: manifest.id,
                name: manifest.name,
                version: manifest.version.to_string(),
                path,
                mode: manifest.runtime.mode,
                contributions_count: manifest.contributions.bar_widgets.len(),
                status: if diagnostics.is_empty() {
                    "discovered"
                } else {
                    "invalid"
                }
                .into(),
                diagnostics,
            })
        })
        .collect()
}
