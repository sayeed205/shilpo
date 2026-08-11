use super::process::HostGeneration;
use super::protocol::{
    ContributionDescriptor, ContributionInstance, ContributionSurface, ExtensionChanges,
    ExtensionCommand, ExtensionGeneration, ExtensionSnapshot, ExtensionUpdate, ReplaceableEvent,
};
use crate::{
    AuthorizedHostEffectKind, CURRENT_SHILPO_VERSION, CatalogPaths, ExtensionCatalog,
    ExtensionHost, ExtensionRuntime,
};
use semver::Version;
use shilpo_ext_api::{
    CanonicalId, Capability, ExtensionEvent, ExtensionId, ExtensionManifest, HostEffect, ViewTree,
};
use std::{
    collections::{BTreeMap, HashMap},
    fs,
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
    sync::Arc,
    time::UNIX_EPOCH,
};

#[derive(Clone, Debug, PartialEq)]
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

    pub fn clear(&mut self) {
        self.host.clear();
        self.manifests.clear();
        self.runtime_ids.clear();
        self.views.clear();
        self.instances.clear();
        self.state.clear();
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
            .map_err(|error| error.to_string())?;

        for contribution in &manifest.contributions.bar_widgets {
            let canonical = CanonicalId::new(id.clone(), contribution.id.clone());
            if let Ok(Some(view)) = self.host.render_view(&canonical) {
                self.views.insert(canonical, view);
            }
        }
        for contribution in &manifest.contributions.desktop_widgets {
            let canonical = CanonicalId::new(id.clone(), contribution.id.clone());
            if let Ok(Some(view)) = self.host.render_view(&canonical) {
                self.views.insert(canonical, view);
            }
        }

        self.manifests.insert(id.clone(), manifest);
        self.runtime_ids.push(id);
        Ok(())
    }

    pub fn dispatch(&mut self, event: &ExtensionEvent) -> ExtensionChanges {
        let mut changes = ExtensionChanges::default();
        let target_ids = match event {
            ExtensionEvent::ContributionMounted {
                contribution_id,
                instance_id,
                width,
                height,
            } => {
                if let Ok(canonical) = contribution_id.parse::<CanonicalId>() {
                    let instance_key = instance_id.clone().unwrap_or_else(|| canonical.to_string());
                    self.instances.insert(
                        instance_key,
                        ContributionInstance {
                            id: instance_id.clone().unwrap_or_else(|| canonical.to_string()),
                            contribution: canonical.clone(),
                            output: None,
                            width: *width,
                            height: *height,
                            settings: serde_json::Value::Object(serde_json::Map::new()),
                        },
                    );
                    vec![canonical.extension_id]
                } else {
                    Vec::new()
                }
            }
            ExtensionEvent::ContributionUnmounted {
                contribution_id,
                instance_id,
            } => {
                if let Ok(canonical) = contribution_id.parse::<CanonicalId>() {
                    let instance_key = instance_id.clone().unwrap_or_else(|| canonical.to_string());
                    self.instances.remove(&instance_key);
                    vec![canonical.extension_id]
                } else {
                    Vec::new()
                }
            }
            ExtensionEvent::ContributionResized {
                contribution_id,
                instance_id,
                width,
                height,
            } => {
                if let Ok(canonical) = contribution_id.parse::<CanonicalId>() {
                    let instance_key = instance_id.clone().unwrap_or_else(|| canonical.to_string());
                    if let Some(instance) = self.instances.get_mut(&instance_key) {
                        instance.width = *width;
                        instance.height = *height;
                    }
                    vec![canonical.extension_id]
                } else {
                    Vec::new()
                }
            }
            ExtensionEvent::ContributionSettingsChanged {
                contribution_id,
                instance_id,
                settings,
            } => {
                if let Ok(canonical) = contribution_id.parse::<CanonicalId>() {
                    let instance_key = instance_id.clone().unwrap_or_else(|| canonical.to_string());
                    if let Some(instance) = self.instances.get_mut(&instance_key) {
                        instance.settings = settings.clone();
                    }
                    vec![canonical.extension_id]
                } else {
                    Vec::new()
                }
            }
            ExtensionEvent::Input {
                contribution_id, ..
            } => {
                if let Ok(canonical) = contribution_id.parse::<CanonicalId>() {
                    vec![canonical.extension_id]
                } else {
                    Vec::new()
                }
            }
            _ => self.runtime_ids.clone(),
        };

        for id in target_ids {
            if let Ok(result) = self.host.dispatch_event(&id, event) {
                for authorized in result.accepted {
                    self.apply_effect(&id, authorized.kind(), &mut changes);
                }
            }
        }
        changes
    }

    pub fn dispatch_to_extension(
        &mut self,
        extension_id: &ExtensionId,
        event: &ExtensionEvent,
    ) -> ExtensionChanges {
        let mut changes = ExtensionChanges::default();
        if let Ok(result) = self.host.dispatch_event(extension_id, event) {
            for authorized in result.accepted {
                self.apply_effect(extension_id, authorized.kind(), &mut changes);
            }
        }
        changes
    }

    fn apply_effect(
        &mut self,
        extension_id: &ExtensionId,
        effect_kind: &AuthorizedHostEffectKind,
        changes: &mut ExtensionChanges,
    ) {
        match effect_kind {
            AuthorizedHostEffectKind::NonHttp(effect) => match effect {
                HostEffect::InvalidateView { contribution_id } => {
                    if let Ok(canonical) = contribution_id.parse::<CanonicalId>()
                        && canonical.extension_id == *extension_id
                        && let Ok(Some(view)) = self.host.render_view(&canonical)
                    {
                        self.views.insert(canonical.clone(), view);
                        changes.invalidated_views.push(canonical);
                    }
                }
                HostEffect::StateRead { key } => {
                    let value = self
                        .state
                        .get(&(extension_id.clone(), key.clone()))
                        .cloned();
                    let reply_event = ExtensionEvent::StateValue {
                        key: key.clone(),
                        value,
                    };
                    changes.merge(self.dispatch_to_extension(extension_id, &reply_event));
                }
                HostEffect::StateWrite { key, value } => {
                    self.state
                        .insert((extension_id.clone(), key.clone()), value.clone());
                }
                HostEffect::ShowNotification { .. }
                | HostEffect::InvokeAction { .. }
                | HostEffect::SetWallpaper { .. }
                | HostEffect::WallpaperMetadataRead
                | HostEffect::ThemeRead
                | HostEffect::SetThemeSource { .. }
                | HostEffect::ClipboardRead
                | HostEffect::ClipboardWrite { .. }
                | HostEffect::ExecProcess { .. }
                | HostEffect::ReadFile { .. }
                | HostEffect::WriteFile { .. }
                | HostEffect::LocationRead => {
                    if let Ok(authorized) = crate::AuthorizedHostEffect::non_http(effect.clone()) {
                        changes.effects.push((extension_id.clone(), authorized));
                    }
                }
                HostEffect::HttpRequest { .. } => {}
            },
            AuthorizedHostEffectKind::HttpRequest(request) => {
                changes.effects.push((
                    extension_id.clone(),
                    crate::AuthorizedHostEffect::http_request(
                        request.request_id().to_owned(),
                        crate::CanonicalHttpTarget::parse(request.url().as_str(), "GET")
                            .expect("AuthorizedHttpRequest contains a valid target"),
                    ),
                ));
            }
        }
    }
}

pub struct ExtensionEngine<R = crate::WasmRuntime> {
    paths: CatalogPaths,
    catalog: ExtensionCatalog,
    catalog_mtime: Option<std::time::SystemTime>,
    active_sources: BTreeMap<ExtensionId, ActiveSource>,
    session: ExtensionSession<R>,
    generation: ExtensionGeneration,
    diagnostics: Vec<String>,
    replaceable_events: HashMap<std::mem::Discriminant<ReplaceableEvent>, ReplaceableEvent>,
    pending_reconcile: Option<Vec<ContributionInstance>>,
}

impl<R: ExtensionRuntime> ExtensionEngine<R> {
    pub fn new(runtime: R, paths: CatalogPaths) -> Result<Self, String> {
        let shilpo_version =
            Version::parse(CURRENT_SHILPO_VERSION).expect("Shilpo version is valid semver");
        let catalog = ExtensionCatalog::open(paths.clone(), shilpo_version);
        let catalog_mtime = catalog_mtime(&paths.data_dir);
        let session = ExtensionSession::new(runtime);

        let mut engine = Self {
            paths,
            catalog,
            catalog_mtime,
            active_sources: BTreeMap::new(),
            session,
            generation: ExtensionGeneration(0),
            diagnostics: Vec::new(),
            replaceable_events: HashMap::new(),
            pending_reconcile: None,
        };

        engine.reload_all()?;
        Ok(engine)
    }

    pub fn generation(&self) -> ExtensionGeneration {
        self.generation
    }

    pub fn diagnostics(&self) -> &[String] {
        &self.diagnostics
    }

    pub fn handle_command(&mut self, command: ExtensionCommand) -> Option<ExtensionUpdate> {
        match command {
            ExtensionCommand::Lifecycle { expected, event } => {
                if expected != self.generation {
                    return None;
                }
                let changes = self.session.dispatch(&event);
                Some(ExtensionUpdate {
                    host_generation: HostGeneration(0),
                    generation: self.generation,
                    snapshot: None,
                    effects: changes.effects,
                    invalidated_views: changes.invalidated_views,
                })
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
                let event = ExtensionEvent::Input {
                    contribution_id: contribution.to_string(),
                    instance_id,
                    event_id,
                    value,
                };
                let changes = self
                    .session
                    .dispatch_to_extension(&contribution.extension_id, &event);
                Some(ExtensionUpdate {
                    host_generation: HostGeneration(0),
                    generation: self.generation,
                    snapshot: None,
                    effects: changes.effects,
                    invalidated_views: changes.invalidated_views,
                })
            }
            ExtensionCommand::Response {
                expected,
                extension_id,
                event,
            } => {
                if expected != self.generation {
                    return None;
                }
                let changes = self.session.dispatch_to_extension(&extension_id, &event);
                Some(ExtensionUpdate {
                    host_generation: HostGeneration(0),
                    generation: self.generation,
                    snapshot: None,
                    effects: changes.effects,
                    invalidated_views: changes.invalidated_views,
                })
            }
            ExtensionCommand::Replaceable(event) => {
                self.replaceable_events
                    .insert(std::mem::discriminant(&event), event.clone());
                let ext_event = match event {
                    ReplaceableEvent::Power {
                        percentage,
                        charging,
                    } => ExtensionEvent::PowerChanged {
                        percentage,
                        charging,
                    },
                    ReplaceableEvent::Network { connected } => {
                        ExtensionEvent::NetworkChanged { connected }
                    }
                    ReplaceableEvent::Media {
                        title,
                        artist,
                        playing,
                    } => ExtensionEvent::MediaChanged {
                        title,
                        artist,
                        playing,
                    },
                    ReplaceableEvent::TimerFired(name) => ExtensionEvent::TimerFired { name },
                };
                let changes = self.session.dispatch(&ext_event);
                Some(ExtensionUpdate {
                    host_generation: HostGeneration(0),
                    generation: self.generation,
                    snapshot: None,
                    effects: changes.effects,
                    invalidated_views: changes.invalidated_views,
                })
            }
            ExtensionCommand::ReconcileInstances { expected, desired } => {
                if expected != self.generation {
                    return None;
                }
                let changes = self.reconcile_instances(desired);
                Some(ExtensionUpdate {
                    host_generation: HostGeneration(0),
                    generation: self.generation,
                    snapshot: None,
                    effects: changes.effects,
                    invalidated_views: changes.invalidated_views,
                })
            }
            ExtensionCommand::SourcesChanged => {
                if let Err(error) = self.reload_all() {
                    self.diagnostics.push(format!("reload failed: {error}"));
                }
                let snapshot = self.build_snapshot(true);
                Some(ExtensionUpdate {
                    host_generation: HostGeneration(0),
                    generation: self.generation,
                    snapshot: Some(snapshot),
                    effects: Vec::new(),
                    invalidated_views: Vec::new(),
                })
            }
            ExtensionCommand::Shutdown => None,
        }
    }

    fn reconcile_instances(&mut self, desired: Vec<ContributionInstance>) -> ExtensionChanges {
        let desired_keys: HashMap<String, ContributionInstance> = desired
            .into_iter()
            .map(|inst| (inst.id.clone(), inst))
            .collect();

        let mut changes = ExtensionChanges::default();
        let current_keys: Vec<String> = self.session.instances.keys().cloned().collect();

        for key in current_keys {
            if !desired_keys.contains_key(&key)
                && let Some(instance) = self.session.instances.get(&key)
            {
                let event = ExtensionEvent::ContributionUnmounted {
                    contribution_id: instance.contribution.to_string(),
                    instance_id: Some(instance.id.clone()),
                };
                changes.merge(self.session.dispatch(&event));
            }
        }

        for (key, instance) in desired_keys {
            if let Some(existing) = self.session.instances.get_mut(&key) {
                let width_changed = (existing.width - instance.width).abs() > f32::EPSILON
                    || (existing.height - instance.height).abs() > f32::EPSILON;
                let settings_changed = existing.settings != instance.settings;
                if width_changed {
                    existing.width = instance.width;
                    existing.height = instance.height;
                }
                if settings_changed {
                    existing.settings = instance.settings.clone();
                }
                let contribution_str = instance.contribution.to_string();
                let instance_id = instance.id.clone();
                let width = instance.width;
                let height = instance.height;
                let settings = instance.settings;
                if width_changed {
                    let event = ExtensionEvent::ContributionResized {
                        contribution_id: contribution_str.clone(),
                        instance_id: Some(instance_id.clone()),
                        width,
                        height,
                    };
                    changes.merge(self.session.dispatch(&event));
                }
                if settings_changed {
                    let event = ExtensionEvent::ContributionSettingsChanged {
                        contribution_id: contribution_str,
                        instance_id: Some(instance_id),
                        settings,
                    };
                    changes.merge(self.session.dispatch(&event));
                }
            } else {
                let event = ExtensionEvent::ContributionMounted {
                    contribution_id: instance.contribution.to_string(),
                    instance_id: Some(instance.id.clone()),
                    width: instance.width,
                    height: instance.height,
                };
                changes.merge(self.session.dispatch(&event));
            }
        }

        changes
    }

    pub fn reload_all(&mut self) -> Result<(), String> {
        self.diagnostics.clear();
        let new_sources = self.discover_sources()?;
        let catalog_changed = self.catalog_mtime != catalog_mtime(&self.paths.data_dir);

        if new_sources == self.active_sources && !catalog_changed {
            return Ok(());
        }

        let preserved_instances: Vec<ContributionInstance> =
            self.session.instances.values().cloned().collect();

        self.session.clear();
        self.active_sources = new_sources;
        self.catalog_mtime = catalog_mtime(&self.paths.data_dir);

        for (id, source) in &self.active_sources {
            let wasm_file = source.root.join("extension.wasm");
            let wasm_bytes = match fs::read(&wasm_file) {
                Ok(bytes) => bytes,
                Err(error) => {
                    self.diagnostics
                        .push(format!("failed to read extension WASM for '{id}': {error}"));
                    continue;
                }
            };
            let module = match self.session.host.runtime().compile_module(&wasm_bytes) {
                Ok(mod_obj) => mod_obj,
                Err(error) => {
                    self.diagnostics.push(format!(
                        "failed to compile extension module for '{id}': {error}"
                    ));
                    continue;
                }
            };

            if let Err(error) =
                self.session
                    .register(source.manifest.clone(), module, source.grants.clone())
            {
                self.diagnostics
                    .push(format!("failed to register extension '{id}': {error}"));
            }
        }

        self.generation = self.generation.next();
        self.session.dispatch(&ExtensionEvent::ShellStarted);

        for event in self.replaceable_events.values() {
            let ext_event = match event {
                ReplaceableEvent::Power {
                    percentage,
                    charging,
                } => ExtensionEvent::PowerChanged {
                    percentage: *percentage,
                    charging: *charging,
                },
                ReplaceableEvent::Network { connected } => ExtensionEvent::NetworkChanged {
                    connected: *connected,
                },
                ReplaceableEvent::Media {
                    title,
                    artist,
                    playing,
                } => ExtensionEvent::MediaChanged {
                    title: title.clone(),
                    artist: artist.clone(),
                    playing: *playing,
                },
                ReplaceableEvent::TimerFired(name) => {
                    ExtensionEvent::TimerFired { name: name.clone() }
                }
            };
            self.session.dispatch(&ext_event);
        }

        self.pending_reconcile = Some(preserved_instances);
        Ok(())
    }

    fn discover_sources(&mut self) -> Result<BTreeMap<ExtensionId, ActiveSource>, String> {
        let mut map = BTreeMap::new();

        let installed = self
            .catalog
            .installed()
            .map_err(|error| error.to_string())?;

        for cat_ext in installed {
            let root = cat_ext.package_dir.clone();
            let manifest = cat_ext.manifest;
            let grants = cat_ext.grants.granted_capabilities;

            let shilpo_version =
                Version::parse(CURRENT_SHILPO_VERSION).expect("Shilpo version is valid semver");
            if manifest.min_shilpo_version > shilpo_version {
                self.diagnostics.push(format!(
                    "extension '{}' requires Shilpo >= {}, but current is {}",
                    manifest.id, manifest.min_shilpo_version, shilpo_version
                ));
                continue;
            }

            let fingerprint = compute_fingerprint(&root, &manifest);
            map.insert(
                manifest.id.clone(),
                ActiveSource {
                    manifest,
                    root,
                    grants,
                    fingerprint,
                },
            );
        }

        let state_dir = crate::default_extension_state_dir();
        let (dev_registrations, diags) = crate::development_registrations(&state_dir);
        self.diagnostics.extend(diags);

        for reg in dev_registrations {
            let id = reg.id;
            let root = reg.path;
            let manifest_path = root.join("extension.toml");
            let toml_str = match fs::read_to_string(&manifest_path) {
                Ok(s) => s,
                Err(error) => {
                    self.diagnostics.push(format!(
                        "failed to read dev manifest at {}: {error}",
                        manifest_path.display()
                    ));
                    continue;
                }
            };
            let manifest = match ExtensionManifest::from_toml(&toml_str) {
                Ok(m) => m,
                Err(error) => {
                    self.diagnostics.push(format!(
                        "invalid dev manifest at {}: {error}",
                        manifest_path.display()
                    ));
                    continue;
                }
            };

            let grants = self
                .catalog
                .load_grants(&manifest.id)
                .map(|g| g.granted_capabilities)
                .unwrap_or_default();

            let fingerprint = compute_fingerprint(&root, &manifest);
            map.insert(
                id,
                ActiveSource {
                    manifest,
                    root,
                    grants,
                    fingerprint,
                },
            );
        }

        Ok(map)
    }

    pub fn build_snapshot(&mut self, catalog_changed: bool) -> ExtensionSnapshot {
        if let Some(desired) = self.pending_reconcile.take() {
            self.reconcile_instances(desired);
        }

        let mut descriptors = Vec::new();
        let mut settings_schemas = BTreeMap::new();
        let mut prevalidated_asset_roots = BTreeMap::new();

        for (ext_id, source) in &self.active_sources {
            prevalidated_asset_roots.insert(ext_id.clone(), source.root.clone());
            let m = &source.manifest;

            for contrib in &m.contributions.bar_widgets {
                descriptors.push(ContributionDescriptor {
                    id: CanonicalId::new(ext_id.clone(), contrib.id.clone()),
                    extension_name: m.name.clone(),
                    name: contrib.name.clone(),
                    surface: ContributionSurface::Bar,
                    settings_schema: None,
                    default_size: None,
                    minimum_size: None,
                });
            }
            for contrib in &m.contributions.desktop_widgets {
                let default_size = match (contrib.default_width, contrib.default_height) {
                    (Some(w), Some(h)) => Some((w, h)),
                    _ => None,
                };
                let minimum_size = match (contrib.min_width, contrib.min_height) {
                    (Some(w), Some(h)) => Some((w, h)),
                    _ => None,
                };
                descriptors.push(ContributionDescriptor {
                    id: CanonicalId::new(ext_id.clone(), contrib.id.clone()),
                    extension_name: m.name.clone(),
                    name: contrib.name.clone(),
                    surface: ContributionSurface::Desktop,
                    settings_schema: None,
                    default_size,
                    minimum_size,
                });
            }
            for contrib in &m.contributions.settings_pages {
                let canonical = CanonicalId::new(ext_id.clone(), contrib.id.clone());
                let schema_path = source.root.join(&contrib.schema);
                if let Ok(schema_str) = fs::read_to_string(&schema_path)
                    && let Ok(json_val) = serde_json::from_str(&schema_str)
                {
                    settings_schemas.insert(canonical.clone(), json_val);
                }
                descriptors.push(ContributionDescriptor {
                    id: canonical,
                    extension_name: m.name.clone(),
                    name: contrib.name.clone(),
                    surface: ContributionSurface::Settings,
                    settings_schema: Some(contrib.schema.clone()),
                    default_size: None,
                    minimum_size: None,
                });
            }
            for contrib in &m.contributions.side_panels {
                descriptors.push(ContributionDescriptor {
                    id: CanonicalId::new(ext_id.clone(), contrib.id.clone()),
                    extension_name: m.name.clone(),
                    name: contrib.name.clone(),
                    surface: ContributionSurface::SidePanel,
                    settings_schema: None,
                    default_size: None,
                    minimum_size: None,
                });
            }
            for contrib in &m.contributions.launcher_providers {
                descriptors.push(ContributionDescriptor {
                    id: CanonicalId::new(ext_id.clone(), contrib.id.clone()),
                    extension_name: m.name.clone(),
                    name: contrib.name.clone(),
                    surface: ContributionSurface::Launcher,
                    settings_schema: None,
                    default_size: None,
                    minimum_size: None,
                });
            }
            for contrib in &m.contributions.actions {
                descriptors.push(ContributionDescriptor {
                    id: CanonicalId::new(ext_id.clone(), contrib.id.clone()),
                    extension_name: m.name.clone(),
                    name: contrib.name.clone(),
                    surface: ContributionSurface::Action,
                    settings_schema: None,
                    default_size: None,
                    minimum_size: None,
                });
            }
            for contrib in &m.contributions.background_tasks {
                descriptors.push(ContributionDescriptor {
                    id: CanonicalId::new(ext_id.clone(), contrib.id.clone()),
                    extension_name: m.name.clone(),
                    name: contrib.name.clone(),
                    surface: ContributionSurface::Background,
                    settings_schema: None,
                    default_size: None,
                    minimum_size: None,
                });
            }
        }

        let views = self.session.views.clone().into_iter().collect();

        ExtensionSnapshot {
            generation: self.generation,
            descriptors: Arc::from(descriptors),
            views: Arc::new(views),
            diagnostics: Arc::from(self.diagnostics.clone()),
            catalog_changed_at: if catalog_changed {
                Some(self.generation)
            } else {
                None
            },
            settings_schemas: Arc::new(settings_schemas),
            prevalidated_asset_roots: Arc::new(prevalidated_asset_roots),
        }
    }
}

fn catalog_mtime(path: &Path) -> Option<std::time::SystemTime> {
    fs::metadata(path).and_then(|m| m.modified()).ok()
}

fn compute_fingerprint(root: &Path, manifest: &ExtensionManifest) -> u64 {
    let mut hasher = std::hash::DefaultHasher::new();
    manifest.id.hash(&mut hasher);
    manifest.version.hash(&mut hasher);

    let wasm_file = root.join("extension.wasm");
    if let Ok(meta) = fs::metadata(&wasm_file) {
        if let Ok(modified) = meta.modified()
            && let Ok(duration) = modified.duration_since(UNIX_EPOCH)
        {
            duration.as_nanos().hash(&mut hasher);
        }
        meta.len().hash(&mut hasher);
    }
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{GuestExtension, InMemoryRuntime};
    use shilpo_ext_api::HostEffect;

    struct TestGuest;
    impl GuestExtension for TestGuest {
        fn on_event(&mut self, event: &ExtensionEvent) -> Vec<HostEffect> {
            match event {
                ExtensionEvent::ShellStarted => vec![HostEffect::ShowNotification {
                    title: "Hello".into(),
                    body: "World".into(),
                    icon: None,
                }],
                _ => Vec::new(),
            }
        }

        fn view(&self, _contribution_id: &str) -> Option<ViewTree> {
            None
        }
    }

    #[test]
    fn test_extension_session_basic_lifecycle() {
        let runtime = InMemoryRuntime::new();
        let manifest = ExtensionManifest::from_toml(
            r#"
            id = "io.github.test.sample"
            name = "Sample"
            version = "1.0.0"
            schema_version = 1
            api_version = "0.2.0"
            min_shilpo_version = "0.2.0"

            [[capabilities]]
            kind = "notifications:show"
            "#,
        )
        .unwrap();

        let mut session = ExtensionSession::new(runtime);
        session
            .register(
                manifest.clone(),
                Box::new(TestGuest),
                manifest.capabilities.clone(),
            )
            .unwrap();

        let changes = session.dispatch(&ExtensionEvent::ShellStarted);
        assert_eq!(changes.effects.len(), 1);
        assert_eq!(changes.effects[0].0.as_str(), "io.github.test.sample");
    }
}
