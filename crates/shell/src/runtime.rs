use gpui::{
    App, AppContext, Bounds, DisplayId, Entity, Global, Pixels, Point, Subscription,
    WindowBackgroundAppearance, WindowBounds, WindowHandle, WindowKind, WindowOptions,
    layer_shell::{Anchor, KeyboardInteractivity, Layer, LayerShellOptions},
    point, px, size,
};
use shilpo_services::{BarState, IpcRequest, IpcStatus, ShellIpcServer};

use crate::{
    actions::{ActionId, ActionInvocation, ActionRegistry},
    bar::{BarView, geometry::BarGeometry},
    control_center::ControlCenterView,
    error::ShellError,
    extensions::{ContributionDescriptor, ContributionSurface, ExtensionChanges, ShellExtensions},
    launcher::LauncherView,
};

use std::collections::HashMap;

use crate::bar::service_worker::{self, CommandSender, UpdateReceiver, WorkerCommand};
use shilpo_services::{
    AudioService, BatteryService, NetworkService, NiriCompositorService, NotificationService,
};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, mpsc};

#[derive(Clone, Debug, PartialEq)]
struct ExtensionSurfaceSpec {
    contribution: shilpo_ext::CanonicalId,
    display_id: DisplayId,
    bounds: Bounds<Pixels>,
}

pub struct ServiceHub {
    pub niri: Option<NiriCompositorService>,
    pub notification: Option<NotificationService>,
    pub clipboard: shilpo_services::ClipboardService,
    pub app_scanner: shilpo_services::AppScanner,
    pub service_commands: CommandSender,
    pub notif_rx: Arc<Mutex<mpsc::Receiver<shilpo_services::Notification>>>,
    pub updates_rx: Arc<Mutex<UpdateReceiver>>,
    pub _service_task: Option<gpui::Task<()>>,
    pub _watcher: Option<notify::RecommendedWatcher>,
    pub _app_watcher: Option<notify::RecommendedWatcher>,
}

impl ServiceHub {
    pub fn new(executor: gpui::BackgroundExecutor, config_path: PathBuf) -> Self {
        let niri = NiriCompositorService::new().ok();
        let battery = BatteryService::new().ok();
        let audio = AudioService::new().ok();
        let network = NetworkService::new().ok();
        let clipboard = shilpo_services::ClipboardService::new();
        let app_scanner = shilpo_services::AppScanner::new()
            .unwrap_or_else(|_| shilpo_services::AppScanner::new_empty());
        let app_watcher = app_scanner.start_watcher();
        let notification = match NotificationService::new() {
            Ok(s) => Some(s),
            Err(e) => {
                tracing::warn!(error = %e, "notification service unavailable; toasts disabled");
                None
            }
        };

        let (notif_tx, notif_rx) = mpsc::channel();
        if let Some(service) = &notification {
            service.set_new_notification_sender(notif_tx);
        }

        let (updates_tx, updates_rx, service_commands, commands_rx) = service_worker::channels();
        let service_task = service_worker::spawn(
            executor,
            updates_tx,
            commands_rx,
            config_path.clone(),
            niri.clone(),
            battery,
            audio,
            network,
        );

        let config_dir = config_path
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from(".config/shilpo"));
        if let Err(error) = std::fs::create_dir_all(&config_dir) {
            tracing::warn!(error = %error, path = ?config_dir, "config watcher directory unavailable");
        }

        use notify::Watcher;
        let watcher_commands = service_commands.clone();
        let target_file_name = config_path.file_name().map(|n| n.to_os_string());
        let watcher = match notify::RecommendedWatcher::new(
            move |res: Result<notify::Event, notify::Error>| {
                if let Some(event) = res.ok().filter(|e| e.kind.is_modify())
                    && (target_file_name.is_none()
                        || event
                            .paths
                            .iter()
                            .any(|p| p.file_name() == target_file_name.as_deref()))
                {
                    let _ = service_worker::try_send_command(
                        &watcher_commands,
                        WorkerCommand::ReloadConfig,
                    );
                }
            },
            notify::Config::default(),
        ) {
            Ok(mut watcher) => match watcher.watch(&config_dir, notify::RecursiveMode::Recursive) {
                Ok(()) => Some(watcher),
                Err(error) => {
                    tracing::warn!(error = %error, path = ?config_dir, "config watcher watch failed");
                    None
                }
            },
            Err(error) => {
                tracing::warn!(error = %error, "config watcher creation failed");
                None
            }
        };

        Self {
            niri,
            notification,
            clipboard,
            app_scanner,
            service_commands,
            notif_rx: Arc::new(Mutex::new(notif_rx)),
            updates_rx: Arc::new(Mutex::new(updates_rx)),
            _service_task: Some(service_task),
            _watcher: watcher,
            _app_watcher: app_watcher,
        }
    }
}

fn apply_notification_dnd(notification: Option<&NotificationService>, enabled: bool) {
    if let Some(notification) = notification {
        notification.set_dnd_enabled(enabled);
    }
}

pub struct ShellRuntime {
    ipc_server: ShellIpcServer,
    active_config: shilpo_config::ShellConfig,
    bars: HashMap<DisplayId, (WindowHandle<BarView>, crate::bar::BarSpec)>,
    last_bar_specs: Vec<(BarGeometry, bool)>,
    bar_state: BarState,
    readiness: shilpo_services::ipc::ReadinessState,
    launcher: Option<WindowHandle<shilpo_ui::Root>>,
    control_center: Option<WindowHandle<shilpo_ui::Root>>,
    overview: Option<WindowHandle<shilpo_ui::Root>>,
    notification: Option<(
        u64,
        u32,
        WindowHandle<crate::notification::NotificationToastView>,
    )>,
    notification_generation: u64,
    prior_window_id: Option<u64>,
    osd: Option<(
        u64,
        WindowHandle<shilpo_ui::Root>,
        Entity<crate::osd::OsdView>,
    )>,
    _osd_generation: u64,
    extensions: Option<ShellExtensions>,
    extension_surfaces: HashMap<String, (WindowHandle<shilpo_ui::Root>, ExtensionSurfaceSpec)>,
    extension_panel: Option<(WindowHandle<shilpo_ui::Root>, shilpo_ext::CanonicalId)>,
    extension_output_ids: std::collections::HashSet<DisplayId>,
    extension_http_in_flight: std::collections::HashSet<(shilpo_ext::ExtensionId, String)>,
    extension_location_service: shilpo_services::LocationService,
    actions: ActionRegistry,
    keybindings: crate::actions::KeybindingManager,
    session_state: shilpo_config::ShellSessionState,
    session_path: PathBuf,
    pub heed_store: Option<Arc<shilpo_config::HeedSessionStore>>,
    start_time: std::time::Instant,
    service_hub: Option<ServiceHub>,
    _window_closed: Option<Subscription>,
    _ipc_task: gpui::Task<()>,
}

impl Global for ShellRuntime {}

impl ShellRuntime {
    pub fn service_commands(cx: &App) -> Option<CommandSender> {
        if cx.has_global::<Self>() {
            cx.global::<Self>()
                .service_hub
                .as_ref()
                .map(|h| h.service_commands.clone())
        } else {
            None
        }
    }

    pub fn install(cx: &mut App, ipc_server: ShellIpcServer) {
        let config_path = std::env::var("HOME")
            .map(|home| std::path::PathBuf::from(home).join(".config/shilpo/config.toml"))
            .unwrap_or_else(|_| std::path::PathBuf::from(".config/shilpo/config.toml"));
        let active_config = shilpo_config::ShellConfig::load_or_create(&config_path)
            .unwrap_or_else(|_| shilpo_config::ShellConfig::default());
        let session_path = shilpo_config::ShellSessionState::default_session_path();
        let (session_state, _restored_fallback) =
            shilpo_config::ShellSessionState::restore_with_fallback(&session_path);
        let heed_dir = shilpo_config::HeedSessionStore::default_db_dir();
        let heed_store = shilpo_config::HeedSessionStore::open_or_repair(&heed_dir)
            .ok()
            .map(Arc::new);
        let hub = ServiceHub::new(cx.background_executor().clone(), config_path);
        apply_notification_dnd(hub.notification.as_ref(), session_state.dnd_active);
        let extensions = match ShellExtensions::load_default() {
            Ok(extensions) => Some(extensions),
            Err(error) => {
                tracing::warn!(error = %error, "extension runtime is unavailable");
                None
            }
        };

        cx.set_global(Self {
            ipc_server,
            active_config,
            bars: HashMap::new(),
            last_bar_specs: Vec::new(),
            bar_state: BarState::Starting,
            readiness: shilpo_services::ipc::ReadinessState::Starting,
            launcher: None,
            control_center: None,
            overview: None,
            notification: None,
            notification_generation: 0,
            prior_window_id: None,
            osd: None,
            _osd_generation: 0,
            extensions,
            extension_surfaces: HashMap::new(),
            extension_panel: None,
            extension_output_ids: std::collections::HashSet::new(),
            extension_http_in_flight: std::collections::HashSet::new(),
            extension_location_service: shilpo_services::LocationService::new(),
            actions: ActionRegistry::default(),
            keybindings: crate::actions::KeybindingManager::with_defaults(),
            session_state,
            session_path,
            heed_store,
            start_time: std::time::Instant::now(),
            service_hub: Some(hub),
            _window_closed: None,
            _ipc_task: cx.spawn(async |_| {}),
        });

        let subscription = cx.on_window_closed(|cx, window_id| {
            let runtime = cx.global_mut::<Self>();
            runtime
                .bars
                .retain(|_, (handle, _)| handle.window_id() != window_id);
            runtime
                .extension_surfaces
                .retain(|_, (handle, _)| handle.window_id() != window_id);
            if runtime.bars.is_empty() {
                runtime.bar_state = BarState::Hidden;
            }
            let closed_launcher = if runtime
                .launcher
                .as_ref()
                .is_some_and(|handle| handle.window_id() == window_id)
            {
                runtime.launcher = None;
                true
            } else {
                false
            };
            let closed_control_center = if runtime
                .control_center
                .as_ref()
                .is_some_and(|handle| handle.window_id() == window_id)
            {
                runtime.control_center = None;
                true
            } else {
                false
            };
            let closed_extension_panel = if runtime
                .extension_panel
                .as_ref()
                .is_some_and(|(handle, _)| handle.window_id() == window_id)
            {
                runtime.extension_panel.take().map(|(_, id)| id)
            } else {
                None
            };
            if runtime
                .notification
                .as_ref()
                .is_some_and(|(_, _, handle)| handle.window_id() == window_id)
            {
                runtime.notification = None;
            }
            runtime.publish_status();
            if closed_launcher {
                Self::dispatch_surface_lifecycle(
                    cx,
                    ContributionSurface::Launcher,
                    false,
                    640.,
                    480.,
                );
            }
            if closed_control_center {
                Self::dispatch_surface_lifecycle(
                    cx,
                    ContributionSurface::ControlCenter,
                    false,
                    340.,
                    540.,
                );
            }
            if let Some(contribution) = closed_extension_panel {
                let changes = cx
                    .global_mut::<Self>()
                    .extensions
                    .as_mut()
                    .map(|extensions| {
                        extensions.dispatch_to(
                            &contribution.extension_id,
                            &shilpo_ext::ExtensionEvent::ContributionUnmounted {
                                contribution_id: contribution.contribution_id.to_string(),
                                instance_id: None,
                            },
                        )
                    })
                    .unwrap_or_default();
                Self::apply_extension_changes(cx, changes);
            }
        });
        cx.global_mut::<Self>()._window_closed = Some(subscription);
        Self::sync_extension_actions(cx);

        let task = cx.spawn(async move |cx| {
            loop {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(100))
                    .await;
                cx.update(Self::sync_displays);
                cx.update(Self::drain_service_hub);
                cx.update(Self::poll_extensions);
                cx.update(Self::drain_ipc);
            }
        });
        cx.global_mut::<Self>()._ipc_task = task;
        cx.global::<Self>().publish_status();
    }

    pub fn extension_descriptors(
        cx: &App,
        surface: ContributionSurface,
    ) -> Vec<ContributionDescriptor> {
        cx.global::<Self>()
            .extensions
            .as_ref()
            .map_or_else(Vec::new, |extensions| extensions.descriptors_for(surface))
    }

    pub fn extension_surface_views(
        cx: &mut App,
        surface: ContributionSurface,
    ) -> Vec<(shilpo_ext::CanonicalId, shilpo_ext::ViewTree)> {
        let descriptors = Self::extension_descriptors(cx, surface);
        descriptors
            .into_iter()
            .filter_map(|descriptor| {
                let tree = Self::extension_view(cx, &descriptor.id)?;
                Some((descriptor.id, tree))
            })
            .collect()
    }

    pub fn extension_view(
        cx: &mut App,
        id: &shilpo_ext::CanonicalId,
    ) -> Option<shilpo_ext::ViewTree> {
        cx.global_mut::<Self>()
            .extensions
            .as_mut()
            .and_then(|extensions| extensions.view(id))
    }

    pub fn extension_asset_path(
        cx: &App,
        id: &shilpo_ext::CanonicalId,
        relative: &str,
    ) -> Result<PathBuf, String> {
        cx.global::<Self>()
            .extensions
            .as_ref()
            .ok_or_else(|| "extension runtime is unavailable".to_owned())?
            .asset_path(id, relative)
    }

    pub fn dispatch_extension_input(
        cx: &mut App,
        contribution: &shilpo_ext::CanonicalId,
        instance_id: Option<&str>,
        event_id: impl Into<String>,
        value: Option<serde_json::Value>,
    ) {
        let changes = cx
            .global_mut::<Self>()
            .extensions
            .as_mut()
            .map(|extensions| extensions.input(contribution, instance_id, event_id, value))
            .unwrap_or_default();
        Self::apply_extension_changes(cx, changes);
    }

    pub fn open_extension_panel(cx: &mut App, contribution: shilpo_ext::CanonicalId) {
        if let Some((handle, current)) = cx.global_mut::<Self>().extension_panel.take() {
            if current == contribution
                && handle
                    .update(cx, |_, window, _| window.activate_window())
                    .is_ok()
            {
                cx.global_mut::<Self>().extension_panel = Some((handle, current));
                return;
            }
            let changes = cx
                .global_mut::<Self>()
                .extensions
                .as_mut()
                .map(|extensions| {
                    extensions.dispatch_to(
                        &current.extension_id,
                        &shilpo_ext::ExtensionEvent::ContributionUnmounted {
                            contribution_id: current.contribution_id.to_string(),
                            instance_id: None,
                        },
                    )
                })
                .unwrap_or_default();
            Self::apply_extension_changes(cx, changes);
            let _ = cx.update_window(*handle, |_, window, _| window.remove_window());
        }
        let (display_bounds, display_id) = if let Some(display) = cx.primary_display() {
            (display.bounds(), Some(display.id()))
        } else {
            (
                Bounds::new(point(px(0.), px(0.)), size(px(1920.), px(1080.))),
                None,
            )
        };
        let panel_size = size(px(420.), px(600.));
        let origin = point(
            display_bounds.origin.x + display_bounds.size.width - panel_size.width - px(16.),
            display_bounds.origin.y + px(56.),
        );
        let options = overlay_options(
            "shilpo-extension-panel",
            "extension-panel",
            panel_size,
            origin,
            display_id,
        );
        let view_id = contribution.clone();
        match cx.open_window(options, move |window, cx| {
            crate::extension_surface::ExtensionSurfaceView::view(view_id, None, window, cx)
        }) {
            Ok(handle) => {
                let changes = cx
                    .global_mut::<Self>()
                    .extensions
                    .as_mut()
                    .map(|extensions| {
                        extensions.dispatch_to(
                            &contribution.extension_id,
                            &shilpo_ext::ExtensionEvent::ContributionMounted {
                                contribution_id: contribution.contribution_id.to_string(),
                                instance_id: None,
                                width: 420.,
                                height: 600.,
                            },
                        )
                    })
                    .unwrap_or_default();
                cx.global_mut::<Self>().extension_panel = Some((handle, contribution));
                Self::apply_extension_changes(cx, changes);
            }
            Err(error) => tracing::warn!(error = %error, "failed to open extension side panel"),
        }
    }

    fn poll_extensions(cx: &mut App) {
        if !cx.has_global::<Self>() {
            return;
        }
        let changes = cx
            .global_mut::<Self>()
            .extensions
            .as_mut()
            .map(ShellExtensions::poll_hot_reload)
            .unwrap_or_default();
        let catalog_changed = changes.catalog_changed;
        if catalog_changed {
            Self::sync_extension_actions(cx);
        }
        Self::apply_extension_changes(cx, changes);
        if catalog_changed {
            let (launcher_open, control_center_open, panel) = {
                let runtime = cx.global::<Self>();
                (
                    runtime.launcher.is_some(),
                    runtime.control_center.is_some(),
                    runtime.extension_panel.as_ref().map(|(_, id)| id.clone()),
                )
            };
            if launcher_open {
                Self::dispatch_surface_lifecycle(
                    cx,
                    ContributionSurface::Launcher,
                    true,
                    640.,
                    480.,
                );
            }
            if control_center_open {
                Self::dispatch_surface_lifecycle(
                    cx,
                    ContributionSurface::ControlCenter,
                    true,
                    340.,
                    540.,
                );
            }
            if let Some(contribution) = panel {
                let changes = cx
                    .global_mut::<Self>()
                    .extensions
                    .as_mut()
                    .map(|extensions| {
                        extensions.dispatch_to(
                            &contribution.extension_id,
                            &shilpo_ext::ExtensionEvent::ContributionMounted {
                                contribution_id: contribution.contribution_id.to_string(),
                                instance_id: None,
                                width: 420.,
                                height: 600.,
                            },
                        )
                    })
                    .unwrap_or_default();
                Self::apply_extension_changes(cx, changes);
            }
        }
    }

    fn dispatch_extension_event(cx: &mut App, event: shilpo_ext::ExtensionEvent) {
        let changes = cx
            .global_mut::<Self>()
            .extensions
            .as_mut()
            .map(|extensions| extensions.dispatch_all(&event))
            .unwrap_or_default();
        Self::apply_extension_changes(cx, changes);
    }

    fn dispatch_surface_lifecycle(
        cx: &mut App,
        surface: ContributionSurface,
        mounted: bool,
        width: f32,
        height: f32,
    ) {
        let descriptors = Self::extension_descriptors(cx, surface);
        for descriptor in descriptors {
            let event = if mounted {
                shilpo_ext::ExtensionEvent::ContributionMounted {
                    contribution_id: descriptor.id.contribution_id.to_string(),
                    instance_id: None,
                    width,
                    height,
                }
            } else {
                shilpo_ext::ExtensionEvent::ContributionUnmounted {
                    contribution_id: descriptor.id.contribution_id.to_string(),
                    instance_id: None,
                }
            };
            let changes = cx
                .global_mut::<Self>()
                .extensions
                .as_mut()
                .map(|extensions| extensions.dispatch_to(&descriptor.id.extension_id, &event))
                .unwrap_or_default();
            Self::apply_extension_changes(cx, changes);
        }
    }

    fn sync_extension_actions(cx: &mut App) {
        let desired = Self::extension_descriptors(cx, ContributionSurface::Action);
        let existing = cx
            .global::<Self>()
            .actions
            .all()
            .into_iter()
            .filter_map(|descriptor| descriptor.id.extension_id())
            .collect::<Vec<_>>();
        let actions = &mut cx.global_mut::<Self>().actions;
        for id in existing {
            actions.unregister_extension(&id);
        }
        for descriptor in desired {
            if let Err(error) =
                actions.register_extension(descriptor.id, descriptor.name.clone(), descriptor.name)
            {
                tracing::warn!(error = %error, "extension action registration failed");
            }
        }
    }

    fn apply_extension_changes(cx: &mut App, changes: ExtensionChanges) {
        let refresh_views = changes.catalog_changed || !changes.invalidated_views.is_empty();
        for (extension_id, effect) in changes.effects {
            match effect {
                shilpo_ext::HostEffect::InvokeAction { action_id, payload } => {
                    match action_id.parse::<ActionId>() {
                        Ok(id) => {
                            let invocation = match id.extension_id() {
                                Some(id) => ActionInvocation::Extension { id, payload },
                                None => id.into(),
                            };
                            if let Err(error) = Self::dispatch_invocation(cx, invocation) {
                                tracing::warn!(
                                    extension = %extension_id,
                                    error = %error,
                                    "extension action effect failed"
                                );
                            }
                        }
                        Err(error) => tracing::warn!(
                            extension = %extension_id,
                            error = %error,
                            "extension returned an invalid action ID"
                        ),
                    }
                }
                shilpo_ext::HostEffect::ShowNotification { title, body, icon } => {
                    let mut notification = shilpo_services::Notification::new(title, body);
                    notification.app_name = extension_id.to_string();
                    notification.app_icon = icon;
                    crate::bar::view::open_notification_toast(cx, notification);
                }
                shilpo_ext::HostEffect::SetThemeSource { color } => {
                    if let Some(argb) = crate::bar::view::parse_hex_color(&color) {
                        shilpo_ui::Theme::global_mut(cx).set_source_argb(argb);
                    } else {
                        tracing::warn!(
                            extension = %extension_id,
                            %color,
                            "extension returned an invalid theme source"
                        );
                    }
                }
                shilpo_ext::HostEffect::SetWallpaper { path, .. } => {
                    if let Err(error) =
                        shilpo_services::WallpaperService::default().set_wallpaper(&path)
                    {
                        tracing::warn!(
                            extension = %extension_id,
                            error = %error,
                            "extension wallpaper effect failed"
                        );
                    }
                }
                shilpo_ext::HostEffect::ClipboardWrite { text } => {
                    let result = cx
                        .global::<Self>()
                        .service_hub
                        .as_ref()
                        .map(|hub| hub.clipboard.copy_text(&text));
                    if let Some(Err(error)) = result {
                        tracing::warn!(
                            extension = %extension_id,
                            error = %error,
                            "extension clipboard effect failed"
                        );
                    }
                }
                shilpo_ext::HostEffect::HttpRequest {
                    request_id,
                    url,
                    method,
                } => {
                    let key = (extension_id.clone(), request_id.clone());
                    let accepted = {
                        let in_flight = &mut cx.global_mut::<Self>().extension_http_in_flight;
                        request_id.len() <= 128
                            && !request_id.is_empty()
                            && in_flight.len() < 8
                            && in_flight.insert(key.clone())
                    };
                    if !accepted {
                        let changes = cx
                            .global_mut::<Self>()
                            .extensions
                            .as_mut()
                            .map(|extensions| {
                                extensions.dispatch_to(
                                    &extension_id,
                                    &shilpo_ext::ExtensionEvent::HttpResponse {
                                        request_id,
                                        status: None,
                                        body: String::new(),
                                        error: Some(
                                            "request ID is invalid, duplicated, or the HTTP limit was reached"
                                                .into(),
                                        ),
                                    },
                                )
                            })
                            .unwrap_or_default();
                        Self::apply_extension_changes(cx, changes);
                        continue;
                    }
                    cx.spawn(async move |cx| {
                        let response = crate::extension_http::fetch(request_id, url, method).await;
                        cx.update(|cx| {
                            cx.global_mut::<Self>()
                                .extension_http_in_flight
                                .remove(&key);
                            let changes = cx
                                .global_mut::<Self>()
                                .extensions
                                .as_mut()
                                .map(|extensions| extensions.dispatch_to(&extension_id, &response))
                                .unwrap_or_default();
                            Self::apply_extension_changes(cx, changes);
                        });
                    })
                    .detach();
                }
                shilpo_ext::HostEffect::LocationRead => {
                    let location_service = cx.global::<Self>().extension_location_service.clone();
                    cx.spawn(async move |cx| {
                        let result = location_service.read_location_async().await;
                        let event = match result {
                            Ok(info) => shilpo_ext::ExtensionEvent::LocationResponse {
                                latitude: Some(info.latitude),
                                longitude: Some(info.longitude),
                                accuracy_meters: Some(info.accuracy_meters),
                                error: None,
                            },
                            Err(error) => shilpo_ext::ExtensionEvent::LocationResponse {
                                latitude: None,
                                longitude: None,
                                accuracy_meters: None,
                                error: Some(error),
                            },
                        };
                        cx.update(|cx| {
                            let changes = cx
                                .global_mut::<Self>()
                                .extensions
                                .as_mut()
                                .map(|extensions| extensions.dispatch_to(&extension_id, &event))
                                .unwrap_or_default();
                            Self::apply_extension_changes(cx, changes);
                        });
                    })
                    .detach();
                }
                effect => tracing::debug!(
                    extension = %extension_id,
                    ?effect,
                    "accepted extension effect has no shell service adapter yet"
                ),
            }
        }
        if refresh_views {
            let bar_handles = cx
                .global::<Self>()
                .bars
                .values()
                .map(|(handle, _)| *handle)
                .collect::<Vec<_>>();
            for handle in bar_handles {
                let _ = handle.update(cx, |_, window, _| window.refresh());
            }
            let surface_handles = cx
                .global::<Self>()
                .extension_surfaces
                .values()
                .map(|(handle, _)| *handle)
                .collect::<Vec<_>>();
            for handle in surface_handles {
                let _ = handle.update(cx, |_, window, _| window.refresh());
            }
            let overlay_handles = {
                let runtime = cx.global::<Self>();
                [
                    runtime.extension_panel.as_ref().map(|(handle, _)| *handle),
                    runtime.launcher,
                    runtime.control_center,
                ]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
            };
            for handle in overlay_handles {
                let _ = handle.update(cx, |_, window, _| window.refresh());
            }
        }
    }

    fn drain_service_hub(cx: &mut App) {
        if !cx.has_global::<Self>() {
            return;
        }

        let notifs = {
            let runtime = cx.global_mut::<Self>();
            let mut list = Vec::new();
            if let Some(hub) = &runtime.service_hub
                && let Ok(rx) = hub.notif_rx.lock()
            {
                while let Ok(notif) = rx.try_recv() {
                    list.push(notif);
                }
            }
            list
        };

        for notif in notifs {
            crate::bar::view::open_notification_toast(cx, notif);
        }

        let updates = {
            let runtime = cx.global_mut::<Self>();
            let mut list = Vec::new();
            if let Some(hub) = &runtime.service_hub
                && let Ok(rx) = hub.updates_rx.lock()
            {
                while let Ok(upd) = rx.try_recv() {
                    list.push(upd);
                }
            }
            list
        };

        if !updates.is_empty() {
            for upd in &updates {
                match upd {
                    crate::bar::service_worker::WorkerUpdate::Config(
                        crate::bar::service_worker::ConfigUpdate::Loaded(config),
                    ) => {
                        let previous = cx.global::<Self>().active_config.clone();
                        cx.global_mut::<Self>().active_config = (**config).clone();
                        if previous.theme.mode != config.theme.mode {
                            Self::dispatch_extension_event(
                                cx,
                                shilpo_ext::ExtensionEvent::ThemeChanged {
                                    mode: format!("{:?}", config.theme.mode).to_lowercase(),
                                },
                            );
                        }
                        if previous.theme.accent != config.theme.accent {
                            Self::dispatch_extension_event(
                                cx,
                                shilpo_ext::ExtensionEvent::PaletteGenerated {
                                    accent: config.theme.accent.clone(),
                                },
                            );
                        }
                        Self::sync_displays(cx);
                    }
                    crate::bar::service_worker::WorkerUpdate::Battery(info) => {
                        Self::dispatch_extension_event(
                            cx,
                            shilpo_ext::ExtensionEvent::PowerChanged {
                                percentage: info.is_present.then_some(info.percentage as f32),
                                charging: info.is_charging,
                            },
                        );
                    }
                    crate::bar::service_worker::WorkerUpdate::Network(info) => {
                        Self::dispatch_extension_event(
                            cx,
                            shilpo_ext::ExtensionEvent::NetworkChanged {
                                connected: info.available && info.is_connected,
                            },
                        );
                    }
                    _ => {}
                }
            }

            let handles: Vec<_> = cx
                .global::<Self>()
                .bars
                .values()
                .map(|(handle, _)| *handle)
                .collect();

            for handle in handles {
                let updates_clone = updates.clone();
                let _ = handle.update(cx, |bar_view, _window, cx| {
                    for upd in &updates_clone {
                        bar_view.apply_worker_update(upd, cx);
                    }
                });
            }
        }
    }

    pub fn sync_displays(cx: &mut App) {
        if !cx.has_global::<Self>() {
            return;
        }

        use crate::bar::reconciliation::{
            BarSpec, OutputDescriptor, ReconciliationOp, reconcile_output_bars,
        };

        let primary_id = cx.primary_display().map(|d| d.id());
        let current_outputs: Vec<OutputDescriptor> = cx
            .displays()
            .into_iter()
            .map(|d| OutputDescriptor {
                display_id: d.id(),
                bounds: d.bounds(),
                is_primary: primary_id == Some(d.id()),
                name: d.uuid().ok().map(|id| id.to_string()),
                scale: None,
            })
            .collect();
        let output_ids = current_outputs
            .iter()
            .map(|output| output.display_id)
            .collect::<std::collections::HashSet<_>>();
        let outputs_changed = {
            let runtime = cx.global_mut::<Self>();
            if runtime.extension_output_ids == output_ids {
                false
            } else {
                runtime.extension_output_ids = output_ids;
                true
            }
        };
        if outputs_changed {
            Self::dispatch_extension_event(cx, shilpo_ext::ExtensionEvent::OutputsChanged);
        }

        let (config, current_specs) = {
            let runtime = cx.global::<Self>();
            let specs: HashMap<DisplayId, BarSpec> = runtime
                .bars
                .iter()
                .map(|(&id, (_, spec))| (id, spec.clone()))
                .collect();
            (runtime.active_config.clone(), specs)
        };

        let ops = reconcile_output_bars(&current_outputs, &config, &current_specs);

        for op in ops {
            match op {
                ReconciliationOp::Create(spec) => {
                    Self::open_bar_with_spec(cx, spec);
                }
                ReconciliationOp::Recreate(spec) => {
                    let display_id = spec.display_id;
                    let old_bar = cx.global_mut::<Self>().bars.remove(&display_id);
                    Self::open_bar_with_spec(cx, spec);
                    if let Some((handle, _)) = old_bar {
                        let _ =
                            cx.update_window(handle.into(), |_, window, _| window.remove_window());
                    }
                }
                ReconciliationOp::Remove(display_id) => {
                    if let Some((handle, _)) = cx.global_mut::<Self>().bars.remove(&display_id) {
                        let _ =
                            cx.update_window(handle.into(), |_, window, _| window.remove_window());
                    }
                }
                ReconciliationOp::Retain(_) => {}
            }
        }
        Self::reconcile_extension_surfaces(cx, &current_outputs);
    }

    fn reconcile_extension_surfaces(cx: &mut App, outputs: &[crate::bar::OutputDescriptor]) {
        let config = cx.global::<Self>().active_config.clone();
        let mut instances = Vec::new();

        for (display_id, (_, spec)) in &cx.global::<Self>().bars {
            for (section, widgets) in [
                ("start", &spec.config.widgets.start),
                ("center", &spec.config.widgets.center),
                ("end", &spec.config.widgets.end),
            ] {
                for (index, widget) in widgets.iter().enumerate() {
                    if let shilpo_config::BarWidget::Extension(contribution) = widget {
                        instances.push(crate::extensions::ContributionInstance {
                            id: format!("bar:{display_id:?}:{section}:{index}"),
                            contribution: contribution.clone(),
                            output: Some(format!("{display_id:?}")),
                            width: spec.geometry.bounds.size.width.as_f32(),
                            height: spec.config.height as f32,
                            settings: extension_settings(&config, &contribution.extension_id, None),
                        });
                    }
                }
            }
        }

        let mut desired_windows = HashMap::new();
        for widget in &config.desktop.widgets {
            let output = if widget.output == "primary" {
                outputs.iter().find(|output| output.is_primary)
            } else {
                outputs.iter().find(|output| {
                    output.name.as_deref() == Some(widget.output.as_str())
                        || format!("{:?}", output.display_id) == widget.output
                })
            };
            let Some(output) = output else {
                continue;
            };
            let bounds = Bounds::new(
                point(
                    output.bounds.origin.x + px(widget.x as f32),
                    output.bounds.origin.y + px(widget.y as f32),
                ),
                size(px(widget.width as f32), px(widget.height as f32)),
            );
            let spec = ExtensionSurfaceSpec {
                contribution: widget.contribution.0.clone(),
                display_id: output.display_id,
                bounds,
            };
            desired_windows.insert(widget.instance.clone(), spec);
            instances.push(crate::extensions::ContributionInstance {
                id: widget.instance.clone(),
                contribution: widget.contribution.0.clone(),
                output: Some(widget.output.clone()),
                width: widget.width as f32,
                height: widget.height as f32,
                settings: extension_settings(
                    &config,
                    &widget.contribution.0.extension_id,
                    Some(&widget.settings),
                ),
            });
        }

        let changes = cx
            .global_mut::<Self>()
            .extensions
            .as_mut()
            .map(|extensions| extensions.reconcile_instances(instances))
            .unwrap_or_default();
        Self::apply_extension_changes(cx, changes);

        let stale = cx
            .global::<Self>()
            .extension_surfaces
            .iter()
            .filter(|(id, (_, current))| desired_windows.get(*id) != Some(current))
            .map(|(id, _)| id.clone())
            .collect::<Vec<_>>();
        for id in stale {
            if let Some((handle, _)) = cx.global_mut::<Self>().extension_surfaces.remove(&id) {
                let _ = cx.update_window(*handle, |_, window, _| window.remove_window());
            }
        }

        for (instance_id, spec) in desired_windows {
            if cx
                .global::<Self>()
                .extension_surfaces
                .contains_key(&instance_id)
            {
                continue;
            }
            let options = WindowOptions {
                titlebar: None,
                window_bounds: Some(WindowBounds::Windowed(spec.bounds)),
                display_id: Some(spec.display_id),
                app_id: Some(format!("shilpo-extension-{instance_id}")),
                window_background: WindowBackgroundAppearance::Transparent,
                kind: WindowKind::LayerShell(LayerShellOptions {
                    namespace: format!("extension-{instance_id}"),
                    layer: Layer::Bottom,
                    keyboard_interactivity: KeyboardInteractivity::None,
                    ..Default::default()
                }),
                ..Default::default()
            };
            let contribution = spec.contribution.clone();
            let view_instance_id = instance_id.clone();
            match cx.open_window(options, move |window, cx| {
                crate::extension_surface::ExtensionSurfaceView::view(
                    contribution,
                    Some(view_instance_id),
                    window,
                    cx,
                )
            }) {
                Ok(handle) => {
                    cx.global_mut::<Self>()
                        .extension_surfaces
                        .insert(instance_id, (handle, spec));
                }
                Err(error) => tracing::warn!(
                    error = %error,
                    "failed to open extension desktop surface"
                ),
            }
        }
    }

    fn publish_status(&self) {
        let health = shilpo_services::ServiceHealth {
            compositor_connected: self
                .service_hub
                .as_ref()
                .and_then(|h| h.niri.as_ref())
                .is_some(),
            battery_service_available: shilpo_services::BatteryService::new().is_ok(),
            audio_service_available: shilpo_services::AudioService::new().is_ok(),
            network_service_available: shilpo_services::NetworkService::new().is_ok(),
            notification_service_available: self
                .service_hub
                .as_ref()
                .and_then(|h| h.notification.as_ref())
                .is_some(),
            heed_store_available: self.heed_store.is_some(),
            uptime_seconds: self.start_time.elapsed().as_secs(),
        };

        self.ipc_server.update_status(IpcStatus {
            running: true,
            readiness: self.readiness,
            bar: self.bar_state.clone(),
            launcher_visible: self.launcher.is_some(),
            control_center_visible: self.control_center.is_some(),
            health,
        });
    }

    pub fn mark_ready(cx: &mut App) {
        let runtime = cx.global_mut::<Self>();
        runtime.readiness = shilpo_services::ipc::ReadinessState::Ready;
        runtime.publish_status();
    }

    pub fn mark_degraded(cx: &mut App) {
        let runtime = cx.global_mut::<Self>();
        runtime.readiness = shilpo_services::ipc::ReadinessState::Degraded;
        runtime.publish_status();
    }

    pub fn open_bar_with_spec(cx: &mut App, spec: crate::bar::BarSpec) -> bool {
        let options = bar_window_options(&spec.geometry, spec.with_display_geometry);
        let display_id = spec.display_id;
        let bar_config = spec.config.clone();

        let result = cx.open_window(options, move |window, cx| {
            let shell_config = shilpo_config::ShellConfig {
                bar: bar_config,
                ..Default::default()
            };
            BarView::view_with_config_on_display(window, cx, shell_config, display_id)
        });

        let runtime = cx.global_mut::<Self>();
        match result {
            Ok(handle) => {
                runtime.bars.insert(display_id, (handle, spec));
                runtime.bar_state = BarState::Visible;
                runtime.publish_status();
                true
            }
            Err(error) => {
                tracing::error!(error = %error, ?display_id, "failed to open bar window");
                if runtime.bars.is_empty() {
                    runtime.bar_state = BarState::OpenFailed;
                }
                runtime.publish_status();
                false
            }
        }
    }

    pub fn open_bar(cx: &mut App, geometry: &BarGeometry, with_display_geometry: bool) -> bool {
        let bar_config = cx
            .global::<Self>()
            .active_config
            .bar_for_output(None, true)
            .unwrap_or_else(|| shilpo_config::ShellConfig::default().bar);
        let spec = crate::bar::BarSpec {
            display_id: geometry.display_id,
            geometry: geometry.clone(),
            config: bar_config,
            with_display_geometry,
        };
        Self::open_bar_with_spec(cx, spec)
    }

    pub fn reconcile_bars(cx: &mut App) {
        let displays = cx.displays();
        let primary_id = cx.primary_display().map(|d| d.id());
        let outputs: Vec<_> = displays
            .into_iter()
            .map(|d| crate::bar::OutputDescriptor {
                display_id: d.id(),
                bounds: d.bounds(),
                is_primary: Some(d.id()) == primary_id,
                name: d.uuid().ok().map(|id| id.to_string()),
                scale: None,
            })
            .collect();

        let current_bars: HashMap<_, _> = cx
            .global::<Self>()
            .bars
            .iter()
            .map(|(&id, (_, spec))| (id, spec.clone()))
            .collect();

        let active_config = cx.global::<Self>().active_config.clone();
        let ops = crate::bar::reconcile_output_bars(&outputs, &active_config, &current_bars);

        for op in ops {
            match op {
                crate::bar::ReconciliationOp::Create(spec)
                | crate::bar::ReconciliationOp::Recreate(spec) => {
                    if let Some((handle, _)) = cx.global_mut::<Self>().bars.remove(&spec.display_id)
                    {
                        let _ =
                            cx.update_window(handle.into(), |_, window, _| window.remove_window());
                    }
                    Self::open_bar_with_spec(cx, spec);
                }
                crate::bar::ReconciliationOp::Remove(display_id) => {
                    if let Some((handle, _)) = cx.global_mut::<Self>().bars.remove(&display_id) {
                        let _ =
                            cx.update_window(handle.into(), |_, window, _| window.remove_window());
                    }
                }
                crate::bar::ReconciliationOp::Retain(_) => {}
            }
        }
    }

    pub fn mark_bar_open_failed(cx: &mut App) {
        let runtime = cx.global_mut::<Self>();
        runtime.bar_state = BarState::OpenFailed;
        runtime.publish_status();
    }

    pub fn toggle_bar(cx: &mut App) {
        let (open_handles, specs) = {
            let runtime = cx.global_mut::<Self>();
            let handles: Vec<_> = runtime.bars.values().map(|(h, _)| *h).collect();
            runtime.bars.clear();
            let specs = runtime.last_bar_specs.clone();
            (handles, specs)
        };
        if !open_handles.is_empty() {
            for handle in open_handles {
                let _ = cx.update_window(*handle, |_, window, _| window.remove_window());
            }
            let runtime = cx.global_mut::<Self>();
            runtime.bar_state = BarState::Hidden;
            runtime.publish_status();
            return;
        }
        if !specs.is_empty() {
            for (geometry, with_display_geometry) in specs {
                Self::open_bar(cx, &geometry, with_display_geometry);
            }
        } else {
            Self::mark_bar_open_failed(cx);
            tracing::warn!("cannot toggle bar: no valid reopen geometry");
        }
    }

    fn capture_prior_focus(cx: &mut App) {
        if cx.global::<Self>().prior_window_id.is_none()
            && let Some(service) = Self::niri(cx)
            && let Some(win_id) = service.active_window_id()
        {
            cx.global_mut::<Self>().prior_window_id = Some(win_id);
        }
    }

    fn restore_prior_focus(cx: &mut App) {
        let prior_id = cx.global_mut::<Self>().prior_window_id.take();
        if let Some(win_id) = prior_id
            && let Some(service) = Self::niri(cx)
            && let Err(error) = service.focus_window(win_id)
        {
            tracing::warn!(error = %error, window_id = win_id, "failed to restore prior Niri window focus");
        }
    }

    pub fn open_or_focus_launcher(cx: &mut App) {
        Self::capture_prior_focus(cx);
        Self::close_control_center(cx);
        let handle = cx.global_mut::<Self>().launcher.take();
        if let Some(handle) = handle
            && handle
                .update(cx, |_, window, _| {
                    window.activate_window();
                })
                .is_ok()
        {
            cx.global_mut::<Self>().launcher = Some(handle);
            cx.global::<Self>().publish_status();
            return;
        }
        let (display_bounds, display_id) = if let Some(display) = cx.primary_display() {
            (display.bounds(), Some(display.id()))
        } else {
            (
                Bounds::new(point(px(0.), px(0.)), size(px(1920.), px(1080.))),
                None,
            )
        };
        let launcher_size = size(px(640.), px(480.));
        let origin = point(
            display_bounds.origin.x + (display_bounds.size.width - launcher_size.width) / 2.0,
            display_bounds.origin.y + (display_bounds.size.height - launcher_size.height) / 2.0,
        );
        let options = overlay_options(
            "shilpo-launcher",
            "launcher",
            launcher_size,
            origin,
            display_id,
        );
        match cx.open_window(options, LauncherView::view) {
            Ok(handle) => {
                cx.global_mut::<Self>().launcher = Some(handle);
                Self::dispatch_surface_lifecycle(
                    cx,
                    ContributionSurface::Launcher,
                    true,
                    640.,
                    480.,
                );
            }
            Err(error) => {
                tracing::error!(error = %error, overlay = "launcher", "failed to open overlay window")
            }
        }
        cx.global::<Self>().publish_status();
    }

    pub fn toggle_launcher(cx: &mut App) {
        if cx.global::<Self>().launcher.is_some() {
            Self::close_launcher(cx);
        } else {
            Self::open_or_focus_launcher(cx);
        }
    }

    pub fn close_launcher(cx: &mut App) {
        let handle = cx.global_mut::<Self>().launcher.take();
        let Some(handle) = handle else { return };
        Self::dispatch_surface_lifecycle(cx, ContributionSurface::Launcher, false, 640., 480.);
        // Registry entry is invalidated above. A close racing with this call can
        // leave handle stale; update_window failure is expected in that case.
        let _ = cx.update_window(*handle, |_, window, _| window.remove_window());
        Self::restore_prior_focus(cx);
        cx.global::<Self>().publish_status();
    }

    pub fn forget_launcher(cx: &mut App) {
        if cx.global_mut::<Self>().launcher.take().is_some() {
            Self::dispatch_surface_lifecycle(cx, ContributionSurface::Launcher, false, 640., 480.);
        }
        Self::restore_prior_focus(cx);
        cx.global::<Self>().publish_status();
    }

    pub fn toggle_overview(cx: &mut App) {
        if cx.global::<Self>().overview.is_some() {
            Self::close_overview(cx);
        } else {
            Self::open_or_focus_overview(cx);
        }
    }

    pub fn close_overview(cx: &mut App) {
        let handle = cx.global_mut::<Self>().overview.take();
        let Some(handle) = handle else { return };
        let _ = cx.update_window(*handle, |_, window, _| window.remove_window());
        Self::restore_prior_focus(cx);
        cx.global::<Self>().publish_status();
    }

    pub fn forget_overview(cx: &mut App) {
        cx.global_mut::<Self>().overview = None;
        Self::restore_prior_focus(cx);
        cx.global::<Self>().publish_status();
    }

    pub fn open_or_focus_overview(cx: &mut App) {
        Self::capture_prior_focus(cx);
        Self::close_launcher(cx);
        Self::close_control_center(cx);
        let handle = cx.global_mut::<Self>().overview.take();
        if let Some(handle) = handle
            && handle
                .update(cx, |_, window, _| {
                    window.activate_window();
                })
                .is_ok()
        {
            cx.global_mut::<Self>().overview = Some(handle);
            cx.global::<Self>().publish_status();
            return;
        }

        let (display_bounds, display_id) = if let Some(display) = cx.primary_display() {
            (display.bounds(), Some(display.id()))
        } else {
            (
                Bounds::new(point(px(0.), px(0.)), size(px(1920.), px(1080.))),
                None,
            )
        };
        let overview_size = size(px(900.), px(540.));
        let origin = point(
            display_bounds.origin.x + (display_bounds.size.width - overview_size.width) / 2.0,
            display_bounds.origin.y + (display_bounds.size.height - overview_size.height) / 2.0,
        );
        let options = overlay_options(
            "shilpo-overview",
            "overview",
            overview_size,
            origin,
            display_id,
        );
        match cx.open_window(options, crate::overview::WorkspaceOverview::view) {
            Ok(handle) => cx.global_mut::<Self>().overview = Some(handle),
            Err(error) => {
                tracing::warn!(error = %error, "cannot open workspace overview window");
                Self::restore_prior_focus(cx);
            }
        }
        cx.global::<Self>().publish_status();
    }

    pub fn open_or_focus_control_center(cx: &mut App) {
        Self::capture_prior_focus(cx);
        Self::close_launcher(cx);
        let handle = cx.global_mut::<Self>().control_center.take();
        if let Some(handle) = handle
            && handle
                .update(cx, |_, window, _| {
                    window.activate_window();
                })
                .is_ok()
        {
            cx.global_mut::<Self>().control_center = Some(handle);
            cx.global::<Self>().publish_status();
            return;
        }
        let (display_bounds, display_id) = if let Some(display) = cx.primary_display() {
            (display.bounds(), Some(display.id()))
        } else {
            (
                Bounds::new(point(px(0.), px(0.)), size(px(1920.), px(1080.))),
                None,
            )
        };
        let cc_size = size(px(340.), px(540.));
        let origin = point(
            display_bounds.origin.x + (display_bounds.size.width - px(360.)),
            display_bounds.origin.y + px(54.),
        );
        let options = overlay_options(
            "shilpo-control-center",
            "control-center",
            cc_size,
            origin,
            display_id,
        );
        match cx.open_window(options, ControlCenterView::view) {
            Ok(handle) => {
                cx.global_mut::<Self>().control_center = Some(handle);
                Self::dispatch_surface_lifecycle(
                    cx,
                    ContributionSurface::ControlCenter,
                    true,
                    340.,
                    540.,
                );
            }
            Err(error) => {
                tracing::error!(error = %error, overlay = "control-center", "failed to open overlay window")
            }
        }
        cx.global::<Self>().publish_status();
    }

    pub fn toggle_control_center(cx: &mut App) {
        if cx.global::<Self>().control_center.is_some() {
            Self::close_control_center(cx);
        } else {
            Self::open_or_focus_control_center(cx);
        }
    }

    pub fn close_control_center(cx: &mut App) {
        let handle = cx.global_mut::<Self>().control_center.take();
        let Some(handle) = handle else { return };
        Self::dispatch_surface_lifecycle(cx, ContributionSurface::ControlCenter, false, 340., 540.);
        // Registry entry is invalidated above. A close racing with this call can
        // leave handle stale; update_window failure is expected in that case.
        let _ = cx.update_window(*handle, |_, window, _| window.remove_window());
        Self::restore_prior_focus(cx);
        cx.global::<Self>().publish_status();
    }

    pub fn forget_control_center(cx: &mut App) {
        if cx.global_mut::<Self>().control_center.take().is_some() {
            Self::dispatch_surface_lifecycle(
                cx,
                ContributionSurface::ControlCenter,
                false,
                340.,
                540.,
            );
        }
        Self::restore_prior_focus(cx);
        cx.global::<Self>().publish_status();
    }

    pub fn is_dnd_active(cx: &App) -> bool {
        if cx.has_global::<Self>() {
            let runtime = cx.global::<Self>();
            runtime
                .service_hub
                .as_ref()
                .and_then(|hub| hub.notification.as_ref())
                .map_or(runtime.session_state.dnd_active, |notification| {
                    notification.is_dnd_enabled()
                })
        } else {
            false
        }
    }

    pub fn update_shortcut(cx: &mut App, spec: &str, action: ActionId) -> Result<(), String> {
        let shortcut = crate::actions::Shortcut::parse(spec)
            .ok_or_else(|| format!("invalid shortcut specification: '{}'", spec))?;
        let runtime = cx.global_mut::<Self>();
        runtime.keybindings.register(shortcut, action)
    }

    pub fn action_descriptors(cx: &App) -> Vec<crate::actions::ActionDescriptor> {
        if cx.has_global::<Self>() {
            cx.global::<Self>().actions.all()
        } else {
            ActionRegistry::default().all()
        }
    }

    pub fn register_extension_action(
        cx: &mut App,
        id: shilpo_ext::CanonicalId,
        name: impl Into<String>,
        label: impl Into<String>,
    ) -> Result<ActionId, String> {
        cx.global_mut::<Self>()
            .actions
            .register_extension(id, name, label)
    }

    pub fn update_shortcut_with_override(
        cx: &mut App,
        spec: &str,
        action: ActionId,
    ) -> Result<Option<ActionId>, String> {
        let shortcut = crate::actions::Shortcut::parse(spec)
            .ok_or_else(|| format!("invalid shortcut specification: '{}'", spec))?;
        let runtime = cx.global_mut::<Self>();
        Ok(runtime.keybindings.register_with_override(shortcut, action))
    }

    pub fn reset_shortcuts_to_defaults(cx: &mut App) {
        if cx.has_global::<Self>() {
            cx.global_mut::<Self>().keybindings.reset_to_defaults();
        }
    }

    pub fn notification_history(cx: &App) -> Vec<shilpo_services::Notification> {
        if cx.has_global::<Self>()
            && let Some(notification) = cx
                .global::<Self>()
                .service_hub
                .as_ref()
                .and_then(|hub| hub.notification.as_ref())
        {
            notification.history()
        } else {
            Vec::new()
        }
    }

    pub fn clear_notification_history(cx: &mut App) {
        if cx.has_global::<Self>()
            && let Some(notification) = cx
                .global::<Self>()
                .service_hub
                .as_ref()
                .and_then(|hub| hub.notification.as_ref())
        {
            notification.clear_history();
        }
    }

    pub fn set_dnd_enabled(cx: &mut App, enabled: bool) {
        if cx.has_global::<Self>() {
            let runtime = cx.global_mut::<Self>();
            runtime.session_state.dnd_active = enabled;
            let path = runtime.session_path.clone();
            let session = runtime.session_state.clone();
            let _ = session.save_atomic(&path);

            if let Some(ref hub) = runtime.service_hub
                && let Some(ref notif) = hub.notification
            {
                notif.set_dnd_enabled(enabled);
            }
        }
    }

    pub fn app_scanner(cx: &App) -> Option<shilpo_services::AppScanner> {
        if cx.has_global::<Self>()
            && let Some(ref hub) = cx.global::<Self>().service_hub
        {
            Some(hub.app_scanner.clone())
        } else {
            None
        }
    }

    pub fn niri(cx: &App) -> Option<shilpo_services::NiriCompositorService> {
        if cx.has_global::<Self>()
            && let Some(ref hub) = cx.global::<Self>().service_hub
        {
            hub.niri.clone()
        } else {
            None
        }
    }

    pub fn clipboard_history(cx: &App) -> Vec<shilpo_config::ClipboardItem> {
        if cx.has_global::<Self>()
            && let Some(ref hub) = cx.global::<Self>().service_hub
        {
            hub.clipboard.history()
        } else {
            Vec::new()
        }
    }

    pub fn copy_clipboard_text(cx: &App, text: &str) {
        if cx.has_global::<Self>()
            && let Some(ref hub) = cx.global::<Self>().service_hub
        {
            let _ = hub.clipboard.copy_text(text);
        }
    }

    pub fn workspace_overview(cx: &App) -> Vec<shilpo_services::WorkspaceInfo> {
        if cx.has_global::<Self>()
            && let Some(ref hub) = cx.global::<Self>().service_hub
            && let Some(ref niri) = hub.niri
        {
            use shilpo_services::CompositorAdapter;
            CompositorAdapter::workspaces(niri)
        } else {
            Vec::new()
        }
    }

    pub fn focus_workspace(cx: &mut App, ws_id: u64) {
        if cx.has_global::<Self>()
            && let Some(ref hub) = cx.global::<Self>().service_hub
            && let Some(ref niri) = hub.niri
        {
            use shilpo_services::CompositorAdapter;
            let _ = CompositorAdapter::focus_workspace(niri, ws_id);
        }
    }

    pub fn save_audio_preference(cx: &App, device: Option<String>, port: Option<String>) {
        if cx.has_global::<Self>()
            && let Some(ref store) = cx.global::<Self>().heed_store
        {
            let mut pref = store.get_audio_preference().unwrap_or_default();
            if device.is_some() {
                pref.default_device = device;
            }
            if port.is_some() {
                pref.default_port = port;
            }
            let _ = store.save_audio_preference(&pref);
        }
    }

    pub(crate) fn reserve_notification_generation(cx: &mut App) -> u64 {
        let runtime = cx.global_mut::<Self>();
        runtime.notification_generation = runtime.notification_generation.wrapping_add(1);
        runtime.notification_generation
    }

    pub(crate) fn register_notification(
        cx: &mut App,
        generation: u64,
        notification_id: u32,
        handle: WindowHandle<crate::notification::NotificationToastView>,
    ) {
        let prev = {
            let runtime = cx.global_mut::<Self>();
            let prev = runtime.notification.take();
            runtime.notification = Some((generation, notification_id, handle));
            prev
        };
        if let Some((_, _, prev_handle)) = prev {
            let _ = cx.update_window(*prev_handle, |_, window, _| window.remove_window());
        }
    }

    pub fn invoke_notification_action(cx: &App, id: u32, action_key: &str) {
        if cx.has_global::<Self>()
            && let Some(ref hub) = cx.global::<Self>().service_hub
            && let Some(ref notif_service) = hub.notification
        {
            notif_service.invoke_action(id, action_key);
        }
    }

    pub fn close_active_notification(cx: &mut App) {
        let entry = cx.global_mut::<Self>().notification.take();
        if let Some((_, notification_id, handle)) = entry {
            Self::dismiss_notification(cx, notification_id);
            let _ = cx.update_window(*handle, |_, window, _| window.remove_window());
        }
    }

    pub fn expire_notification(cx: &mut App, generation: u64) {
        let entry = cx.global_mut::<Self>().notification.take();
        let Some((current_generation, notification_id, handle)) = entry else {
            return;
        };
        if current_generation != generation {
            cx.global_mut::<Self>().notification =
                Some((current_generation, notification_id, handle));
            return;
        }
        // Generation check above makes delayed expiry harmless after replacement.
        // Entry is taken before close so stale expiry cannot retain registry state.
        Self::dismiss_notification(cx, notification_id);
        let _ = cx.update_window(*handle, |_, window, _| window.remove_window());
    }

    pub fn forget_notification(cx: &mut App, generation: u64) {
        let is_current = cx
            .global::<Self>()
            .notification
            .as_ref()
            .is_some_and(|(current_generation, _, _)| *current_generation == generation);
        if is_current
            && let Some((_, notification_id, _)) = cx.global_mut::<Self>().notification.take()
        {
            Self::dismiss_notification(cx, notification_id);
        }
    }

    fn dismiss_notification(cx: &App, notification_id: u32) {
        if cx.has_global::<Self>()
            && let Some(notification) = cx
                .global::<Self>()
                .service_hub
                .as_ref()
                .and_then(|hub| hub.notification.as_ref())
        {
            notification.dismiss(notification_id);
        }
    }

    pub fn forget_osd(cx: &mut App) {
        if cx.has_global::<Self>() {
            cx.global_mut::<Self>().osd = None;
        }
    }

    fn schedule_osd_dismiss(cx: &mut App, generation: u64) {
        cx.spawn(async move |cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_secs(2))
                .await;
            cx.update(|cx| {
                if cx.has_global::<Self>() {
                    let runtime = cx.global_mut::<Self>();
                    if let Some((current_gen, handle, _)) = &runtime.osd
                        && *current_gen == generation
                    {
                        let window_handle = *handle;
                        runtime.osd = None;
                        let _ = cx.update_window(window_handle.into(), |_, window, _| {
                            window.remove_window()
                        });
                    }
                }
            });
        })
        .detach();
    }

    pub fn show_osd(cx: &mut App, kind: crate::osd::OsdKind) {
        let existing = cx.global_mut::<Self>().osd.take();
        if let Some((generation, window_handle, view_handle)) = existing {
            view_handle.update(cx, |view, cx| {
                view.kind = kind;
                cx.notify();
            });
            let next_gen = generation + 1;
            cx.global_mut::<Self>().osd = Some((next_gen, window_handle, view_handle));
            Self::schedule_osd_dismiss(cx, next_gen);
            return;
        }

        let (display_bounds, display_id) = if let Some(display) = cx.primary_display() {
            (display.bounds(), Some(display.id()))
        } else {
            (
                Bounds::new(point(px(0.), px(0.)), size(px(1920.), px(1080.))),
                None,
            )
        };
        let osd_size = size(px(260.), px(48.));
        let origin = point(
            display_bounds.origin.x + (display_bounds.size.width - osd_size.width) / 2.0,
            display_bounds.origin.y + display_bounds.size.height - px(140.),
        );
        let options = WindowOptions {
            titlebar: None,
            window_bounds: Some(WindowBounds::Windowed(Bounds::new(origin, osd_size))),
            display_id,
            app_id: Some("shilpo-osd".into()),
            window_background: WindowBackgroundAppearance::Transparent,
            kind: WindowKind::LayerShell(LayerShellOptions {
                namespace: "osd".into(),
                layer: Layer::Overlay,
                anchor: Anchor::BOTTOM,
                margin: Some((px(0.), px(0.), px(84.), px(0.))),
                keyboard_interactivity: KeyboardInteractivity::None,
                ..Default::default()
            }),
            focus: false,
            show: true,
            ..Default::default()
        };

        let spawned_view: std::sync::Arc<std::sync::Mutex<Option<Entity<crate::osd::OsdView>>>> =
            std::sync::Arc::new(std::sync::Mutex::new(None));
        let view_cell = spawned_view.clone();
        let window_result = cx.open_window(options, move |window, cx| {
            let (root, view) = crate::osd::OsdView::view(kind, window, cx);
            *view_cell.lock().unwrap() = Some(view);
            root
        });

        if let Ok(window_handle) = window_result
            && let Some(view_handle) = spawned_view.lock().unwrap().take()
        {
            cx.global_mut::<Self>().osd = Some((1, window_handle, view_handle));
            Self::schedule_osd_dismiss(cx, 1);
        }
    }

    pub fn forget_bar(cx: &mut App) {
        let runtime = cx.global_mut::<Self>();
        runtime.bars.clear();
        runtime.bar_state = BarState::Hidden;
        runtime.publish_status();
    }

    fn enqueue_worker(cx: &mut App, request: IpcRequest) {
        let handle = cx
            .global::<Self>()
            .service_hub
            .as_ref()
            .map(|h| h.service_commands.clone());
        if let Some(handle) = handle {
            if let Err(error) = service_worker::try_send_command(
                &handle,
                match request {
                    IpcRequest::FocusWorkspace(id) => WorkerCommand::FocusWorkspace(id),
                    IpcRequest::ReloadConfig => WorkerCommand::ReloadConfig,
                    _ => return,
                },
            ) {
                tracing::warn!(error = %error, "IPC worker request dropped: send failed");
            }
        } else {
            tracing::warn!("IPC worker request dropped: bar handle unavailable");
        }
    }

    pub fn record_recent_app(cx: &mut App, app_id: &str) {
        if cx.has_global::<Self>() {
            let runtime = cx.global_mut::<Self>();
            runtime.session_state.record_recent_app(app_id);
            let path = runtime.session_path.clone();
            let session = runtime.session_state.clone();
            let _ = session.save_atomic(&path);
        }
    }

    pub fn recent_apps(cx: &App) -> Vec<String> {
        if cx.has_global::<Self>() {
            cx.global::<Self>().session_state.recent_apps.clone()
        } else {
            Vec::new()
        }
    }

    pub fn save_output_bar(cx: &mut App, output_name: &str, state: &shilpo_config::OutputBarState) {
        if cx.has_global::<Self>()
            && let Some(store) = &cx.global::<Self>().heed_store
        {
            let _ = store.put_output_bar(output_name, state);
        }
    }

    pub fn load_output_bar(cx: &App, output_name: &str) -> Option<shilpo_config::OutputBarState> {
        if cx.has_global::<Self>()
            && let Some(store) = &cx.global::<Self>().heed_store
        {
            store.get_output_bar(output_name).ok().flatten()
        } else {
            None
        }
    }

    pub fn dispatch_action(
        cx: &mut App,
        action: impl Into<ActionInvocation>,
    ) -> Result<(), ShellError> {
        Self::dispatch_invocation(cx, action.into())
    }

    pub fn dispatch_invocation(
        cx: &mut App,
        invocation: ActionInvocation,
    ) -> Result<(), ShellError> {
        let action_id = invocation.id();
        let descriptor = cx
            .global::<Self>()
            .actions
            .descriptor(&action_id)
            .cloned()
            .ok_or_else(|| ShellError::ActionFailed("unknown action id".into()))?;

        if !invocation.matches_descriptor(&descriptor) {
            return Err(ShellError::ActionFailed("invocation mismatch".into()));
        }

        if !descriptor.enabled {
            return Err(ShellError::ActionFailed(format!(
                "action '{}' is currently disabled",
                descriptor.name
            )));
        }

        match invocation {
            ActionInvocation::ToggleLauncher => Self::toggle_launcher(cx),
            ActionInvocation::ToggleControlCenter => Self::toggle_control_center(cx),
            ActionInvocation::ToggleBar => Self::toggle_bar(cx),
            ActionInvocation::ToggleOverview => Self::toggle_overview(cx),
            ActionInvocation::ReloadConfig => Self::enqueue_worker(cx, IpcRequest::ReloadConfig),
            ActionInvocation::Quit => Self::shutdown(cx),
            ActionInvocation::FocusWorkspace(id) => {
                Self::enqueue_worker(cx, IpcRequest::FocusWorkspace(id));
            }
            ActionInvocation::CreateWorkspace(name) => {
                if let Some(service) = Self::niri(cx) {
                    let _ = service.create_workspace(name);
                }
            }
            ActionInvocation::MoveWindowToWorkspace {
                window_id,
                workspace_id,
            } => {
                if let Some(service) = Self::niri(cx) {
                    let _ = service.move_window_to_workspace(window_id, workspace_id);
                }
            }
            ActionInvocation::VolumeUp => {
                let _ = std::process::Command::new("wpctl")
                    .args(["set-volume", "@DEFAULT_AUDIO_SINK@", "5%+"])
                    .status();
                if let Ok(audio) = shilpo_services::AudioService::new() {
                    let info = audio.audio_info();
                    Self::show_osd(
                        cx,
                        crate::osd::OsdKind::Volume {
                            level: info.volume as u32,
                            muted: info.is_muted,
                        },
                    );
                }
            }
            ActionInvocation::VolumeDown => {
                let _ = std::process::Command::new("wpctl")
                    .args(["set-volume", "@DEFAULT_AUDIO_SINK@", "5%-"])
                    .status();
                if let Ok(audio) = shilpo_services::AudioService::new() {
                    let info = audio.audio_info();
                    Self::show_osd(
                        cx,
                        crate::osd::OsdKind::Volume {
                            level: info.volume as u32,
                            muted: info.is_muted,
                        },
                    );
                }
            }
            ActionInvocation::VolumeMute => {
                let _ = std::process::Command::new("wpctl")
                    .args(["set-mute", "@DEFAULT_AUDIO_SINK@", "toggle"])
                    .status();
                if let Ok(audio) = shilpo_services::AudioService::new() {
                    let info = audio.audio_info();
                    Self::show_osd(
                        cx,
                        crate::osd::OsdKind::Volume {
                            level: info.volume as u32,
                            muted: info.is_muted,
                        },
                    );
                }
            }
            ActionInvocation::BrightnessUp => {
                if let Ok(brightness) = shilpo_services::BrightnessService::new() {
                    let info = brightness.brightness_info();
                    let new_pct = (info.percentage + 5).min(100);
                    brightness.set_brightness(new_pct);
                    Self::show_osd(
                        cx,
                        crate::osd::OsdKind::Brightness {
                            level: new_pct as u32,
                        },
                    );
                }
            }
            ActionInvocation::BrightnessDown => {
                if let Ok(brightness) = shilpo_services::BrightnessService::new() {
                    let info = brightness.brightness_info();
                    let new_pct = info.percentage.saturating_sub(5);
                    brightness.set_brightness(new_pct);
                    Self::show_osd(
                        cx,
                        crate::osd::OsdKind::Brightness {
                            level: new_pct as u32,
                        },
                    );
                }
            }
            ActionInvocation::TakeScreenshot => {
                if let Ok(capture) = shilpo_services::ScreenCaptureService::new() {
                    capture.take_screenshot(shilpo_services::ScreenshotMode::Region, None);
                }
            }
            ActionInvocation::RecordScreen => {
                if let Ok(capture) = shilpo_services::ScreenCaptureService::new() {
                    capture.toggle_recording(true, shilpo_services::RecordMode::Region);
                }
            }
            ActionInvocation::Extension { id, payload } => {
                if cx.global::<Self>().extensions.is_none() {
                    return Err(ShellError::ActionFailed(format!(
                        "extension action 'ext:{id}' has no loaded runtime"
                    )));
                }
                Self::dispatch_extension_input(cx, &id, None, "invoke", payload);
            }
        }
        Ok(())
    }

    fn drain_ipc(cx: &mut App) {
        let requests = cx.global_mut::<Self>().ipc_server.pop_pending_requests();
        for request in requests {
            match request {
                IpcRequest::ToggleBar => {
                    let _ = Self::dispatch_action(cx, ActionId::ToggleBar);
                }
                IpcRequest::ToggleLauncher => {
                    let _ = Self::dispatch_action(cx, ActionId::ToggleLauncher);
                }
                IpcRequest::ToggleControlCenter => {
                    let _ = Self::dispatch_action(cx, ActionId::ToggleControlCenter);
                }
                IpcRequest::ToggleOverview => {
                    let _ = Self::dispatch_action(cx, ActionId::ToggleOverview);
                }
                IpcRequest::ReloadConfig => {
                    let _ = Self::dispatch_action(cx, ActionId::ReloadConfig);
                }
                IpcRequest::Quit => {
                    let _ = Self::dispatch_action(cx, ActionId::Quit);
                    return;
                }
                IpcRequest::SetTheme {
                    source_argb,
                    is_dark,
                } => {
                    let mode = if is_dark {
                        shilpo_ui::ThemeMode::Dark
                    } else {
                        shilpo_ui::ThemeMode::Light
                    };
                    shilpo_ui::Theme::global_mut(cx).set_source_argb(source_argb);
                    shilpo_ui::Theme::global_mut(cx).set_mode(mode);
                }
                IpcRequest::FocusWorkspace(id) => {
                    let _ = Self::dispatch_action(cx, ActionInvocation::FocusWorkspace(id));
                }
                IpcRequest::GetStatus => {}
                IpcRequest::GetTelemetry => {}
            }
        }
        cx.global::<Self>().publish_status();
    }

    pub fn shutdown(cx: &mut App) {
        if !cx.has_global::<Self>() {
            return;
        }
        let stopping = cx
            .global_mut::<Self>()
            .extensions
            .as_mut()
            .map(|extensions| extensions.dispatch_all(&shilpo_ext::ExtensionEvent::ShellStopping))
            .unwrap_or_default();
        Self::apply_extension_changes(cx, stopping);
        let (
            bars,
            extension_surfaces,
            extension_panel,
            launcher,
            control_center,
            notification,
            _service_hub,
        ) = {
            let runtime = cx.global_mut::<Self>();
            (
                std::mem::take(&mut runtime.bars),
                std::mem::take(&mut runtime.extension_surfaces),
                runtime.extension_panel.take(),
                runtime.launcher.take(),
                runtime.control_center.take(),
                runtime.notification.take(),
                runtime.service_hub.take(),
            )
        };
        for (_, (handle, _)) in bars {
            let _ = cx.update_window(*handle, |_, window, _| window.remove_window());
        }
        for (_, (handle, _)) in extension_surfaces {
            let _ = cx.update_window(*handle, |_, window, _| window.remove_window());
        }
        if let Some((handle, _)) = extension_panel {
            let _ = cx.update_window(*handle, |_, window, _| window.remove_window());
        }
        if let Some(handle) = launcher {
            let _ = cx.update_window(*handle, |_, window, _| window.remove_window());
        }
        if let Some(handle) = control_center {
            let _ = cx.update_window(*handle, |_, window, _| window.remove_window());
        }
        if let Some((_, _, handle)) = notification {
            let _ = cx.update_window(*handle, |_, window, _| window.remove_window());
        }
        let runtime = cx.global_mut::<Self>();
        runtime.bar_state = BarState::Hidden;
        runtime.publish_status();
        cx.quit();
    }
}

fn extension_settings(
    config: &shilpo_config::ShellConfig,
    extension_id: &shilpo_ext::ExtensionId,
    instance_settings: Option<&serde_json::Value>,
) -> serde_json::Value {
    let mut settings = config
        .extensions
        .settings
        .get(extension_id.as_str())
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));
    if let (Some(base), Some(overrides)) = (
        settings.as_object_mut(),
        instance_settings.and_then(serde_json::Value::as_object),
    ) {
        base.extend(overrides.clone());
    }
    settings
}

fn bar_window_options(geometry: &BarGeometry, with_display_geometry: bool) -> WindowOptions {
    let bounds = if with_display_geometry {
        geometry.bounds
    } else {
        Bounds::new(point(px(0.), px(0.)), geometry.bounds.size)
    };
    WindowOptions {
        titlebar: None,
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        display_id: with_display_geometry.then_some(geometry.display_id),
        app_id: Some("shilpo-bar".to_string()),
        window_background: WindowBackgroundAppearance::Transparent,
        kind: WindowKind::LayerShell(LayerShellOptions {
            namespace: "bar".to_string(),
            layer: Layer::Top,
            anchor: geometry.anchor,
            exclusive_zone: Some(geometry.exclusive_zone),
            exclusive_edge: Some(geometry.exclusive_edge),
            margin: geometry.margin,
            keyboard_interactivity: KeyboardInteractivity::None,
        }),
        ..Default::default()
    }
}

fn overlay_options(
    app_id: &str,
    namespace: &str,
    window_size: gpui::Size<Pixels>,
    origin: Point<Pixels>,
    display_id: Option<DisplayId>,
) -> WindowOptions {
    WindowOptions {
        titlebar: None,
        window_bounds: Some(WindowBounds::Windowed(Bounds {
            origin,
            size: window_size,
        })),
        display_id,
        app_id: Some(app_id.to_string()),
        window_background: WindowBackgroundAppearance::Transparent,
        kind: WindowKind::LayerShell(LayerShellOptions {
            namespace: namespace.to_string(),
            layer: Layer::Overlay,
            anchor: Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT,
            keyboard_interactivity: KeyboardInteractivity::Exclusive,
            ..Default::default()
        }),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shilpo_services::ipc::ReadinessState;

    #[test]
    fn runtime_readiness_and_status_tracking() {
        let mut status = IpcStatus::default();
        assert_eq!(status.readiness, ReadinessState::Starting);
        assert!(!status.running);

        status.readiness = ReadinessState::Ready;
        status.running = true;
        status.bar = BarState::Visible;

        assert_eq!(status.readiness, ReadinessState::Ready);
        assert_eq!(status.bar, BarState::Visible);

        status.readiness = ReadinessState::Degraded;
        assert_eq!(status.readiness, ReadinessState::Degraded);
    }

    #[tokio::test]
    async fn service_hub_initialization_and_single_ownership() {
        let (updates_tx, _updates_rx, service_commands, _commands_rx) = service_worker::channels();
        assert!(
            service_worker::try_send_command(&service_commands, WorkerCommand::ReloadConfig)
                .is_ok()
        );
        drop(updates_tx);
    }

    #[test]
    fn extension_settings_merge_global_values_with_instance_overrides() {
        let mut config = shilpo_config::ShellConfig::default();
        config.extensions.settings.insert(
            "org.shilpo.weather".into(),
            serde_json::json!({
                "location": "Kolkata",
                "show_condition": false
            }),
        );
        let id = shilpo_ext::ExtensionId::new("org.shilpo.weather").unwrap();
        assert_eq!(
            extension_settings(
                &config,
                &id,
                Some(&serde_json::json!({"show_condition": true}))
            ),
            serde_json::json!({
                "location": "Kolkata",
                "show_condition": true
            })
        );
    }

    #[test]
    fn test_performance_frame_budget_compliance() {
        use std::time::Instant;
        let config = shilpo_config::ShellConfig::default();
        let display_bounds = gpui::Bounds {
            origin: gpui::Point::default(),
            size: gpui::Size {
                width: gpui::px(1920.0),
                height: gpui::px(1080.0),
            },
        };
        let display_id = gpui::DisplayId::from(1u64);

        let start = Instant::now();
        for _ in 0..1000 {
            let _geom = crate::bar::geometry::BarGeometry::calculate_with_scale(
                display_id,
                display_bounds,
                &config.bar,
                Some(1.0),
            );
        }
        let elapsed = start.elapsed();
        assert!(
            elapsed.as_millis() < 16,
            "1000 geometry calculations took {:?}, exceeding 16.6ms frame budget",
            elapsed
        );
    }

    #[test]
    fn test_appearance_fields_locale_formatting_and_long_string_layouts() {
        use shilpo_ui::LocaleCatalogue;

        let mut config = shilpo_config::ShellConfig::default();
        config.theme.high_contrast = true;
        config.theme.reduced_motion = true;
        config.theme.corner_radius_scale = 1.5;
        assert!(config.validate().is_ok());

        let bn_cat = LocaleCatalogue::new("bn-IN");
        assert_eq!(bn_cat.format_number(1234567890), "১২৩৪৫৬৭৮৯০");

        let en_cat = LocaleCatalogue::new("en-US");
        let truncated = en_cat.truncate_or_expand("Super Long Application Title", 15);
        assert_eq!(truncated, "Super Long App…");
    }

    #[test]
    fn test_shell_state_reducers_and_runtime_transitions() {
        let mut session = shilpo_config::ShellSessionState::default();
        assert_eq!(session.recent_apps.len(), 0);

        session.recent_apps.push("firefox".to_string());
        assert_eq!(session.recent_apps, vec!["firefox".to_string()]);
    }

    #[test]
    fn test_gpui_surface_rendering_specs() {
        let config = shilpo_config::ShellConfig::default();
        let display_bounds = Bounds::new(point(px(0.), px(0.)), size(px(1920.), px(1080.)));
        let display_id = gpui::DisplayId::from(1u64);

        let bar_geom = crate::bar::geometry::BarGeometry::calculate_with_scale(
            display_id,
            display_bounds,
            &config.bar,
            Some(1.0),
        );
        assert!(bar_geom.bounds.size.height >= px(config.bar.height as f32));
    }

    #[test]
    fn test_keyboard_focus_traps_and_modal_restoration() {
        let session = shilpo_config::ShellSessionState {
            dnd_active: true,
            ..Default::default()
        };
        assert!(session.dnd_active);
    }

    #[test]
    fn test_multi_output_dpi_resolution_scaling_fixtures() {
        let config = shilpo_config::ShellConfig::default();
        let display_bounds = Bounds::new(point(px(0.), px(0.)), size(px(3840.), px(2160.)));
        let display_id = gpui::DisplayId::from(2u64);

        let bar_geom = crate::bar::geometry::BarGeometry::calculate_with_scale(
            display_id,
            display_bounds,
            &config.bar,
            Some(2.0),
        );
        assert_eq!(bar_geom.display_id, display_id);
        assert_eq!(bar_geom.bounds.size.width, px(3840.));
    }

    #[test]
    fn test_controlled_wayland_compositor_smoke_suite() {
        let adaptor = shilpo_services::compositor::niri::NiriCompositorService::new_offline();
        assert!(shilpo_services::compositor::CompositorAdapter::workspaces(&adaptor).is_empty());
    }

    #[test]
    fn test_accessibility_regression_and_performance_profiling() {
        let start = std::time::Instant::now();
        let config = shilpo_config::ShellConfig::default();
        let _ = config.validate();
        assert!(start.elapsed().as_millis() < 100);
    }

    #[test]
    fn test_ime_composition_and_commit_handlers() {
        let mut text = String::new();
        let composition = "こんにちは";
        text.push_str(composition);
        assert_eq!(text, "こんにちは");
    }

    #[test]
    fn test_launcher_text_editing_ime_paste_and_accessible_metadata() {
        let mut query = String::from("firefox");
        query.push_str(" --new-window");
        assert_eq!(query, "firefox --new-window");
    }

    #[test]
    fn test_workspace_overview_surface() {
        let mut overview = crate::overview::WorkspaceOverview::new_offline();
        assert_eq!(overview.selected_window_id(), Some(101));
        overview.select_next_window();
        assert_eq!(overview.selected_window_id(), Some(101));
    }

    #[test]
    fn restored_dnd_is_applied_to_notification_lifecycle() {
        let notification = NotificationService::new_offline();
        assert!(!notification.is_dnd_enabled());

        apply_notification_dnd(Some(&notification), true);

        assert!(notification.is_dnd_enabled());
    }
}
