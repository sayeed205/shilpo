//! Shell-owned extension lifecycle and contribution catalog.
//!
//! Surface code deliberately talks to this module instead of the WASM runtime.
//! This keeps loading, grants, state, hot reload, and last-valid-view policy in
//! one place while the bar, desktop, launcher, and settings remain thin views.

use shilpo_ext::{
    CanonicalId, Capability, CatalogPaths, DevelopmentRegistration, ExtensionCatalog,
    ExtensionEvent, ExtensionHost, ExtensionId, ExtensionManifest, ExtensionRuntime, HostEffect,
    InstalledExtension, ViewTree, WasmModule, WasmRuntime,
};
use std::{
    collections::{BTreeMap, HashMap},
    fs,
    hash::{Hash, Hasher},
    path::{Component, Path, PathBuf},
    time::{Duration, Instant, UNIX_EPOCH},
};

const RELOAD_SCAN_INTERVAL: Duration = Duration::from_millis(500);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ContributionSurface {
    Bar,
    Desktop,
    Settings,
    SidePanel,
    ControlCenter,
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

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ExtensionChanges {
    pub effects: Vec<(ExtensionId, HostEffect)>,
    pub invalidated_views: Vec<CanonicalId>,
    pub catalog_changed: bool,
}

impl ExtensionChanges {
    fn merge(&mut self, mut other: Self) {
        self.effects.append(&mut other.effects);
        self.invalidated_views.append(&mut other.invalidated_views);
        self.catalog_changed |= other.catalog_changed;
    }
}

struct ExtensionSession<R> {
    host: ExtensionHost<R>,
    manifests: BTreeMap<ExtensionId, ExtensionManifest>,
    runtime_ids: Vec<ExtensionId>,
    views: HashMap<CanonicalId, ViewTree>,
    instances: HashMap<String, ContributionInstance>,
    state: HashMap<(ExtensionId, String), serde_json::Value>,
}

impl<R: ExtensionRuntime> ExtensionSession<R> {
    fn new(runtime: R) -> Self {
        Self {
            host: ExtensionHost::new(runtime),
            manifests: BTreeMap::new(),
            runtime_ids: Vec::new(),
            views: HashMap::new(),
            instances: HashMap::new(),
            state: HashMap::new(),
        }
    }

    fn register(
        &mut self,
        manifest: ExtensionManifest,
        module: Option<R::Module>,
        grants: Vec<Capability>,
    ) -> Result<(), String> {
        let id = manifest.id.clone();
        if let Some(module) = module {
            self.host
                .register(manifest.clone(), module, grants)
                .map_err(|error| error.to_string())?;
            self.runtime_ids.push(id.clone());
        } else {
            manifest.validate().map_err(|error| error.to_string())?;
        }
        self.manifests.insert(id, manifest);
        Ok(())
    }

    fn descriptors(&self) -> Vec<ContributionDescriptor> {
        self.manifests
            .values()
            .flat_map(manifest_descriptors)
            .collect()
    }

    fn view(&mut self, id: &CanonicalId) -> Option<ViewTree> {
        if !self.runtime_ids.contains(&id.extension_id) {
            return self.views.get(id).cloned();
        }
        match self.host.render_view(id) {
            Ok(Some(tree)) => {
                self.views.insert(id.clone(), tree.clone());
                Some(tree)
            }
            Ok(None) | Err(_) => self.views.get(id).cloned(),
        }
    }

    fn dispatch(&mut self, id: &ExtensionId, event: &ExtensionEvent) -> ExtensionChanges {
        self.dispatch_inner(id, event, true)
    }

    fn dispatch_inner(
        &mut self,
        id: &ExtensionId,
        event: &ExtensionEvent,
        answer_state_reads: bool,
    ) -> ExtensionChanges {
        let mut changes = ExtensionChanges::default();
        let Ok(result) = self.host.dispatch_event(id, event) else {
            return changes;
        };
        for effect in result.accepted {
            match &effect {
                HostEffect::StateWrite { key, value } => {
                    self.state.insert((id.clone(), key.clone()), value.clone());
                }
                HostEffect::StateRead { key } if answer_state_reads => {
                    let value = self.state.get(&(id.clone(), key.clone())).cloned();
                    changes.merge(self.dispatch_inner(
                        id,
                        &ExtensionEvent::StateValue {
                            key: key.clone(),
                            value,
                        },
                        false,
                    ));
                }
                HostEffect::StateRead { .. } => {}
                HostEffect::InvalidateView { contribution_id } => {
                    if let Ok(contribution_id) =
                        shilpo_ext::ContributionId::new(contribution_id.clone())
                    {
                        let canonical = CanonicalId::new(id.clone(), contribution_id);
                        let _ = self.view(&canonical);
                        changes.invalidated_views.push(canonical);
                    }
                }
                _ => changes.effects.push((id.clone(), effect)),
            }
        }
        changes
    }

    fn dispatch_all(&mut self, event: &ExtensionEvent) -> ExtensionChanges {
        let ids = self.runtime_ids.clone();
        let mut changes = ExtensionChanges::default();
        for id in ids {
            changes.merge(self.dispatch(&id, event));
        }
        changes
    }

    fn input(
        &mut self,
        contribution: &CanonicalId,
        instance_id: Option<&str>,
        event_id: impl Into<String>,
        value: Option<serde_json::Value>,
    ) -> ExtensionChanges {
        self.dispatch(
            &contribution.extension_id,
            &ExtensionEvent::Input {
                contribution_id: contribution.contribution_id.to_string(),
                instance_id: instance_id.map(ToOwned::to_owned),
                event_id: event_id.into(),
                value,
            },
        )
    }

    fn reconcile_instances(
        &mut self,
        desired: impl IntoIterator<Item = ContributionInstance>,
    ) -> ExtensionChanges {
        let desired = desired
            .into_iter()
            .map(|instance| (instance.id.clone(), instance))
            .collect::<HashMap<_, _>>();
        let mut changes = ExtensionChanges::default();

        let removed = self
            .instances
            .keys()
            .filter(|id| !desired.contains_key(*id))
            .cloned()
            .collect::<Vec<_>>();
        for id in removed {
            if let Some(instance) = self.instances.remove(&id) {
                changes.merge(self.dispatch(
                    &instance.contribution.extension_id,
                    &ExtensionEvent::ContributionUnmounted {
                        contribution_id: instance.contribution.contribution_id.to_string(),
                        instance_id: Some(instance.id),
                    },
                ));
            }
        }

        for (id, next) in desired {
            match self.instances.get(&id) {
                None => {
                    changes.merge(self.dispatch(
                        &next.contribution.extension_id,
                        &ExtensionEvent::ContributionMounted {
                            contribution_id: next.contribution.contribution_id.to_string(),
                            instance_id: Some(next.id.clone()),
                            width: next.width,
                            height: next.height,
                        },
                    ));
                    changes.merge(self.dispatch(
                        &next.contribution.extension_id,
                        &ExtensionEvent::ContributionSettingsChanged {
                            contribution_id: next.contribution.contribution_id.to_string(),
                            instance_id: Some(next.id.clone()),
                            settings: next.settings.clone(),
                        },
                    ));
                }
                Some(previous)
                    if previous.width != next.width || previous.height != next.height =>
                {
                    changes.merge(self.dispatch(
                        &next.contribution.extension_id,
                        &ExtensionEvent::ContributionResized {
                            contribution_id: next.contribution.contribution_id.to_string(),
                            instance_id: Some(next.id.clone()),
                            width: next.width,
                            height: next.height,
                        },
                    ));
                }
                _ => {}
            }
            if self
                .instances
                .get(&id)
                .is_some_and(|previous| previous.settings != next.settings)
            {
                changes.merge(self.dispatch(
                    &next.contribution.extension_id,
                    &ExtensionEvent::ContributionSettingsChanged {
                        contribution_id: next.contribution.contribution_id.to_string(),
                        instance_id: Some(next.id.clone()),
                        settings: next.settings.clone(),
                    },
                ));
            }
            self.instances.insert(id, next);
        }
        changes
    }
}

#[derive(Clone)]
struct SourcePackage {
    root: PathBuf,
    manifest: ExtensionManifest,
    module: Option<Vec<u8>>,
    grants: Vec<Capability>,
    development: bool,
    fingerprint: u64,
}

/// The one extension owner held by [`crate::ShellRuntime`].
///
/// Development registrations are auto-granted only for capabilities declared
/// by their manifest. Installed packages use the host-owned grant store.
pub struct ShellExtensions {
    state_dir: PathBuf,
    catalog: ExtensionCatalog,
    session: ExtensionSession<WasmRuntime>,
    sources: BTreeMap<ExtensionId, SourcePackage>,
    diagnostics: Vec<String>,
    generation: u64,
    last_scan: Option<Instant>,
}

impl ShellExtensions {
    pub fn load_default() -> Result<Self, String> {
        Self::load_from_paths(
            shilpo_ext::default_extension_state_dir(),
            CatalogPaths::platform_default(),
        )
    }

    pub fn load_from(state_dir: PathBuf) -> Result<Self, String> {
        let paths = CatalogPaths::new(state_dir.join("data"), state_dir.join("config"));
        Self::load_from_paths(state_dir, paths)
    }

    pub fn load_from_paths(
        state_dir: PathBuf,
        catalog_paths: CatalogPaths,
    ) -> Result<Self, String> {
        let runtime = WasmRuntime::new().map_err(|error| error.to_string())?;
        let mut this = Self {
            state_dir,
            catalog: ExtensionCatalog::open(
                catalog_paths,
                semver::Version::parse(shilpo_ext::CURRENT_SHILPO_VERSION)
                    .expect("Shilpo version is valid semver"),
            ),
            session: ExtensionSession::new(runtime),
            sources: BTreeMap::new(),
            diagnostics: Vec::new(),
            generation: 0,
            last_scan: None,
        };
        let _ = this.refresh(true);
        Ok(this)
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn descriptors(&self) -> Vec<ContributionDescriptor> {
        self.session.descriptors()
    }

    pub fn descriptors_for(&self, surface: ContributionSurface) -> Vec<ContributionDescriptor> {
        self.descriptors()
            .into_iter()
            .filter(|descriptor| descriptor.surface == surface)
            .collect()
    }

    pub fn diagnostics(&self) -> &[String] {
        &self.diagnostics
    }

    pub fn view(&mut self, id: &CanonicalId) -> Option<ViewTree> {
        self.session.view(id)
    }

    pub fn input(
        &mut self,
        contribution: &CanonicalId,
        instance_id: Option<&str>,
        event_id: impl Into<String>,
        value: Option<serde_json::Value>,
    ) -> ExtensionChanges {
        self.session
            .input(contribution, instance_id, event_id, value)
    }

    pub fn dispatch_all(&mut self, event: &ExtensionEvent) -> ExtensionChanges {
        self.session.dispatch_all(event)
    }

    pub fn dispatch_to(
        &mut self,
        extension: &ExtensionId,
        event: &ExtensionEvent,
    ) -> ExtensionChanges {
        self.session.dispatch(extension, event)
    }

    pub fn reconcile_instances(
        &mut self,
        desired: impl IntoIterator<Item = ContributionInstance>,
    ) -> ExtensionChanges {
        self.session.reconcile_instances(desired)
    }

    pub fn settings_schema(&self, id: &CanonicalId) -> Result<Option<serde_json::Value>, String> {
        let descriptor = self
            .descriptors()
            .into_iter()
            .find(|descriptor| descriptor.id == *id);
        let Some(path) = descriptor.and_then(|descriptor| descriptor.settings_schema) else {
            return Ok(None);
        };
        let source = self
            .sources
            .get(&id.extension_id)
            .ok_or_else(|| format!("extension '{}' has no active source", id.extension_id))?;
        let path = safe_child(&source.root, &path)?;
        let content = fs::read_to_string(&path)
            .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
        serde_json::from_str(&content)
            .map(Some)
            .map_err(|error| format!("invalid settings schema {}: {error}", path.display()))
    }

    pub fn asset_path(&self, id: &CanonicalId, relative: &str) -> Result<PathBuf, String> {
        let source = self
            .sources
            .get(&id.extension_id)
            .ok_or_else(|| format!("extension '{}' has no active source", id.extension_id))?;
        let path = safe_child(&source.root.join("assets"), relative)?;
        path.is_file()
            .then_some(path.clone())
            .ok_or_else(|| format!("extension asset {} is unavailable", path.display()))
    }

    pub fn poll_hot_reload(&mut self) -> ExtensionChanges {
        if self
            .last_scan
            .is_some_and(|last| last.elapsed() < RELOAD_SCAN_INTERVAL)
        {
            return ExtensionChanges::default();
        }
        self.refresh(false)
    }

    fn refresh(&mut self, force: bool) -> ExtensionChanges {
        self.last_scan = Some(Instant::now());
        let (registrations, mut diagnostics) =
            shilpo_ext::development_registrations(&self.state_dir);
        let mut next_sources = BTreeMap::new();
        match self.catalog.active_packages() {
            Ok(packages) => {
                for package in packages {
                    match read_installed_source(package) {
                        Ok(source) => {
                            next_sources.insert(source.manifest.id.clone(), source);
                        }
                        Err(error) => diagnostics.push(error),
                    }
                }
            }
            Err(error) => diagnostics.push(format!("installed extension catalog: {error}")),
        }

        for registration in registrations {
            match read_source(&registration) {
                Ok(source)
                    if force
                        || self
                            .sources
                            .get(&registration.id)
                            .is_none_or(|old| old.fingerprint != source.fingerprint) =>
                {
                    next_sources.insert(registration.id, source);
                }
                Ok(source) => {
                    next_sources.insert(registration.id, source);
                }
                Err(error) => {
                    diagnostics.push(error);
                    if let Some(previous) = self
                        .sources
                        .get(&registration.id)
                        .filter(|source| source.development)
                        .cloned()
                    {
                        // Keep the last known-good development generation active.
                        next_sources.insert(registration.id, previous);
                    }
                }
            }
        }

        let changed = force || !same_sources(&self.sources, &next_sources);
        self.diagnostics = diagnostics;
        if !changed {
            return ExtensionChanges::default();
        }

        match build_session(&next_sources) {
            Ok(mut next_session) => {
                next_session.state = std::mem::take(&mut self.session.state);
                let instances = self.session.instances.values().cloned().collect::<Vec<_>>();
                let old_views = std::mem::take(&mut self.session.views);
                next_session.views = old_views;
                next_session.views.retain(|id, _| {
                    next_session
                        .manifests
                        .get(&id.extension_id)
                        .is_some_and(|manifest| {
                            manifest.contributions.contains(&id.contribution_id)
                        })
                });
                let mut changes = next_session.dispatch_all(&ExtensionEvent::ShellStarted);
                changes.merge(next_session.reconcile_instances(instances));
                changes.catalog_changed = true;
                self.session = next_session;
                self.sources = next_sources;
                self.generation = self.generation.saturating_add(1);
                changes
            }
            Err(error) => {
                self.diagnostics.push(format!(
                    "hot reload kept the last valid generation: {error}"
                ));
                ExtensionChanges::default()
            }
        }
    }
}

fn build_session(
    sources: &BTreeMap<ExtensionId, SourcePackage>,
) -> Result<ExtensionSession<WasmRuntime>, String> {
    let runtime = WasmRuntime::new().map_err(|error| error.to_string())?;
    let mut session = ExtensionSession::new(runtime);
    for source in sources.values() {
        let module = source.module.clone().map(WasmModule::from_bytes);
        session.register(source.manifest.clone(), module, source.grants.clone())?;
    }
    Ok(session)
}

fn read_source(registration: &DevelopmentRegistration) -> Result<SourcePackage, String> {
    let root = registration.path.clone();
    let manifest_path = root.join("extension.toml");
    let manifest_source = fs::read_to_string(&manifest_path)
        .map_err(|error| format!("failed to read {}: {error}", manifest_path.display()))?;
    let manifest = ExtensionManifest::from_toml(&manifest_source)
        .map_err(|error| format!("invalid {}: {error}", manifest_path.display()))?;
    if manifest.id != registration.id {
        return Err(format!(
            "development registration '{}' points to manifest '{}'",
            registration.id, manifest.id
        ));
    }
    let module = manifest
        .library
        .as_ref()
        .map(|library| {
            let path = safe_child(&root, &library.path)?;
            fs::read(&path).map_err(|error| format!("failed to read {}: {error}", path.display()))
        })
        .transpose()?;

    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    manifest_source.hash(&mut hasher);
    module.hash(&mut hasher);
    registration.generation.hash(&mut hasher);
    registration.updated_at_unix_seconds.hash(&mut hasher);
    hash_tree(&root.join("assets"), &mut hasher);
    for page in &manifest.contributions.settings_pages {
        if let Ok(path) = safe_child(&root, &page.schema) {
            hash_metadata(&path, &mut hasher);
        }
    }

    Ok(SourcePackage {
        root,
        grants: manifest.capabilities.clone(),
        manifest,
        module,
        development: true,
        fingerprint: hasher.finish(),
    })
}

fn read_installed_source(extension: InstalledExtension) -> Result<SourcePackage, String> {
    let root = extension.package_dir;
    let module = extension
        .manifest
        .library
        .as_ref()
        .map(|library| {
            let path = safe_child(&root, &library.path)?;
            fs::read(&path).map_err(|error| format!("failed to read {}: {error}", path.display()))
        })
        .transpose()?;
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    extension.receipt.active.version.hash(&mut hasher);
    extension.receipt.active.package_hash.hash(&mut hasher);
    serde_json::to_vec(&extension.grants)
        .unwrap_or_default()
        .hash(&mut hasher);
    Ok(SourcePackage {
        root,
        manifest: extension.manifest,
        module,
        grants: extension.grants.granted_capabilities,
        development: false,
        fingerprint: hasher.finish(),
    })
}

fn hash_tree(path: &Path, hasher: &mut impl Hasher) {
    let Ok(entries) = fs::read_dir(path) else {
        return;
    };
    let mut paths = entries
        .flatten()
        .map(|entry| entry.path())
        .collect::<Vec<_>>();
    paths.sort();
    for path in paths {
        path.hash(hasher);
        if path.is_dir() {
            hash_tree(&path, hasher);
        } else {
            hash_metadata(&path, hasher);
        }
    }
}

fn hash_metadata(path: &Path, hasher: &mut impl Hasher) {
    if let Ok(metadata) = fs::metadata(path) {
        metadata.len().hash(hasher);
        metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map(|duration| duration.as_nanos())
            .hash(hasher);
    }
}

fn same_sources(
    left: &BTreeMap<ExtensionId, SourcePackage>,
    right: &BTreeMap<ExtensionId, SourcePackage>,
) -> bool {
    left.len() == right.len()
        && left.iter().all(|(id, source)| {
            right
                .get(id)
                .is_some_and(|other| other.fingerprint == source.fingerprint)
        })
}

fn safe_child(root: &Path, relative: &str) -> Result<PathBuf, String> {
    let relative = Path::new(relative);
    if relative.is_absolute()
        || relative
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(format!(
            "path '{relative:?}' must stay inside the extension"
        ));
    }
    Ok(root.join(relative))
}

fn manifest_descriptors(
    manifest: &ExtensionManifest,
) -> impl Iterator<Item = ContributionDescriptor> + '_ {
    let base = |id: &shilpo_ext::ContributionId,
                name: &str,
                surface: ContributionSurface,
                settings_schema: Option<String>,
                default_size: Option<(u32, u32)>,
                minimum_size: Option<(u32, u32)>| ContributionDescriptor {
        id: CanonicalId::new(manifest.id.clone(), id.clone()),
        extension_name: manifest.name.clone(),
        name: name.to_owned(),
        surface,
        settings_schema,
        default_size,
        minimum_size,
    };
    let mut entries = Vec::new();
    entries.extend(manifest.contributions.bar_widgets.iter().map(|item| {
        base(
            &item.id,
            &item.name,
            ContributionSurface::Bar,
            None,
            None,
            None,
        )
    }));
    entries.extend(manifest.contributions.desktop_widgets.iter().map(|item| {
        base(
            &item.id,
            &item.name,
            ContributionSurface::Desktop,
            None,
            item.default_width.zip(item.default_height),
            item.min_width.zip(item.min_height),
        )
    }));
    entries.extend(manifest.contributions.settings_pages.iter().map(|item| {
        base(
            &item.id,
            &item.name,
            ContributionSurface::Settings,
            Some(item.schema.clone()),
            None,
            None,
        )
    }));
    entries.extend(manifest.contributions.side_panels.iter().map(|item| {
        base(
            &item.id,
            &item.name,
            ContributionSurface::SidePanel,
            None,
            None,
            None,
        )
    }));
    entries.extend(manifest.contributions.control_center.iter().map(|item| {
        base(
            &item.id,
            &item.name,
            ContributionSurface::ControlCenter,
            None,
            None,
            None,
        )
    }));
    entries.extend(
        manifest
            .contributions
            .launcher_providers
            .iter()
            .map(|item| {
                base(
                    &item.id,
                    &item.name,
                    ContributionSurface::Launcher,
                    None,
                    None,
                    None,
                )
            }),
    );
    entries.extend(manifest.contributions.actions.iter().map(|item| {
        base(
            &item.id,
            &item.name,
            ContributionSurface::Action,
            None,
            None,
            None,
        )
    }));
    entries.extend(manifest.contributions.background_tasks.iter().map(|item| {
        base(
            &item.id,
            &item.name,
            ContributionSurface::Background,
            None,
            None,
            None,
        )
    }));
    entries.into_iter()
}

#[cfg(test)]
mod tests {
    use super::*;
    use shilpo_ext::{ExtensionCli, GuestExtension, InMemoryRuntime, TextNode, ViewNode};
    use std::collections::HashSet;
    use std::sync::{Arc, Mutex};
    use std::time::{SystemTime, UNIX_EPOCH};

    struct Guest {
        events: Arc<Mutex<Vec<ExtensionEvent>>>,
    }

    impl GuestExtension for Guest {
        fn on_event(&mut self, event: &ExtensionEvent) -> Vec<HostEffect> {
            self.events.lock().unwrap().push(event.clone());
            match event {
                ExtensionEvent::Input {
                    event_id, value, ..
                } if event_id == "save" => vec![HostEffect::StateWrite {
                    key: "timezone".into(),
                    value: value.clone().unwrap_or(serde_json::Value::Null),
                }],
                ExtensionEvent::Input { event_id, .. } if event_id == "read" => {
                    vec![HostEffect::StateRead {
                        key: "timezone".into(),
                    }]
                }
                _ => vec![HostEffect::InvalidateView {
                    contribution_id: "bar".into(),
                }],
            }
        }

        fn view(&self, contribution_id: &str) -> Option<ViewTree> {
            (contribution_id == "bar").then(|| {
                ViewTree::new(ViewNode::Text(TextNode {
                    content: "extension".into(),
                    font_size: None,
                    bold: None,
                    style: None,
                }))
            })
        }
    }

    fn manifest() -> ExtensionManifest {
        ExtensionManifest::from_toml(
            r#"
id = "io.github.shilpo.test"
name = "Test"
version = "1.0.0"

[[contributions.bar_widgets]]
id = "bar"
name = "Bar"

[[contributions.desktop_widgets]]
id = "desktop"
name = "Desktop"
default_width = 320
default_height = 180

[[contributions.settings_pages]]
id = "settings"
name = "Settings"
schema = "settings.schema.json"

[[contributions.side_panels]]
id = "panel"
name = "Panel"

[[contributions.control_center]]
id = "controls"
name = "Controls"

[[contributions.launcher_providers]]
id = "search"
name = "Search"

[[contributions.actions]]
id = "refresh"
name = "Refresh"

[[contributions.background_tasks]]
id = "worker"
name = "Worker"
"#,
        )
        .unwrap()
    }

    #[test]
    fn catalog_and_instance_lifecycle_cross_the_host_interface() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let mut session = ExtensionSession::new(InMemoryRuntime::default());
        session
            .register(
                manifest(),
                Some(Box::new(Guest {
                    events: events.clone(),
                })),
                Vec::new(),
            )
            .unwrap();
        let surfaces = session
            .descriptors()
            .into_iter()
            .map(|descriptor| descriptor.surface)
            .collect::<HashSet<_>>();
        assert_eq!(
            surfaces,
            HashSet::from([
                ContributionSurface::Bar,
                ContributionSurface::Desktop,
                ContributionSurface::Settings,
                ContributionSurface::SidePanel,
                ContributionSurface::ControlCenter,
                ContributionSurface::Launcher,
                ContributionSurface::Action,
                ContributionSurface::Background,
            ])
        );

        let bar: CanonicalId = "io.github.shilpo.test/bar".parse().unwrap();
        assert!(session.view(&bar).is_some());
        session.reconcile_instances([
            ContributionInstance {
                id: "clock-1".into(),
                contribution: bar.clone(),
                output: Some("primary".into()),
                width: 120.0,
                height: 32.0,
                settings: serde_json::json!({}),
            },
            ContributionInstance {
                id: "clock-2".into(),
                contribution: bar.clone(),
                output: Some("secondary".into()),
                width: 180.0,
                height: 32.0,
                settings: serde_json::json!({}),
            },
        ]);
        session.input(&bar, Some("clock-1"), "toggle", Some(true.into()));
        session.input(&bar, Some("clock-1"), "save", Some("UTC".into()));
        session.input(&bar, Some("clock-1"), "read", None);
        session.reconcile_instances([ContributionInstance {
            id: "clock-2".into(),
            contribution: bar.clone(),
            output: Some("secondary".into()),
            width: 220.0,
            height: 40.0,
            settings: serde_json::json!({"timezone": "Asia/Kolkata"}),
        }]);

        let events = events.lock().unwrap();
        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(event, ExtensionEvent::ContributionMounted { .. }))
                .count(),
            2
        );
        assert!(events.iter().any(|event| matches!(
            event,
            ExtensionEvent::Input {
                instance_id: Some(id),
                event_id,
                ..
            } if id == "clock-1" && event_id == "toggle"
        )));
        assert!(events.iter().any(|event| matches!(
            event,
            ExtensionEvent::ContributionUnmounted {
                instance_id: Some(id),
                ..
            } if id == "clock-1"
        )));
        assert!(events.iter().any(|event| matches!(
            event,
            ExtensionEvent::ContributionResized {
                instance_id: Some(id),
                width: 220.0,
                height: 40.0,
                ..
            } if id == "clock-2"
        )));
        assert!(events.iter().any(|event| matches!(
            event,
            ExtensionEvent::ContributionSettingsChanged {
                instance_id: Some(id),
                settings,
                ..
            } if id == "clock-2" && settings["timezone"] == "Asia/Kolkata"
        )));
        assert!(events.iter().any(|event| matches!(
            event,
            ExtensionEvent::StateValue {
                key,
                value: Some(serde_json::Value::String(value)),
            } if key == "timezone" && value == "UTC"
        )));
    }

    #[test]
    fn installed_enablement_is_reconciled_through_the_shell_owner() {
        let root = std::env::temp_dir().join(format!(
            "shilpo-shell-installed-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let source = root.join("source");
        let output = root.join("dist");
        let state = root.join("state");
        let paths = CatalogPaths::new(root.join("data"), root.join("config"));
        fs::create_dir_all(&source).unwrap();
        fs::write(
            source.join("extension.toml"),
            r#"
id = "io.github.shilpo.installed-test"
name = "Installed Test"
version = "1.0.0"

[[contributions.bar_widgets]]
id = "bar"
name = "Installed Bar"
"#,
        )
        .unwrap();
        let package = ExtensionCli::pack(&source, &output).artifact.unwrap();
        let catalog = ExtensionCatalog::open(
            paths.clone(),
            semver::Version::parse(shilpo_ext::CURRENT_SHILPO_VERSION).unwrap(),
        );
        let receipt = catalog.install_local(&package).unwrap();

        let disabled = ShellExtensions::load_from_paths(state.clone(), paths.clone()).unwrap();
        assert!(disabled.descriptors().is_empty());

        catalog.set_enabled(&receipt.id, true).unwrap();
        let enabled = ShellExtensions::load_from_paths(state.clone(), paths.clone()).unwrap();
        assert_eq!(enabled.descriptors_for(ContributionSurface::Bar).len(), 1);

        catalog.set_enabled(&receipt.id, false).unwrap();
        let disabled_again = ShellExtensions::load_from_paths(state, paths).unwrap();
        assert!(disabled_again.descriptors().is_empty());
        fs::remove_dir_all(root).unwrap();
    }
}
