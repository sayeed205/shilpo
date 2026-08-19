use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

use shilpo_ext_api::{
    CanonicalId, ExtensionEvent, ExtensionId, WallpaperMode, WallpaperRequest,
    WallpaperRequestReason, WallpaperSource, WallpaperTarget, WallpaperTargetKind, WorkspaceTarget,
};
use shilpo_ext_runtime::{ContributionDescriptor, ContributionSurface, ExtensionGeneration};

use crate::config::ExtensionsConfig;

#[derive(Clone, Debug, PartialEq)]
pub struct PendingWallpaperRequest {
    pub generation: ExtensionGeneration,
    pub extension_id: ExtensionId,
    pub target: WallpaperTarget,
    pub mode: WallpaperMode,
    pub reason: WallpaperRequestReason,
}

pub struct WallpaperCoordinator {
    active_provider: Option<CanonicalId>,
    active_modes: Vec<WallpaperMode>,
    active_targets: Vec<WallpaperTargetKind>,
    provider_generation: Option<ExtensionGeneration>,
    pending_requests: HashMap<String, PendingWallpaperRequest>,
    active_request_by_target: HashMap<WallpaperTarget, String>,
    last_valid_wallpapers: HashMap<WallpaperTarget, String>,
    last_active_workspace: Option<String>,
    next_request_seq: u64,
    slideshow_interval: Duration,
    last_slideshow_tick: Instant,
    settings_signature: Option<String>,
}

impl Default for WallpaperCoordinator {
    fn default() -> Self {
        Self::new()
    }
}

impl WallpaperCoordinator {
    pub fn new() -> Self {
        Self {
            active_provider: None,
            active_modes: Vec::new(),
            active_targets: Vec::new(),
            provider_generation: None,
            pending_requests: HashMap::new(),
            active_request_by_target: HashMap::new(),
            last_valid_wallpapers: HashMap::new(),
            last_active_workspace: None,
            next_request_seq: 0,
            slideshow_interval: Duration::from_secs(300),
            last_slideshow_tick: Instant::now(),
            settings_signature: None,
        }
    }

    pub fn active_provider(&self) -> Option<&CanonicalId> {
        self.active_provider.as_ref()
    }

    pub fn active_modes(&self) -> &[WallpaperMode] {
        &self.active_modes
    }

    pub fn active_targets(&self) -> &[WallpaperTargetKind] {
        &self.active_targets
    }

    pub fn provider_generation(&self) -> Option<ExtensionGeneration> {
        self.provider_generation
    }

    pub fn accepts_result(
        &self,
        extension_id: &ExtensionId,
        generation: ExtensionGeneration,
    ) -> bool {
        self.provider_generation == Some(generation)
            && self
                .active_provider
                .as_ref()
                .is_some_and(|provider| provider.extension_id == *extension_id)
    }

    pub fn last_valid_wallpaper(&self, target: &WallpaperTarget) -> Option<&str> {
        self.last_valid_wallpapers.get(target).map(String::as_str)
    }

    pub fn sync_active_provider(
        &mut self,
        config: &ExtensionsConfig,
        descriptors: &[ContributionDescriptor],
        generation: ExtensionGeneration,
    ) -> Option<(ExtensionId, ExtensionEvent)> {
        let wallpaper_descriptors: Vec<&ContributionDescriptor> = descriptors
            .iter()
            .filter(|desc| desc.surface == ContributionSurface::Wallpaper)
            .collect();

        let chosen = config.wallpaper_provider.as_ref().and_then(|configured| {
            wallpaper_descriptors
                .into_iter()
                .find(|desc| &desc.id == configured)
        });

        match chosen {
            Some(desc) => {
                let settings_signature = config
                    .settings
                    .get(desc.id.extension_id.as_str())
                    .map(ToString::to_string);
                let is_new = self.active_provider.as_ref() != Some(&desc.id)
                    || self.provider_generation != Some(generation);
                let settings_changed = self.settings_signature != settings_signature;

                if let Some(seconds) = config
                    .settings
                    .get(desc.id.extension_id.as_str())
                    .and_then(|settings| settings.get("slideshow_interval_seconds"))
                    .and_then(serde_json::Value::as_u64)
                {
                    self.slideshow_interval = Duration::from_secs(seconds.clamp(30, 86_400));
                }

                if is_new {
                    self.pending_requests.clear();
                    self.active_request_by_target.clear();
                    self.active_provider = Some(desc.id.clone());
                    self.active_modes = desc
                        .wallpaper_modes
                        .clone()
                        .unwrap_or_else(|| vec![WallpaperMode::Manual, WallpaperMode::Slideshow]);
                    self.active_targets = desc.wallpaper_targets.clone().unwrap_or_else(|| {
                        vec![WallpaperTargetKind::Global, WallpaperTargetKind::Workspace]
                    });
                    self.provider_generation = Some(generation);
                    self.last_slideshow_tick = Instant::now();
                    self.settings_signature = settings_signature;

                    // Initial activation request
                    let mode = if self.active_modes.contains(&WallpaperMode::Slideshow) {
                        WallpaperMode::Slideshow
                    } else {
                        WallpaperMode::Manual
                    };
                    self.dispatch_request(
                        WallpaperRequestReason::Activate,
                        mode,
                        WallpaperTarget::Global,
                    )
                } else if settings_changed {
                    self.settings_signature = settings_signature;
                    self.on_settings_changed()
                } else {
                    None
                }
            }
            None => {
                self.on_provider_disabled_or_unloaded();
                None
            }
        }
    }

    pub fn on_provider_disabled_or_unloaded(&mut self) {
        self.pending_requests.clear();
        self.active_request_by_target.clear();
        self.active_provider = None;
        self.active_modes.clear();
        self.active_targets.clear();
        self.provider_generation = None;
    }

    pub fn request_next_wallpaper(&mut self) -> Option<(ExtensionId, ExtensionEvent)> {
        let Some(_provider) = &self.active_provider else {
            return None;
        };

        let target = if self
            .active_targets
            .contains(&WallpaperTargetKind::Workspace)
            && let Some(ws) = &self.last_active_workspace
        {
            WallpaperTarget::Workspace(WorkspaceTarget {
                workspace_id: ws.clone(),
                output_name: None,
            })
        } else {
            WallpaperTarget::Global
        };

        let mode = if self.active_modes.contains(&WallpaperMode::Slideshow) {
            WallpaperMode::Slideshow
        } else {
            WallpaperMode::Manual
        };

        self.dispatch_request(WallpaperRequestReason::UserNext, mode, target)
    }

    pub fn on_workspace_changed(
        &mut self,
        workspace_id: &str,
        output_name: Option<&str>,
    ) -> Option<(ExtensionId, ExtensionEvent)> {
        self.last_active_workspace = Some(workspace_id.to_string());

        if !self
            .active_targets
            .contains(&WallpaperTargetKind::Workspace)
        {
            return None;
        }

        let target = WallpaperTarget::Workspace(WorkspaceTarget {
            workspace_id: workspace_id.to_string(),
            output_name: output_name.map(ToString::to_string),
        });

        if let Some(cached_path) = self.last_valid_wallpapers.get(&target) {
            let path = cached_path.clone();
            shilpo_theme_daemon::ThemeClient::spawn_task(async move {
                let client = shilpo_theme_daemon::ThemeClient::new().await;
                let _ = client.set_wallpaper(&path).await;
            });
            None
        } else {
            self.dispatch_request(
                WallpaperRequestReason::WorkspaceChanged,
                WallpaperMode::Manual,
                target,
            )
        }
    }

    pub fn on_settings_changed(&mut self) -> Option<(ExtensionId, ExtensionEvent)> {
        if self.active_provider.is_some() {
            self.dispatch_request(
                WallpaperRequestReason::SettingsChanged,
                WallpaperMode::Manual,
                WallpaperTarget::Global,
            )
        } else {
            None
        }
    }

    pub fn on_slideshow_tick(&mut self) -> Option<(ExtensionId, ExtensionEvent)> {
        if self.active_provider.is_some() && self.active_modes.contains(&WallpaperMode::Slideshow) {
            self.dispatch_request(
                WallpaperRequestReason::SlideshowTick,
                WallpaperMode::Slideshow,
                WallpaperTarget::Global,
            )
        } else {
            None
        }
    }

    pub fn on_wallpaper_tick(&mut self, now: Instant) -> Option<(ExtensionId, ExtensionEvent)> {
        if now.duration_since(self.last_slideshow_tick) < self.slideshow_interval {
            return None;
        }
        self.last_slideshow_tick = now;
        self.on_slideshow_tick()
    }

    pub fn dispatch_request(
        &mut self,
        reason: WallpaperRequestReason,
        mode: WallpaperMode,
        target: WallpaperTarget,
    ) -> Option<(ExtensionId, ExtensionEvent)> {
        let (Some(provider_id), Some(current_gen)) =
            (&self.active_provider, self.provider_generation)
        else {
            return None;
        };

        // Check if mode is supported
        if !self.active_modes.contains(&mode) {
            return None;
        }

        // Check if target kind is supported
        let target_kind = match &target {
            WallpaperTarget::Global => WallpaperTargetKind::Global,
            WallpaperTarget::Workspace(_) => WallpaperTargetKind::Workspace,
        };
        if !self.active_targets.contains(&target_kind) {
            return None;
        }

        self.next_request_seq += 1;
        let req_id = format!("wp-req-{}", self.next_request_seq);

        // Coalesce: remove any superseded request for the same target
        if let Some(old_req_id) = self
            .active_request_by_target
            .insert(target.clone(), req_id.clone())
        {
            self.pending_requests.remove(&old_req_id);
        }

        self.pending_requests.insert(
            req_id.clone(),
            PendingWallpaperRequest {
                generation: current_gen,
                extension_id: provider_id.extension_id.clone(),
                target: target.clone(),
                mode,
                reason,
            },
        );

        // Bound pending requests table to 32 entries
        if self.pending_requests.len() > 32
            && let Some(oldest_key) = self.pending_requests.keys().next().cloned()
        {
            self.pending_requests.remove(&oldest_key);
        }

        let event = ExtensionEvent::WallpaperRequest(WallpaperRequest {
            request_id: req_id,
            contribution_id: provider_id.contribution_id.to_string(),
            reason,
            mode,
            target,
        });

        Some((provider_id.extension_id.clone(), event))
    }

    pub fn validate_wallpaper_effect(
        &mut self,
        _path: &str,
        source: WallpaperSource,
        request_id: &Option<String>,
        target: Option<WallpaperTarget>,
        extension_id: &ExtensionId,
        generation: ExtensionGeneration,
    ) -> Result<WallpaperTarget, Option<String>> {
        if source == WallpaperSource::Remote {
            return Err(Some(
                "remote wallpaper source is unsupported without a host-owned downloader".into(),
            ));
        }

        if let Some(req_id) = request_id {
            match self.pending_requests.remove(req_id) {
                Some(pending) => {
                    if pending.generation != generation || pending.extension_id != *extension_id {
                        return Err(None);
                    }
                    if self.active_request_by_target.get(&pending.target) != Some(req_id) {
                        return Err(None);
                    }
                    Ok(pending.target)
                }
                None => Err(None),
            }
        } else {
            Ok(target.unwrap_or(WallpaperTarget::Global))
        }
    }

    pub fn record_successful_wallpaper(&mut self, target: WallpaperTarget, path: String) {
        self.last_valid_wallpapers.insert(target, path);
    }
}

#[cfg(test)]
mod tests {
    use shilpo_ext_api::ContributionId;

    use super::*;

    #[test]
    fn coordinator_tracks_active_provider_and_discards_stale_replies() {
        let mut coordinator = WallpaperCoordinator::new();
        assert!(coordinator.active_provider().is_none());
        assert!(
            coordinator
                .last_valid_wallpaper(&WallpaperTarget::Global)
                .is_none()
        );

        let ext_id = ExtensionId::new("org.shilpo.wallpaper").unwrap();
        let contrib_id = ContributionId::new("provider").unwrap();
        let canonical = CanonicalId::new(ext_id.clone(), contrib_id);

        coordinator.active_provider = Some(canonical.clone());
        coordinator.active_modes = vec![WallpaperMode::Manual, WallpaperMode::Slideshow];
        coordinator.active_targets =
            vec![WallpaperTargetKind::Global, WallpaperTargetKind::Workspace];
        coordinator.provider_generation = Some(ExtensionGeneration(1));

        // Simulate a pending request
        coordinator.pending_requests.insert(
            "wp-req-1".into(),
            PendingWallpaperRequest {
                generation: ExtensionGeneration(1),
                extension_id: ext_id.clone(),
                target: WallpaperTarget::Global,
                mode: WallpaperMode::Manual,
                reason: WallpaperRequestReason::Activate,
            },
        );
        coordinator
            .active_request_by_target
            .insert(WallpaperTarget::Global, "wp-req-1".into());

        // Stale generation reply should fail validation
        let result_stale = coordinator.validate_wallpaper_effect(
            "/path/to/wall.png",
            WallpaperSource::LocalFile,
            &Some("wp-req-1".into()),
            None,
            &ext_id,
            ExtensionGeneration(2),
        );
        assert!(result_stale.is_err());

        // Successful wallpaper recorded
        coordinator
            .record_successful_wallpaper(WallpaperTarget::Global, "/path/to/wall.png".into());
        assert_eq!(
            coordinator.last_valid_wallpaper(&WallpaperTarget::Global),
            Some("/path/to/wall.png")
        );

        // On provider disable, last valid wallpaper remains visible
        coordinator.on_provider_disabled_or_unloaded();
        assert!(coordinator.active_provider().is_none());
        assert_eq!(
            coordinator.last_valid_wallpaper(&WallpaperTarget::Global),
            Some("/path/to/wall.png")
        );
    }

    fn descriptor(id: CanonicalId) -> ContributionDescriptor {
        ContributionDescriptor {
            id,
            extension_name: "Wallpaper".into(),
            name: "Provider".into(),
            surface: ContributionSurface::Wallpaper,
            runtime_kind: shilpo_ext_runtime::worker::protocol::ExtensionRuntimeKind::Wasm,
            settings_schema: None,
            default_size: None,
            minimum_size: None,
            bar_widget: None,
            action: None,
            default_binding: None,
            wallpaper_modes: Some(vec![WallpaperMode::Manual, WallpaperMode::Slideshow]),
            wallpaper_targets: Some(vec![WallpaperTargetKind::Global]),
            search_modes: None,
        }
    }

    #[test]
    fn provider_selection_requires_exact_configured_canonical_id() {
        let extension_id = ExtensionId::new("org.shilpo.wallpaper").unwrap();
        let provider = CanonicalId::new(
            extension_id.clone(),
            ContributionId::new("provider").unwrap(),
        );
        let mut coordinator = WallpaperCoordinator::new();
        let descriptors = vec![descriptor(provider.clone())];
        let config = ExtensionsConfig::default();
        assert!(
            coordinator
                .sync_active_provider(&config, &descriptors, ExtensionGeneration(1))
                .is_none()
        );
        assert!(coordinator.active_provider().is_none());

        let config = ExtensionsConfig {
            wallpaper_provider: Some(provider.clone()),
            ..ExtensionsConfig::default()
        };
        assert!(
            coordinator
                .sync_active_provider(&config, &descriptors, ExtensionGeneration(1))
                .is_some()
        );
        assert_eq!(coordinator.active_provider(), Some(&provider));
    }

    #[test]
    fn slideshow_tick_is_host_scheduled_and_coalesced() {
        let extension_id = ExtensionId::new("org.shilpo.wallpaper").unwrap();
        let provider = CanonicalId::new(
            extension_id.clone(),
            ContributionId::new("provider").unwrap(),
        );
        let mut coordinator = WallpaperCoordinator::new();
        let config = ExtensionsConfig {
            wallpaper_provider: Some(provider),
            settings: [(
                extension_id.to_string(),
                serde_json::json!({"slideshow_interval_seconds": 30}),
            )]
            .into_iter()
            .collect(),
        };
        let descriptors = vec![descriptor(coordinator_test_provider(&config))];
        coordinator.sync_active_provider(&config, &descriptors, ExtensionGeneration(1));
        coordinator.slideshow_interval = Duration::ZERO;
        let first = coordinator.on_wallpaper_tick(Instant::now());
        assert!(matches!(
            first,
            Some((_, ExtensionEvent::WallpaperRequest(_)))
        ));
        assert!(coordinator.on_wallpaper_tick(Instant::now()).is_some());
    }

    fn coordinator_test_provider(config: &ExtensionsConfig) -> CanonicalId {
        config
            .wallpaper_provider
            .clone()
            .expect("provider configured")
    }
}
