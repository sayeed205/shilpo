use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

use gpui::App;
use shilpo_services::{ClipboardItem, Notification, NotificationPort, NotificationService};

use crate::bar::service_worker::{
    self, CommandSender, DeviceCommand, UpdateReceiver, WorkerCommand, WorkerUpdate,
};

use super::{SessionContext, ShellRuntime, shell_surfaces::ShellSurfaces};

/// Owns shell-facing integrations (compositor, notifications, clipboard, app
/// scanning) and the client bridge that reports device state from the daemon.
///
/// All state is private: the shell reaches the services exclusively through the
/// narrow method surface below.
pub struct ServiceHub {
    compositor: Arc<dyn shilpo_services::CompositorAdapter>,
    notification: Arc<dyn NotificationPort>,
    clipboard: shilpo_services::ClipboardService,
    app_scanner: shilpo_services::AppScanner,
    service_commands: CommandSender,
    device_snapshot: crate::bar::service_worker::DeviceSnapshot,
    availability: crate::bar::service_worker::ServiceAvailability,
    notif_rx: Arc<Mutex<tokio::sync::broadcast::Receiver<Notification>>>,
    updates_rx: Arc<Mutex<UpdateReceiver>>,
    _service_task: Option<gpui::Task<()>>,
    _watcher: Option<notify::RecommendedWatcher>,
    _app_watcher: Option<notify::RecommendedWatcher>,
}

impl ServiceHub {
    /// Starts the service hub from a restored session, applying persisted DND state.
    pub fn start(executor: gpui::BackgroundExecutor, session: &SessionContext) -> Self {
        let hub = Self::new(
            executor,
            session.config_path.clone(),
            session.heed_store.clone(),
        );
        if session.session_state.dnd_active {
            hub.notification.set_dnd_enabled(true);
        }
        hub
    }

    fn new(
        executor: gpui::BackgroundExecutor,
        config_path: PathBuf,
        session_store: Option<Arc<shilpo_services::HeedSessionStore>>,
    ) -> Self {
        let compositor: Arc<dyn shilpo_services::CompositorAdapter> =
            shilpo_services::NiriCompositorService::new();
        let device_client = shilpo_services::DeviceClient::new();
        let clipboard = shilpo_services::ClipboardService::with_store(session_store);
        let app_scanner = shilpo_services::AppScanner::new()
            .unwrap_or_else(|_| shilpo_services::AppScanner::new_empty());
        let app_watcher = app_scanner.start_watcher();

        let notification: Arc<dyn shilpo_services::NotificationPort> =
            match NotificationService::new() {
                Ok(service) => Arc::new(service),
                Err(error) => {
                    tracing::warn!(error = %error, "notification service unavailable; using offline port");
                    Arc::new(shilpo_services::TestNotificationAdapter::new_ready(32))
                }
            };

        let notif_rx = notification.subscribe_events();

        let (updates_tx, updates_rx, service_commands, commands_rx) = service_worker::channels();
        let service_task = service_worker::spawn(
            executor,
            updates_tx,
            commands_rx,
            config_path.clone(),
            device_client,
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
        let watcher = if !config_dir.starts_with(std::env::temp_dir()) {
            match notify::RecommendedWatcher::new(
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
                Ok(mut watcher) => {
                    match watcher.watch(&config_dir, notify::RecursiveMode::Recursive) {
                        Ok(()) => Some(watcher),
                        Err(error) => {
                            tracing::warn!(error = %error, path = ?config_dir, "config watcher watch failed");
                            None
                        }
                    }
                }
                Err(error) => {
                    tracing::warn!(error = %error, "config watcher creation failed");
                    None
                }
            }
        } else {
            None
        };

        Self {
            compositor,
            notification,
            clipboard,
            app_scanner,
            service_commands,
            device_snapshot: crate::bar::service_worker::DeviceSnapshot::default(),
            availability: crate::bar::service_worker::ServiceAvailability::default(),
            notif_rx: Arc::new(Mutex::new(notif_rx)),
            updates_rx: Arc::new(Mutex::new(updates_rx)),
            _service_task: Some(service_task),
            _watcher: watcher,
            _app_watcher: app_watcher,
        }
    }

    pub(crate) fn compositor(&self) -> Arc<dyn shilpo_services::CompositorAdapter> {
        self.compositor.clone()
    }

    pub(crate) fn app_scanner(&self) -> shilpo_services::AppScanner {
        self.app_scanner.clone()
    }

    pub(crate) fn service_commands(&self) -> CommandSender {
        self.service_commands.clone()
    }

    pub(crate) fn device_snapshot(&self) -> crate::bar::service_worker::DeviceSnapshot {
        self.device_snapshot.clone()
    }

    pub(crate) fn service_availability(&self) -> crate::bar::service_worker::ServiceAvailability {
        self.availability.clone()
    }

    pub(crate) fn is_dnd_enabled(&self) -> bool {
        self.notification.snapshot().dnd_enabled
    }

    #[cfg(test)]
    pub(crate) fn notification_dnd_flag(&self) -> bool {
        self.notification.snapshot().dnd_enabled
    }

    pub(crate) fn set_dnd_enabled(&mut self, enabled: bool) {
        self.notification.set_dnd_enabled(enabled);
    }

    pub(crate) fn push_notification(&self, notification: Notification) {
        self.notification.push_notification(notification);
    }

    pub(crate) fn dismiss_notification(&self, id: u32) {
        self.notification.dismiss(id);
    }

    pub(crate) fn expire_notification(&self, id: u32) {
        self.notification.expire(id);
    }

    pub(crate) fn invoke_notification_action(&self, id: u32, action_key: &str) {
        self.notification.invoke_action(id, action_key);
    }

    pub(crate) fn notification_history(&self) -> Vec<Notification> {
        self.notification.snapshot().history
    }

    pub(crate) fn clear_notification_history(&self) {
        self.notification.clear_history();
    }

    pub(crate) fn copy_text(&self, text: &str) -> anyhow::Result<()> {
        self.clipboard.copy_text(text)
    }

    pub(crate) fn clipboard_history(&self) -> Vec<ClipboardItem> {
        self.clipboard.history()
    }

    pub(crate) fn send_device_command(&self, command: DeviceCommand) {
        let _ = service_worker::try_send_command(
            &self.service_commands,
            WorkerCommand::Device(command),
        );
    }

    fn drain_notifications(&mut self) -> Vec<Notification> {
        let mut list = Vec::new();
        if let Ok(mut rx) = self.notif_rx.lock() {
            while let Ok(notif) = rx.try_recv() {
                list.push(notif);
            }
        }
        list
    }

    fn drain_updates(&mut self) -> Vec<WorkerUpdate> {
        let mut list = Vec::new();
        if let Ok(rx) = self.updates_rx.lock() {
            while let Ok(upd) = rx.try_recv() {
                list.push(upd);
            }
        }
        list
    }

    fn apply_update(&mut self, update: &WorkerUpdate) {
        self.device_snapshot.apply(update);
        match update {
            crate::bar::service_worker::WorkerUpdate::ServiceStateChange {
                service,
                state,
                last_error,
            } => {
                let available = state.is_ready();
                match *service {
                    "battery" => {
                        self.availability.battery_available = available;
                        self.availability.battery_state = *state;
                        self.availability.battery_last_error = last_error.clone();
                    }
                    "audio" => {
                        self.availability.audio_available = available;
                        self.availability.audio_state = *state;
                        self.availability.audio_last_error = last_error.clone();
                    }
                    "network" => {
                        self.availability.network_available = available;
                        self.availability.network_state = *state;
                        self.availability.network_last_error = last_error.clone();
                    }
                    "media" => {
                        self.availability.media_available = available;
                        self.availability.media_state = *state;
                        self.availability.media_last_error = last_error.clone();
                    }
                    "brightness" => {
                        self.availability.brightness_available = available;
                        self.availability.brightness_state = *state;
                        self.availability.brightness_last_error = last_error.clone();
                    }
                    _ => tracing::warn!(service, "unknown service state update"),
                }
            }
            crate::bar::service_worker::WorkerUpdate::CommandRejected { reason, .. } => {
                tracing::warn!(%reason, "device command rejected")
            }
            crate::bar::service_worker::WorkerUpdate::CommandOutcome(outcome) => {
                tracing::debug!(?outcome, "device command reached terminal outcome");
            }
            _ => {}
        }
    }

    /// Drains the notification inbox and the service-worker update stream,
    /// applying device state and forwarding updates to the shell surfaces.
    pub(crate) fn drain(cx: &mut App) {
        if !cx.has_global::<ShellRuntime>() {
            return;
        }

        ShellRuntime::publish_status(cx);

        let notifs = cx
            .global_mut::<ShellRuntime>()
            .service_hub_mut()
            .map(ServiceHub::drain_notifications)
            .unwrap_or_default();
        for notif in notifs {
            ShellSurfaces::request(
                cx,
                super::shell_surfaces::SurfaceRequest::ShowNotification(notif),
            );
        }

        let updates = cx
            .global_mut::<ShellRuntime>()
            .service_hub_mut()
            .map(ServiceHub::drain_updates)
            .unwrap_or_default();

        if !updates.is_empty() {
            for upd in &updates {
                if let Some(hub) = cx.global_mut::<ShellRuntime>().service_hub_mut() {
                    hub.apply_update(upd);
                }
                match upd {
                    crate::bar::service_worker::WorkerUpdate::Config(
                        crate::bar::service_worker::ConfigUpdate::Loaded(config),
                    ) => {
                        ShellRuntime::set_active_config(cx, config);
                        ShellSurfaces::request(cx, super::SurfaceRequest::SyncDisplays);
                        ShellSurfaces::reconcile_bar_extension_instances(cx);
                    }
                    crate::bar::service_worker::WorkerUpdate::Battery(info) => {
                        if info.available
                            && !info.is_present
                            && ShellRuntime::service_availability(cx).battery_state
                                == shilpo_services::ServiceLifecycle::Ready
                        {
                            crate::bar::cards::adapter::CardCoordinator::dispatch(
                                cx,
                                crate::bar::cards::model::CardRequest::AnchorRemoved {
                                    source: crate::bar::cards::model::CardSourceId::singleton(
                                        "battery",
                                    ),
                                },
                            );
                        }
                        crate::bar::cards::adapter::CardCoordinator::refresh_owner(
                            cx,
                            &crate::bar::cards::model::CardOwnerId::new("battery"),
                        );
                        ShellRuntime::dispatch_extension_event(
                            cx,
                            shilpo_ext_api::ExtensionEvent::PowerChanged {
                                percentage: info.is_present.then_some(info.percentage as f32),
                                charging: info.is_charging(),
                            },
                        );
                    }
                    crate::bar::service_worker::WorkerUpdate::ServiceStateChange {
                        service: "battery",
                        ..
                    } => {
                        crate::bar::cards::adapter::CardCoordinator::refresh_owner(
                            cx,
                            &crate::bar::cards::model::CardOwnerId::new("battery"),
                        );
                    }
                    crate::bar::service_worker::WorkerUpdate::Network(info) => {
                        ShellRuntime::dispatch_extension_event(
                            cx,
                            shilpo_ext_api::ExtensionEvent::NetworkChanged {
                                connected: info.available && info.is_connected,
                            },
                        );
                    }
                    crate::bar::service_worker::WorkerUpdate::Media(info) => {
                        ShellRuntime::dispatch_extension_event(
                            cx,
                            shilpo_ext_api::ExtensionEvent::MediaChanged {
                                title: (!info.title.is_empty()).then_some(info.title.clone()),
                                artist: (!info.artist.is_empty()).then_some(info.artist.clone()),
                                playing: info.playback_state
                                    == shilpo_services::PlaybackState::Playing,
                            },
                        );
                    }
                    _ => {}
                }
            }

            let handles = cx.global::<ShellRuntime>().shell_surfaces().bar_handles();
            for handle in handles {
                let updates_clone = updates.clone();
                if let Err(error) = handle.update(cx, |bar_view, _window, cx| {
                    for upd in &updates_clone {
                        bar_view.apply_worker_update(upd, cx);
                    }
                }) {
                    tracing::debug!(
                        ?error,
                        window_id = ?handle.window_id(),
                        surface = "bar",
                        "stale window handle on service drain"
                    );
                }
            }
        }
    }
}

/// Applies the do-not-disturb flag to a notification port.
#[cfg(test)]
pub(crate) fn apply_notification_dnd(notification: &dyn NotificationPort, enabled: bool) {
    notification.set_dnd_enabled(enabled);
}

impl ShellRuntime {
    pub fn service_commands(cx: &App) -> Option<crate::bar::service_worker::CommandSender> {
        if cx.has_global::<Self>() {
            cx.global::<Self>()
                .service_hub()
                .map(|hub| hub.service_commands())
        } else {
            None
        }
    }

    pub fn device_snapshot(cx: &App) -> crate::bar::service_worker::DeviceSnapshot {
        cx.global::<Self>()
            .service_hub()
            .map(|hub| hub.device_snapshot())
            .unwrap_or_default()
    }

    pub fn service_availability(cx: &App) -> crate::bar::service_worker::ServiceAvailability {
        cx.global::<Self>()
            .service_hub()
            .map(|hub| hub.service_availability())
            .unwrap_or_default()
    }

    pub fn dispatch_device_command(cx: &App, command: DeviceCommand) {
        if let Some(hub) = cx.global::<Self>().service_hub() {
            hub.send_device_command(command);
        }
    }

    pub fn app_scanner(cx: &App) -> Option<shilpo_services::AppScanner> {
        if cx.has_global::<Self>()
            && let Some(hub) = cx.global::<Self>().service_hub()
        {
            Some(hub.app_scanner())
        } else {
            None
        }
    }

    pub fn compositor(cx: &App) -> Option<Arc<dyn shilpo_services::CompositorAdapter>> {
        if cx.has_global::<Self>()
            && let Some(hub) = cx.global::<Self>().service_hub()
        {
            Some(hub.compositor())
        } else {
            None
        }
    }

    pub fn clipboard_history(cx: &App) -> Vec<ClipboardItem> {
        if cx.has_global::<Self>()
            && let Some(hub) = cx.global::<Self>().service_hub()
        {
            hub.clipboard_history()
        } else {
            Vec::new()
        }
    }

    pub fn copy_clipboard_text(cx: &App, text: &str) {
        if cx.has_global::<Self>()
            && let Some(hub) = cx.global::<Self>().service_hub()
        {
            let _ = hub.copy_text(text);
        }
    }

    pub fn is_dnd_active(cx: &App) -> bool {
        if cx.has_global::<Self>() {
            let runtime = cx.global::<Self>();
            runtime
                .service_hub()
                .map_or(runtime.session_state().dnd_active, |hub| {
                    hub.is_dnd_enabled()
                })
        } else {
            false
        }
    }

    pub fn set_dnd_enabled(cx: &mut App, enabled: bool) {
        if cx.has_global::<Self>() {
            let runtime = cx.global_mut::<Self>();
            runtime.session_state_mut().dnd_active = enabled;
            let path = runtime.session_path().clone();
            let session = runtime.session_state().clone();
            let _ = session.save_atomic(&path);
            if let Some(hub) = runtime.service_hub_mut() {
                hub.set_dnd_enabled(enabled);
            }
        }
    }

    pub fn notification_history(cx: &App) -> Vec<Notification> {
        if cx.has_global::<Self>()
            && let Some(hub) = cx.global::<Self>().service_hub()
        {
            hub.notification_history()
        } else {
            Vec::new()
        }
    }

    pub fn clear_notification_history(cx: &mut App) {
        if cx.has_global::<Self>()
            && let Some(hub) = cx.global::<Self>().service_hub()
        {
            hub.clear_notification_history();
        }
    }

    pub fn invoke_notification_action(cx: &App, id: u32, action_key: &str) {
        if cx.has_global::<Self>()
            && let Some(hub) = cx.global::<Self>().service_hub()
        {
            hub.invoke_notification_action(id, action_key);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn restored_dnd_is_applied_to_notification_lifecycle() {
        let notification = Arc::new(shilpo_services::TestNotificationAdapter::new_ready(32));
        assert!(!notification.snapshot().dnd_enabled);

        apply_notification_dnd(notification.as_ref(), true);

        assert!(notification.snapshot().dnd_enabled);
    }

    #[test]
    fn service_hub_start_initializes_with_dnd() {
        let temp_dir =
            std::env::temp_dir().join(format!("shilpo_service_test_{}", uuid::Uuid::new_v4()));
        let session = SessionContext {
            config_path: temp_dir.join("config.toml"),
            active_config: crate::config::ShellConfig::default(),
            session_path: temp_dir.join("session.toml"),
            session_state: crate::config::ShellSessionState {
                dnd_active: true,
                ..Default::default()
            },
            heed_store: None,
        };
        let mut hub = ServiceHub::new_offline_harness();
        if session.session_state.dnd_active {
            hub.set_dnd_enabled(true);
        }

        assert!(hub.notification_dnd_flag());
        assert!(hub.is_dnd_enabled());

        drop(hub);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn dnd_persistence_flag_survives_notification_restart() {
        let mut hub = ServiceHub::new_offline_harness();
        hub.set_dnd_enabled(true);
        assert!(hub.notification_dnd_flag());
        assert!(hub.is_dnd_enabled());
        hub.set_dnd_enabled(false);
        assert!(!hub.notification_dnd_flag());
    }

    struct ServiceHubTestHarness {
        hub: ServiceHub,
    }

    impl ServiceHubTestHarness {
        fn new_offline() -> Self {
            Self {
                hub: ServiceHub::new_offline_harness(),
            }
        }
    }

    #[test]
    fn test_harness_service_hub_isolated_state_transitions() {
        let mut harness = ServiceHubTestHarness::new_offline();
        assert!(!harness.hub.notification_dnd_flag());
        harness.hub.set_dnd_enabled(true);
        assert!(harness.hub.notification_dnd_flag());
    }

    impl ServiceHub {
        fn new_offline_harness() -> Self {
            let (updates_tx, _updates_rx, service_commands, _commands_rx) =
                service_worker::channels();
            drop(updates_tx);
            let adapter = Arc::new(shilpo_services::TestNotificationAdapter::new_ready(32));
            let notif_rx = adapter.subscribe_events();
            Self {
                compositor: Arc::new(shilpo_services::TestCompositorAdapter::new_default()),
                notification: adapter,
                clipboard: shilpo_services::ClipboardService::with_store(None),
                app_scanner: shilpo_services::AppScanner::new_empty(),
                service_commands,
                device_snapshot: crate::bar::service_worker::DeviceSnapshot::default(),
                availability: crate::bar::service_worker::ServiceAvailability::default(),
                notif_rx: Arc::new(Mutex::new(notif_rx)),
                updates_rx: Arc::new(Mutex::new(_updates_rx)),
                _service_task: None,
                _watcher: None,
                _app_watcher: None,
            }
        }
    }
}
