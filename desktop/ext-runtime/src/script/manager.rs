use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use shilpo_ext_api::{CanonicalId, ExtensionId, ViewTree};

use crate::CatalogPaths;
use crate::worker::protocol::{ContributionDescriptor, ContributionSurface, ExtensionRuntimeKind};

use super::manifest::{ScriptManifest, ScriptMode};
use super::record::decode_and_validate_record;
use super::runner::{ProcessRunner, RealProcessRunner};

#[derive(Clone, Debug)]
pub struct ScriptBundleInfo {
    pub id: ExtensionId,
    pub name: String,
    pub version: String,
    pub path: PathBuf,
    pub mode: ScriptMode,
    pub contributions_count: usize,
    pub diagnostics: Vec<String>,
}

pub struct ScriptBundleState {
    pub manifest: ScriptManifest,
    pub bundle_root: PathBuf,
    pub views: HashMap<CanonicalId, ViewTree>,
    pub diagnostics: Vec<String>,
    pub last_poll_run: Option<Instant>,
    pub poll_in_progress: bool,
    pub stream_process: Option<Box<dyn super::runner::StreamProcess>>,
    pub stream_fail_count: u32,
    pub next_stream_restart: Option<Instant>,
}

impl ScriptBundleState {
    pub fn new(manifest: ScriptManifest, bundle_root: PathBuf) -> Self {
        Self {
            manifest,
            bundle_root,
            views: HashMap::new(),
            diagnostics: Vec::new(),
            last_poll_run: None,
            poll_in_progress: false,
            stream_process: None,
            stream_fail_count: 0,
            next_stream_restart: None,
        }
    }

    pub fn shutdown(&mut self) {
        if let Some(ref mut process) = self.stream_process {
            let _ = process.kill_group();
        }
        self.stream_process = None;
    }
}

pub struct ScriptRuntime {
    paths: CatalogPaths,
    runner: Arc<dyn ProcessRunner>,
    bundles: BTreeMap<ExtensionId, ScriptBundleState>,
    diagnostics: Vec<String>,
}

impl ScriptRuntime {
    pub fn new(paths: CatalogPaths) -> Self {
        Self::with_runner(paths, Arc::new(RealProcessRunner))
    }

    pub fn with_runner(paths: CatalogPaths, runner: Arc<dyn ProcessRunner>) -> Self {
        Self {
            paths,
            runner,
            bundles: BTreeMap::new(),
            diagnostics: Vec::new(),
        }
    }

    pub fn shutdown(&mut self) {
        for state in self.bundles.values_mut() {
            state.shutdown();
        }
        self.bundles.clear();
    }

    #[allow(clippy::collapsible_if)]
    pub fn reconcile(&mut self, active_wasm_ids: &[ExtensionId]) -> Result<(), String> {
        let scripts_dir = self.paths.config_dir.join("scripts");
        let mut discovered: BTreeMap<ExtensionId, (ScriptManifest, PathBuf, Vec<String>)> =
            BTreeMap::new();

        if scripts_dir.is_dir() {
            if let Ok(entries) = fs::read_dir(&scripts_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        let manifest_path = path.join("manifest.toml");
                        if manifest_path.is_file() {
                            match parse_and_validate_bundle(&manifest_path, &path) {
                                Ok((manifest, diags)) => {
                                    let id = manifest.id.clone();
                                    if active_wasm_ids.contains(&id) {
                                        self.diagnostics.push(format!(
                                            "script bundle '{}' at '{}' conflicts with active WASM extension ID",
                                            id,
                                            path.display()
                                        ));
                                        continue;
                                    }
                                    if discovered.contains_key(&id) {
                                        self.diagnostics.push(format!(
                                            "script bundle '{}' at '{}' duplicate of another script bundle",
                                            id,
                                            path.display()
                                        ));
                                        continue;
                                    }
                                    discovered.insert(id, (manifest, path, diags));
                                }
                                Err(err) => {
                                    self.diagnostics.push(format!(
                                        "failed to load script bundle at '{}': {err}",
                                        path.display()
                                    ));
                                }
                            }
                        }
                    }
                }
            }
        }

        let existing_ids: Vec<ExtensionId> = self.bundles.keys().cloned().collect();
        for id in existing_ids {
            if !discovered.contains_key(&id) {
                if let Some(mut state) = self.bundles.remove(&id) {
                    state.shutdown();
                }
            }
        }

        for (id, (manifest, root, diags)) in discovered {
            if let Some(state) = self.bundles.get_mut(&id) {
                if state.manifest != manifest || state.bundle_root != root {
                    state.shutdown();
                    let mut new_state = ScriptBundleState::new(manifest, root);
                    new_state.diagnostics = diags;
                    self.bundles.insert(id, new_state);
                } else {
                    state.diagnostics = diags;
                }
            } else {
                let mut new_state = ScriptBundleState::new(manifest, root);
                new_state.diagnostics = diags;
                self.bundles.insert(id, new_state);
            }
        }

        self.tick_execution();
        Ok(())
    }

    pub fn force_poll_tick(&mut self) {
        for state in self.bundles.values_mut() {
            state.last_poll_run = None;
        }
        self.tick_execution();
    }

    #[allow(clippy::collapsible_if)]
    pub fn tick_execution(&mut self) {
        let now = Instant::now();
        for (ext_id, state) in &mut self.bundles {
            match state.manifest.runtime.mode {
                ScriptMode::Poll => {
                    let interval =
                        Duration::from_millis(state.manifest.runtime.interval_ms.unwrap_or(5000));
                    let should_run = match state.last_poll_run {
                        None => true,
                        Some(last) => now.duration_since(last) >= interval,
                    };
                    if should_run && !state.poll_in_progress {
                        state.poll_in_progress = true;
                        state.last_poll_run = Some(now);
                        let timeout = Duration::from_millis(state.manifest.runtime.timeout_ms);
                        let exec = state.bundle_root.join(&state.manifest.runtime.executable);
                        let args = state.manifest.runtime.args.clone();
                        let cwd = state.bundle_root.clone();

                        match self.runner.run_poll(&exec, &args, &cwd, timeout) {
                            Ok(output) => {
                                if output.exit_code == 0 {
                                    let stdout_trimmed = trim_json_record(&output.stdout);
                                    match decode_and_validate_record(
                                        stdout_trimmed,
                                        &state.manifest,
                                    ) {
                                        Ok((contrib_id, view_tree)) => {
                                            let canonical =
                                                CanonicalId::new(ext_id.clone(), contrib_id);
                                            state.views.insert(canonical, view_tree);
                                        }
                                        Err(e) => {
                                            state
                                                .diagnostics
                                                .push(format!("record decode error: {e}"));
                                        }
                                    }
                                } else {
                                    let stderr_str = String::from_utf8_lossy(&output.stderr);
                                    state.diagnostics.push(format!(
                                        "script exited with code {}: {}",
                                        output.exit_code, stderr_str
                                    ));
                                }
                            }
                            Err(err) => {
                                state.diagnostics.push(format!("poll run error: {err}"));
                            }
                        }
                        state.poll_in_progress = false;
                    }
                }
                ScriptMode::Stream => {
                    if let Some(restart_at) = state.next_stream_restart {
                        if now < restart_at {
                            continue;
                        }
                    }
                    if state.stream_process.is_none() {
                        let exec = state.bundle_root.join(&state.manifest.runtime.executable);
                        let args = state.manifest.runtime.args.clone();
                        let cwd = state.bundle_root.clone();
                        match self.runner.spawn_stream(&exec, &args, &cwd) {
                            Ok(proc) => {
                                state.stream_process = Some(proc);
                                state.next_stream_restart = None;
                            }
                            Err(err) => {
                                state.diagnostics.push(format!("stream spawn error: {err}"));
                                state.stream_fail_count += 1;
                                let backoff = stream_backoff_duration(state.stream_fail_count);
                                state.next_stream_restart = Some(now + backoff);
                                continue;
                            }
                        }
                    }

                    if let Some(ref mut proc) = state.stream_process {
                        let timeout = Duration::from_millis(state.manifest.runtime.timeout_ms);
                        match proc.next_line(timeout) {
                            Ok(Some(line)) => {
                                let trimmed = line.trim();
                                if !trimmed.is_empty() {
                                    match decode_and_validate_record(
                                        trimmed.as_bytes(),
                                        &state.manifest,
                                    ) {
                                        Ok((contrib_id, view_tree)) => {
                                            let canonical =
                                                CanonicalId::new(ext_id.clone(), contrib_id);
                                            state.views.insert(canonical, view_tree);
                                        }
                                        Err(e) => {
                                            state
                                                .diagnostics
                                                .push(format!("stream line decode error: {e}"));
                                        }
                                    }
                                }
                            }
                            Ok(None) => {
                                let stderr = proc.stderr_excerpt();
                                let _ = proc.kill_group();
                                state.stream_process = None;
                                state
                                    .diagnostics
                                    .push(format!("stream EOF reached: {stderr}"));
                                state.stream_fail_count += 1;
                                let backoff = stream_backoff_duration(state.stream_fail_count);
                                state.next_stream_restart = Some(now + backoff);
                            }
                            Err(err) => {
                                let stderr = proc.stderr_excerpt();
                                let _ = proc.kill_group();
                                state.stream_process = None;
                                state
                                    .diagnostics
                                    .push(format!("stream read error '{err}': {stderr}"));
                                state.stream_fail_count += 1;
                                let backoff = stream_backoff_duration(state.stream_fail_count);
                                state.next_stream_restart = Some(now + backoff);
                            }
                        }
                    }
                }
            }
        }
    }

    pub fn descriptors(&self) -> Vec<ContributionDescriptor> {
        let mut list = Vec::new();
        for (ext_id, state) in &self.bundles {
            for widget in &state.manifest.contributions.bar_widgets {
                list.push(ContributionDescriptor {
                    id: CanonicalId::new(ext_id.clone(), widget.id.clone()),
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
                });
            }
        }
        list
    }

    pub fn views(&self) -> HashMap<CanonicalId, ViewTree> {
        let mut map = HashMap::new();
        for state in self.bundles.values() {
            for (k, v) in &state.views {
                map.insert(k.clone(), v.clone());
            }
        }
        map
    }

    pub fn diagnostics(&self) -> Vec<String> {
        let mut list = self.diagnostics.clone();
        for (id, state) in &self.bundles {
            for diag in &state.diagnostics {
                list.push(format!("script[{id}]: {diag}"));
            }
        }
        list
    }

    pub fn asset_roots(&self) -> BTreeMap<ExtensionId, PathBuf> {
        let mut map = BTreeMap::new();
        for (id, state) in &self.bundles {
            map.insert(id.clone(), state.bundle_root.clone());
        }
        map
    }
}

fn parse_and_validate_bundle(
    manifest_path: &Path,
    bundle_root: &Path,
) -> Result<(ScriptManifest, Vec<String>), String> {
    let toml_str =
        fs::read_to_string(manifest_path).map_err(|e| format!("failed to read manifest: {e}"))?;
    let manifest = ScriptManifest::from_toml(&toml_str)?;
    manifest.validate(bundle_root)?;
    Ok((manifest, Vec::new()))
}

fn stream_backoff_duration(fail_count: u32) -> Duration {
    match fail_count {
        1 => Duration::from_millis(250),
        2 => Duration::from_millis(1000),
        _ => Duration::from_millis(4000),
    }
}

fn trim_json_record(bytes: &[u8]) -> &[u8] {
    let trimmed = bytes.trim_ascii();
    if let Some(pos) = trimmed.iter().position(|&b| b == b'\n') {
        &trimmed[..pos]
    } else {
        trimmed
    }
}

#[allow(clippy::collapsible_if)]
pub fn discover_script_bundles(paths: &CatalogPaths) -> Vec<ScriptBundleInfo> {
    let scripts_dir = paths.config_dir.join("scripts");
    let mut list = Vec::new();
    if scripts_dir.is_dir() {
        if let Ok(entries) = fs::read_dir(&scripts_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    let manifest_path = path.join("manifest.toml");
                    if manifest_path.is_file() {
                        if let Ok(toml_str) = fs::read_to_string(&manifest_path) {
                            if let Ok(manifest) = ScriptManifest::from_toml(&toml_str) {
                                let diags = match manifest.validate(&path) {
                                    Ok(()) => Vec::new(),
                                    Err(e) => vec![format!("validation error: {e}")],
                                };
                                list.push(ScriptBundleInfo {
                                    id: manifest.id,
                                    name: manifest.name,
                                    version: manifest.version,
                                    path,
                                    mode: manifest.runtime.mode,
                                    contributions_count: manifest.contributions.bar_widgets.len(),
                                    diagnostics: diags,
                                });
                            }
                        }
                    }
                }
            }
        }
    }
    list
}
