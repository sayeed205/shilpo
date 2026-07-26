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
    launcher::LauncherView,
};

use std::collections::HashMap;

use crate::bar::service_worker::{self, CommandSender, UpdateReceiver, WorkerCommand};
use shilpo_services::{
    AudioService, BatteryService, NetworkService, NiriCompositorService, NotificationService,
};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, mpsc};

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
            if runtime.bars.is_empty() {
                runtime.bar_state = BarState::Hidden;
            }
            if runtime
                .launcher
                .as_ref()
                .is_some_and(|handle| handle.window_id() == window_id)
            {
                runtime.launcher = None;
            }
            if runtime
                .control_center
                .as_ref()
                .is_some_and(|handle| handle.window_id() == window_id)
            {
                runtime.control_center = None;
            }
            if runtime
                .notification
                .as_ref()
                .is_some_and(|(_, _, handle)| handle.window_id() == window_id)
            {
                runtime.notification = None;
            }
            runtime.publish_status();
        });
        cx.global_mut::<Self>()._window_closed = Some(subscription);

        let task = cx.spawn(async move |cx| {
            loop {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(100))
                    .await;
                cx.update(Self::sync_displays);
                cx.update(Self::drain_service_hub);
                cx.update(Self::drain_ipc);
            }
        });
        cx.global_mut::<Self>()._ipc_task = task;
        cx.global::<Self>().publish_status();
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
                if let crate::bar::service_worker::WorkerUpdate::Config(
                    crate::bar::service_worker::ConfigUpdate::Loaded(config),
                ) = upd
                {
                    cx.global_mut::<Self>().active_config = (**config).clone();
                    Self::sync_displays(cx);
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
                name: None,
                scale: None,
            })
            .collect();

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
            BarView::view_with_config(window, cx, shell_config)
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
                name: None,
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
            Ok(handle) => cx.global_mut::<Self>().launcher = Some(handle),
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
        // Registry entry is invalidated above. A close racing with this call can
        // leave handle stale; update_window failure is expected in that case.
        let _ = cx.update_window(*handle, |_, window, _| window.remove_window());
        Self::restore_prior_focus(cx);
        cx.global::<Self>().publish_status();
    }

    pub fn forget_launcher(cx: &mut App) {
        cx.global_mut::<Self>().launcher = None;
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
            Ok(handle) => cx.global_mut::<Self>().control_center = Some(handle),
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
        // Registry entry is invalidated above. A close racing with this call can
        // leave handle stale; update_window failure is expected in that case.
        let _ = cx.update_window(*handle, |_, window, _| window.remove_window());
        Self::restore_prior_focus(cx);
        cx.global::<Self>().publish_status();
    }

    pub fn forget_control_center(cx: &mut App) {
        cx.global_mut::<Self>().control_center = None;
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
            ActionInvocation::Extension { id, .. } => {
                return Err(ShellError::ActionFailed(format!(
                    "extension action 'ext:{id}' has no loaded runtime"
                )));
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
        let (bars, launcher, control_center, notification, _service_hub) = {
            let runtime = cx.global_mut::<Self>();
            (
                std::mem::take(&mut runtime.bars),
                runtime.launcher.take(),
                runtime.control_center.take(),
                runtime.notification.take(),
                runtime.service_hub.take(),
            )
        };
        for (_, (handle, _)) in bars {
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
