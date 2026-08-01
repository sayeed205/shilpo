use super::coordinator::{
    ContributionDescriptor, ContributionInstance, ContributionSurface, ExtensionChanges,
    ExtensionCommand, ExtensionGeneration, ExtensionSnapshot, ExtensionUpdate, ReplaceableEvent,
};
use semver::Version;
use shilpo_ext::{
    AuthorizedHostEffectKind, CURRENT_SHILPO_VERSION, CanonicalId, Capability, CatalogPaths,
    ContributionId, ExtensionCatalog, ExtensionEvent, ExtensionHost, ExtensionId,
    ExtensionManifest, ExtensionRuntime, HostEffect, ViewTree, WasmModule,
};
use std::{
    collections::{BTreeMap, HashMap},
    fs,
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
    sync::{Arc, RwLock, mpsc},
    time::{Duration, Instant, UNIX_EPOCH},
};

const DEBOUNCE_DURATION: Duration = Duration::from_millis(200);

#[derive(Clone, Debug)]
pub struct ActiveSource {
    pub manifest: ExtensionManifest,
    pub root: PathBuf,
    pub grants: Vec<Capability>,
    pub fingerprint: u64,
}

pub struct ExtensionSession<R> {
    pub host: ExtensionHost<R>,
    pub manifests: BTreeMap<ExtensionId, ExtensionManifest>,
    pub runtime_ids: Vec<ExtensionId>,
    pub views: HashMap<CanonicalId, ViewTree>,
    pub instances: HashMap<String, ContributionInstance>,
    pub state: HashMap<(ExtensionId, String), serde_json::Value>,
}

impl<R: ExtensionRuntime> ExtensionSession<R> {
    pub fn new(runtime: R) -> Self {
        Self {
            host: ExtensionHost::new(runtime),
            manifests: BTreeMap::new(),
            runtime_ids: Vec::new(),
            views: HashMap::new(),
            instances: HashMap::new(),
            state: HashMap::new(),
        }
    }

    pub fn register(
        &mut self,
        manifest: ExtensionManifest,
        module: R::Module,
        grants: Vec<Capability>,
    ) -> Result<(), String> {
        let id = manifest.id.clone();
        self.host
            .register(manifest.clone(), module, grants)
            .map_err(|e| e.to_string())?;
        self.runtime_ids.push(id.clone());
        self.manifests.insert(id, manifest);
        Ok(())
    }

    pub fn replace(
        &mut self,
        manifest: ExtensionManifest,
        module: R::Module,
        grants: Vec<Capability>,
    ) -> Result<(), String> {
        let id = manifest.id.clone();
        self.host
            .replace(manifest.clone(), module, grants)
            .map_err(|e| e.to_string())?;
        self.manifests.insert(id, manifest);
        Ok(())
    }

    pub fn descriptors(&self) -> Vec<ContributionDescriptor> {
        let mut descriptors = Vec::new();
        for (extension_id, manifest) in &self.manifests {
            for widget in &manifest.contributions.bar_widgets {
                descriptors.push(ContributionDescriptor {
                    id: CanonicalId {
                        extension_id: extension_id.clone(),
                        contribution_id: widget.id.clone(),
                    },
                    extension_name: manifest.name.clone(),
                    name: widget.name.clone(),
                    surface: ContributionSurface::Bar,
                    settings_schema: None,
                    default_size: None,
                    minimum_size: None,
                });
            }
            for widget in &manifest.contributions.desktop_widgets {
                let default_size = match (widget.default_width, widget.default_height) {
                    (Some(w), Some(h)) => Some((w, h)),
                    _ => None,
                };
                let minimum_size = match (widget.min_width, widget.min_height) {
                    (Some(w), Some(h)) => Some((w, h)),
                    _ => None,
                };
                descriptors.push(ContributionDescriptor {
                    id: CanonicalId {
                        extension_id: extension_id.clone(),
                        contribution_id: widget.id.clone(),
                    },
                    extension_name: manifest.name.clone(),
                    name: widget.name.clone(),
                    surface: ContributionSurface::Desktop,
                    settings_schema: None,
                    default_size,
                    minimum_size,
                });
            }
            for page in &manifest.contributions.settings_pages {
                descriptors.push(ContributionDescriptor {
                    id: CanonicalId {
                        extension_id: extension_id.clone(),
                        contribution_id: page.id.clone(),
                    },
                    extension_name: manifest.name.clone(),
                    name: page.name.clone(),
                    surface: ContributionSurface::Settings,
                    settings_schema: Some(page.schema.clone()),
                    default_size: None,
                    minimum_size: None,
                });
            }
            for panel in &manifest.contributions.side_panels {
                descriptors.push(ContributionDescriptor {
                    id: CanonicalId {
                        extension_id: extension_id.clone(),
                        contribution_id: panel.id.clone(),
                    },
                    extension_name: manifest.name.clone(),
                    name: panel.name.clone(),
                    surface: ContributionSurface::SidePanel,
                    settings_schema: None,
                    default_size: None,
                    minimum_size: None,
                });
            }
            for cc in &manifest.contributions.control_center {
                descriptors.push(ContributionDescriptor {
                    id: CanonicalId {
                        extension_id: extension_id.clone(),
                        contribution_id: cc.id.clone(),
                    },
                    extension_name: manifest.name.clone(),
                    name: cc.name.clone(),
                    surface: ContributionSurface::ControlCenter,
                    settings_schema: None,
                    default_size: None,
                    minimum_size: None,
                });
            }
            for launcher in &manifest.contributions.launcher_providers {
                descriptors.push(ContributionDescriptor {
                    id: CanonicalId {
                        extension_id: extension_id.clone(),
                        contribution_id: launcher.id.clone(),
                    },
                    extension_name: manifest.name.clone(),
                    name: launcher.name.clone(),
                    surface: ContributionSurface::Launcher,
                    settings_schema: None,
                    default_size: None,
                    minimum_size: None,
                });
            }
            for action in &manifest.contributions.actions {
                descriptors.push(ContributionDescriptor {
                    id: CanonicalId {
                        extension_id: extension_id.clone(),
                        contribution_id: action.id.clone(),
                    },
                    extension_name: manifest.name.clone(),
                    name: action.name.clone(),
                    surface: ContributionSurface::Action,
                    settings_schema: None,
                    default_size: None,
                    minimum_size: None,
                });
            }
            for task in &manifest.contributions.background_tasks {
                descriptors.push(ContributionDescriptor {
                    id: CanonicalId {
                        extension_id: extension_id.clone(),
                        contribution_id: task.id.clone(),
                    },
                    extension_name: manifest.name.clone(),
                    name: task.name.clone(),
                    surface: ContributionSurface::Background,
                    settings_schema: None,
                    default_size: None,
                    minimum_size: None,
                });
            }
        }
        descriptors
    }

    pub fn view(&mut self, id: &CanonicalId) -> Option<ViewTree> {
        let tree = self.host.render_view(id).ok()??;
        self.views.insert(id.clone(), tree.clone());
        Some(tree)
    }

    pub fn view_cached_or_render(&mut self, id: &CanonicalId) -> Option<ViewTree> {
        if let Ok(Some(tree)) = self.host.render_view(id) {
            self.views.insert(id.clone(), tree.clone());
            Some(tree)
        } else {
            self.views.get(id).cloned()
        }
    }

    pub fn input(
        &mut self,
        contribution: &CanonicalId,
        instance_id: Option<&str>,
        event_id: impl Into<String>,
        value: Option<serde_json::Value>,
    ) -> ExtensionChanges {
        let event = ExtensionEvent::Input {
            contribution_id: contribution.contribution_id.to_string(),
            instance_id: instance_id.map(ToString::to_string),
            event_id: event_id.into(),
            value,
        };
        self.dispatch(&contribution.extension_id, &event)
    }

    pub fn dispatch(
        &mut self,
        extension: &ExtensionId,
        event: &ExtensionEvent,
    ) -> ExtensionChanges {
        let mut changes = ExtensionChanges::default();
        if let Ok(result) = self.host.dispatch_event(extension, event) {
            for effect in result.accepted {
                match effect.kind() {
                    AuthorizedHostEffectKind::NonHttp(HostEffect::InvalidateView {
                        contribution_id,
                    }) => {
                        // Invalidation is a shell-coordination signal, not a
                        // service effect. Convert it to the canonical view id
                        // here so the coordinator can publish a fresh tree and
                        // repaint the owning surface.
                        if let Ok(contribution_id) = ContributionId::new(contribution_id.clone()) {
                            changes.invalidated_views.push(CanonicalId {
                                extension_id: extension.clone(),
                                contribution_id,
                            });
                        }
                    }
                    _ => changes.effects.push((extension.clone(), effect)),
                }
            }
        }
        changes
    }

    pub fn dispatch_all(&mut self, event: &ExtensionEvent) -> ExtensionChanges {
        let mut changes = ExtensionChanges::default();
        let runtime_ids = self.runtime_ids.clone();
        for id in runtime_ids {
            changes.merge(self.dispatch(&id, event));
        }
        changes
    }

    pub fn reconcile_instances(
        &mut self,
        desired: impl IntoIterator<Item = ContributionInstance>,
    ) -> ExtensionChanges {
        let mut next = HashMap::new();
        let mut changes = ExtensionChanges::default();
        for instance in desired {
            let (output_changed, resized, settings_changed, is_new) =
                match self.instances.get(&instance.id) {
                    Some(existing) => (
                        existing.output != instance.output,
                        existing.width != instance.width || existing.height != instance.height,
                        existing.settings != instance.settings,
                        false,
                    ),
                    None => (false, false, true, true),
                };

            if output_changed {
                changes.merge(self.input(
                    &instance.contribution,
                    Some(&instance.id),
                    "output-changed",
                    instance.output.clone().map(Into::into),
                ));
            }
            if resized {
                changes.merge(self.input(
                    &instance.contribution,
                    Some(&instance.id),
                    "resized",
                    Some(serde_json::json!({
                        "width": instance.width,
                        "height": instance.height,
                    })),
                ));
            }
            if is_new {
                changes.merge(self.input(
                    &instance.contribution,
                    Some(&instance.id),
                    "mounted",
                    Some(serde_json::json!({
                        "width": instance.width,
                        "height": instance.height,
                        "output": instance.output,
                        "settings": instance.settings,
                    })),
                ));
                changes.merge(self.dispatch(
                    &instance.contribution.extension_id,
                    &ExtensionEvent::ContributionSettingsChanged {
                        contribution_id: instance.contribution.contribution_id.to_string(),
                        instance_id: Some(instance.id.clone()),
                        settings: instance.settings.clone(),
                    },
                ));
            }
            if settings_changed {
                changes.merge(self.dispatch(
                    &instance.contribution.extension_id,
                    &ExtensionEvent::ContributionSettingsChanged {
                        contribution_id: instance.contribution.contribution_id.to_string(),
                        instance_id: Some(instance.id.clone()),
                        settings: instance.settings.clone(),
                    },
                ));
            }
            next.insert(instance.id.clone(), instance);
        }

        let existing_keys: Vec<_> = self.instances.keys().cloned().collect();
        for id in existing_keys {
            if !next.contains_key(&id)
                && let Some(existing) = self.instances.remove(&id)
            {
                changes.merge(self.input(&existing.contribution, Some(&id), "unmounted", None));
            }
        }

        self.instances = next;
        changes
    }

    pub fn build_snapshot(
        &mut self,
        generation: ExtensionGeneration,
        diagnostics: Arc<[String]>,
        catalog_changed_at: Option<ExtensionGeneration>,
        sources: &HashMap<ExtensionId, ActiveSource>,
    ) -> ExtensionSnapshot {
        let descriptors: Arc<[ContributionDescriptor]> = self.descriptors().into();
        let mut views_map = BTreeMap::new();
        for descriptor in descriptors.iter() {
            if let Some(view_tree) = self.view_cached_or_render(&descriptor.id) {
                views_map.insert(descriptor.id.clone(), view_tree);
            }
        }

        let mut settings_schemas = BTreeMap::new();
        let mut asset_roots = BTreeMap::new();

        for descriptor in descriptors.iter() {
            if let Some(source) = sources.get(&descriptor.id.extension_id) {
                asset_roots.insert(descriptor.id.extension_id.clone(), source.root.clone());
                if let Some(ref schema_rel) = descriptor.settings_schema
                    && let Ok(path) = super::coordinator::safe_child(&source.root, schema_rel)
                    && let Ok(Ok(val)) =
                        fs::read_to_string(&path).map(|content| serde_json::from_str(&content))
                {
                    settings_schemas.insert(descriptor.id.clone(), val);
                }
            }
        }

        ExtensionSnapshot {
            generation,
            descriptors,
            views: Arc::new(views_map),
            diagnostics,
            catalog_changed_at,
            settings_schemas: Arc::new(settings_schemas),
            prevalidated_asset_roots: Arc::new(asset_roots),
        }
    }
}

pub struct ExtensionEngine<R> {
    _paths: CatalogPaths,
    catalog: ExtensionCatalog,
    session: ExtensionSession<R>,
    sources: HashMap<ExtensionId, ActiveSource>,
    diagnostics: Vec<String>,
    generation: ExtensionGeneration,
    _last_timer_tick: Instant,
}

impl<R> ExtensionEngine<R>
where
    R: ExtensionRuntime<Module = WasmModule> + Send + 'static,
{
    pub fn new(runtime: R, paths: CatalogPaths) -> Result<Self, String> {
        let shilpo_version = Version::parse(CURRENT_SHILPO_VERSION)
            .map_err(|e| format!("invalid shilpo version: {e}"))?;
        let catalog = ExtensionCatalog::open(paths.clone(), shilpo_version);
        let session = ExtensionSession::new(runtime);
        let mut engine = Self {
            _paths: paths,
            catalog,
            session,
            sources: HashMap::new(),
            diagnostics: Vec::new(),
            generation: ExtensionGeneration(0),
            _last_timer_tick: Instant::now(),
        };
        let _ = engine.refresh(true);
        Ok(engine)
    }

    pub fn generation(&self) -> ExtensionGeneration {
        self.generation
    }

    pub fn build_snapshot(&mut self, catalog_changed: bool) -> ExtensionSnapshot {
        let cat_changed_at = if catalog_changed {
            Some(self.generation)
        } else {
            None
        };
        self.session.build_snapshot(
            self.generation,
            self.diagnostics.as_slice().into(),
            cat_changed_at,
            &self.sources,
        )
    }

    pub fn refresh(&mut self, force: bool) -> ExtensionChanges {
        let mut new_diagnostics = Vec::new();
        let mut next_sources = HashMap::new();

        if let Ok(dev_sources) = self.load_development_sources(&mut new_diagnostics) {
            for (id, source) in dev_sources {
                next_sources.insert(id, source);
            }
        }

        if let Ok(installed_sources) = self.load_installed_sources(&mut new_diagnostics) {
            for (id, source) in installed_sources {
                next_sources.entry(id).or_insert(source);
            }
        }

        let mut changed = force || self.sources.len() != next_sources.len();
        if !changed {
            for (id, next_source) in &next_sources {
                let current_source = self.sources.get(id);
                if current_source.map(|s| s.fingerprint) != Some(next_source.fingerprint) {
                    changed = true;
                    break;
                }
            }
        }

        self.diagnostics = new_diagnostics;
        if !changed {
            return ExtensionChanges::default();
        }

        let previous_sources = self.sources.clone();
        let current_ids: Vec<_> = self.session.runtime_ids.clone();
        for id in current_ids {
            if !next_sources.contains_key(&id) && self.session.host.unregister(&id).is_ok() {
                self.session.manifests.remove(&id);
                self.session.runtime_ids.retain(|item| item != &id);
            }
        }

        let mut failed = Vec::new();
        let candidate_sources: Vec<_> = next_sources
            .iter()
            .map(|(id, source)| (id.clone(), source.clone()))
            .collect();
        for (id, source) in candidate_sources {
            if self.session.manifests.contains_key(&id) {
                let current_fp = self.sources.get(&id).map(|s| s.fingerprint);
                if current_fp == Some(source.fingerprint) {
                    continue;
                }
                // Keep the current guest active until the replacement has been
                // fully loaded and validated by the runtime adapter.
            }

            let module_path = if let Some(ref lib) = source.manifest.library {
                source.root.join(&lib.path)
            } else {
                source.root.join("extension.wasm")
            };
            let module = match WasmModule::from_file(&module_path) {
                Ok(module) => module,
                Err(error) => {
                    self.diagnostics
                        .push(format!("failed to load guest module for '{id}': {error}"));
                    if let Some(previous) = previous_sources.get(&id) {
                        next_sources.insert(id.clone(), previous.clone());
                    } else {
                        failed.push(id.clone());
                    }
                    continue;
                }
            };

            let result = if self.session.manifests.contains_key(&id) {
                self.session
                    .replace(source.manifest.clone(), module, source.grants.clone())
            } else {
                self.session
                    .register(source.manifest.clone(), module, source.grants.clone())
            };
            if let Err(error) = result {
                self.diagnostics
                    .push(format!("failed to register guest '{id}': {error}"));
                if let Some(previous) = previous_sources.get(&id) {
                    next_sources.insert(id.clone(), previous.clone());
                } else {
                    failed.push(id.clone());
                }
            }
        }

        for id in failed {
            next_sources.remove(&id);
        }

        if !force
            && self.sources.len() == next_sources.len()
            && self.sources.iter().all(|(id, source)| {
                next_sources.get(id).map(|next| next.fingerprint) == Some(source.fingerprint)
            })
        {
            return ExtensionChanges::default();
        }

        self.sources = next_sources;
        self.generation = self.generation.next();

        let mut changes = self.session.dispatch_all(&ExtensionEvent::ShellStarted);
        changes.catalog_changed = true;
        changes
    }

    fn load_development_sources(
        &self,
        diagnostics: &mut Vec<String>,
    ) -> Result<HashMap<ExtensionId, ActiveSource>, String> {
        let mut sources = HashMap::new();
        let state_dir = shilpo_ext::default_extension_state_dir();
        let (registrations, reg_diags) = shilpo_ext::development_registrations(&state_dir);
        for diag in reg_diags {
            diagnostics.push(diag);
        }

        for reg in registrations {
            let root = reg.path.clone();
            let manifest_path = root.join("extension.toml");
            let manifest = match fs::read_to_string(&manifest_path) {
                Ok(content) => match ExtensionManifest::from_toml(&content) {
                    Ok(manifest) => manifest,
                    Err(error) => {
                        diagnostics.push(format!(
                            "invalid development manifest {}: {error}",
                            manifest_path.display()
                        ));
                        continue;
                    }
                },
                Err(error) => {
                    diagnostics.push(format!(
                        "failed to read development manifest {}: {error}",
                        manifest_path.display()
                    ));
                    continue;
                }
            };

            let fingerprint = fingerprint_path(&root);
            sources.insert(
                manifest.id.clone(),
                ActiveSource {
                    manifest: manifest.clone(),
                    root,
                    grants: manifest.capabilities.clone(),
                    fingerprint,
                },
            );
        }
        Ok(sources)
    }

    fn load_installed_sources(
        &self,
        diagnostics: &mut Vec<String>,
    ) -> Result<HashMap<ExtensionId, ActiveSource>, String> {
        let mut sources = HashMap::new();
        let snapshot = self.catalog.snapshot();

        for err in snapshot.diagnostics {
            diagnostics.push(err);
        }

        for installed in snapshot.installed {
            let root = installed.package_dir.clone();
            let fingerprint = fingerprint_path(&root);
            sources.insert(
                installed.manifest.id.clone(),
                ActiveSource {
                    manifest: installed.manifest.clone(),
                    root,
                    grants: installed.grants.granted_capabilities.clone(),
                    fingerprint,
                },
            );
        }
        Ok(sources)
    }

    pub fn handle_command(&mut self, command: ExtensionCommand) -> Option<ExtensionUpdate> {
        match command {
            ExtensionCommand::Lifecycle { expected, event } => {
                if expected != self.generation {
                    return None;
                }
                let changes = self.session.dispatch_all(&event);
                Some(self.build_update(changes, false))
            }
            ExtensionCommand::Input {
                expected,
                contribution,
                instance_id,
                event_id,
                value,
            } => {
                if expected != self.generation {
                    return None;
                }
                let changes =
                    self.session
                        .input(&contribution, instance_id.as_deref(), event_id, value);
                Some(self.build_update(changes, false))
            }
            ExtensionCommand::Response {
                expected,
                extension_id,
                event,
            } => {
                if expected != self.generation {
                    return None;
                }
                let changes = self.session.dispatch(&extension_id, &event);
                Some(self.build_update(changes, false))
            }
            ExtensionCommand::Replaceable(event) => {
                let changes = match event {
                    ReplaceableEvent::Power {
                        percentage,
                        charging,
                    } => self.session.dispatch_all(&ExtensionEvent::PowerChanged {
                        percentage,
                        charging,
                    }),
                    ReplaceableEvent::Network { connected } => self
                        .session
                        .dispatch_all(&ExtensionEvent::NetworkChanged { connected }),
                    ReplaceableEvent::Media {
                        title,
                        artist,
                        playing,
                    } => self.session.dispatch_all(&ExtensionEvent::MediaChanged {
                        title,
                        artist,
                        playing,
                    }),
                    ReplaceableEvent::TimerFired(name) => self
                        .session
                        .dispatch_all(&ExtensionEvent::TimerFired { name }),
                };
                Some(self.build_update(changes, false))
            }
            ExtensionCommand::ReconcileInstances { expected, desired } => {
                if expected != self.generation {
                    return None;
                }
                let changes = self.session.reconcile_instances(desired);
                Some(self.build_update(changes, false))
            }
            ExtensionCommand::SourcesChanged => {
                let changes = self.refresh(false);
                // Publish diagnostics and the coherent current snapshot even
                // when a source change was rejected and the catalog stayed the
                // same. This keeps failed reloads visible without advancing the
                // active generation.
                Some(self.build_update(changes, true))
            }
            ExtensionCommand::Shutdown { reply } => {
                let _ = self.session.dispatch_all(&ExtensionEvent::ShellStopping);
                let _ = reply.send(());
                None
            }
        }
    }

    fn build_update(&mut self, changes: ExtensionChanges, new_snapshot: bool) -> ExtensionUpdate {
        let snapshot =
            if new_snapshot || changes.catalog_changed || !changes.invalidated_views.is_empty() {
                Some(self.build_snapshot(changes.catalog_changed))
            } else {
                None
            };
        ExtensionUpdate {
            generation: self.generation,
            snapshot,
            effects: changes.effects,
            invalidated_views: changes.invalidated_views,
        }
    }

    pub fn run_worker_loop(
        mut self,
        executor: gpui::BackgroundExecutor,
        command_rx: mpsc::Receiver<ExtensionCommand>,
        update_tx: mpsc::SyncSender<ExtensionUpdate>,
        snapshot_lock: Arc<RwLock<ExtensionSnapshot>>,
    ) -> gpui::Task<()> {
        executor.clone().spawn(async move {
            let initial_snapshot = self.build_snapshot(true);
            *snapshot_lock.write().unwrap() = initial_snapshot.clone();
            let initial_update = ExtensionUpdate {
                generation: self.generation,
                snapshot: Some(initial_snapshot),
                effects: Vec::new(),
                invalidated_views: Vec::new(),
            };
            let mut pending_update = Some(initial_update);

            let mut pending_sources_changed = false;
            let mut last_sources_changed: Option<Instant> = None;

            loop {
                if let Some(update) = pending_update.take() {
                    match update_tx.try_send(update) {
                        Ok(()) => {}
                        Err(mpsc::TrySendError::Full(update)) => {
                            pending_update = Some(update);
                            executor.timer(Duration::from_millis(25)).await;
                            continue;
                        }
                        Err(mpsc::TrySendError::Disconnected(_)) => return,
                    }
                }

                while let Ok(cmd) = command_rx.try_recv() {
                    match cmd {
                        ExtensionCommand::SourcesChanged => {
                            pending_sources_changed = true;
                            last_sources_changed = Some(Instant::now());
                        }
                        ExtensionCommand::Shutdown { reply } => {
                            let _ = self.handle_command(ExtensionCommand::Shutdown { reply });
                            return;
                        }
                        other => {
                            if let Some(update) = self.handle_command(other) {
                                if let Some(ref snap) = update.snapshot {
                                    *snapshot_lock.write().unwrap() = snap.clone();
                                }
                                match update_tx.try_send(update) {
                                    Ok(()) => {}
                                    Err(mpsc::TrySendError::Full(update)) => {
                                        pending_update = Some(update);
                                        break;
                                    }
                                    Err(mpsc::TrySendError::Disconnected(_)) => return,
                                }
                            }
                        }
                    }
                }

                if pending_update.is_some() {
                    executor.timer(Duration::from_millis(25)).await;
                    continue;
                }

                if pending_sources_changed
                    && last_sources_changed.is_some_and(|t| t.elapsed() >= DEBOUNCE_DURATION)
                {
                    pending_sources_changed = false;
                    last_sources_changed = None;
                    if let Some(update) = self.handle_command(ExtensionCommand::SourcesChanged) {
                        if let Some(ref snap) = update.snapshot {
                            *snapshot_lock.write().unwrap() = snap.clone();
                        }
                        match update_tx.try_send(update) {
                            Ok(()) => {}
                            Err(mpsc::TrySendError::Full(update)) => {
                                pending_update = Some(update);
                            }
                            Err(mpsc::TrySendError::Disconnected(_)) => return,
                        }
                    }
                }

                executor.timer(Duration::from_millis(50)).await;
            }
        })
    }
}

fn fingerprint_path(path: &Path) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    fingerprint_path_recursive(path, &mut hasher);
    hasher.finish()
}

fn fingerprint_path_recursive<H: Hasher>(path: &Path, hasher: &mut H) {
    let Ok(metadata) = fs::metadata(path) else {
        return;
    };
    path.hash(hasher);
    metadata.len().hash(hasher);
    if let Ok(mtime) = metadata.modified()
        && let Ok(dur) = mtime.duration_since(UNIX_EPOCH)
    {
        dur.as_nanos().hash(hasher);
    }
    if metadata.is_dir()
        && let Ok(entries) = fs::read_dir(path)
    {
        let mut paths = Vec::new();
        for entry in entries.flatten() {
            paths.push(entry.path());
        }
        paths.sort();
        for child in paths {
            fingerprint_path_recursive(&child, hasher);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shilpo_ext::{GuestExtension, HostEffect, InMemoryRuntime};

    struct InvalidationGuest;

    impl GuestExtension for InvalidationGuest {
        fn on_event(&mut self, _event: &ExtensionEvent) -> Vec<HostEffect> {
            vec![HostEffect::InvalidateView {
                contribution_id: "bar".into(),
            }]
        }

        fn view(&self, _contribution_id: &str) -> Option<ViewTree> {
            None
        }
    }

    #[test]
    fn invalidate_view_effect_becomes_a_view_change() {
        let manifest = ExtensionManifest::from_toml(
            r#"
                id = "org.shilpo.test"
                name = "Test"
                version = "1.0.0"

                [[contributions.bar_widgets]]
                id = "bar"
                name = "Bar"
            "#,
        )
        .unwrap();
        let extension_id = manifest.id.clone();
        let mut session = ExtensionSession::new(InMemoryRuntime::default());
        session
            .register(manifest, Box::new(InvalidationGuest), Vec::new())
            .unwrap();

        let changes = session.dispatch(&extension_id, &ExtensionEvent::ShellStarted);

        assert_eq!(
            changes.invalidated_views,
            vec![CanonicalId {
                extension_id,
                contribution_id: ContributionId::new("bar").unwrap(),
            }]
        );
        assert!(changes.effects.is_empty());
    }
}
